use super::*;
use anyhow::Context;

impl RuntimeSleighFrontend {
    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    pub fn decode_and_lift_with_details(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_and_lift_with_details(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                self.native_backend.as_ref(),
                bytes,
                address,
            ),
        }
    }

    pub fn decode_and_lift_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        let (ops, len, _) = self.decode_and_lift_with_details(bytes, address)?;
        Ok((ops, len))
    }

    pub fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        if limit == 0 || bytes.is_empty() {
            return Ok(Vec::new());
        }

        // Pending ContextCommit overrides: address → (context_bits, mask).
        // Populated after each instruction's globalset / ContextCommit ops.
        let mut pending_overrides: std::collections::BTreeMap<u64, (u64, u64)> =
            std::collections::BTreeMap::new();

        let mut decoded = Vec::with_capacity(limit.min(64));
        let mut offset = 0usize;
        let mut current = address;
        while offset < bytes.len() && decoded.len() < limit {
            let remaining = &bytes[offset..];

            // Apply any pending ContextCommit overrides for this address.
            let ctx_override = pending_overrides.get(&current).copied();

            let instruction = match self.decode_instruction_with_context_override(
                remaining,
                current,
                ctx_override,
            ) {
                Ok(instruction) => instruction,
                Err(err) if decoded.is_empty() => return Err(err),
                Err(_) => break,
            };
            if instruction.length == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = instruction.length;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            // Collect context commits from this instruction and queue them.
            for (target_addr, word_index, mask_u32, value_u32) in
                &instruction.pending_context_commits
            {
                let mask_u64 = packed_context_commit_word_to_u64(*word_index, *mask_u32)
                    .with_context(|| "merge pending context commit mask")?;
                let value_u64 = packed_context_commit_word_to_u64(*word_index, *value_u32)
                    .with_context(|| "merge pending context commit value")?;
                let entry = pending_overrides.entry(*target_addr).or_insert((0, 0));
                entry.0 = (entry.0 & !mask_u64) | (value_u64 & mask_u64);
                entry.1 |= mask_u64;
            }

            current = checked_instruction_fallthrough(current, step as u64)?;
            offset = offset
                .checked_add(step)
                .ok_or_else(|| anyhow!("decode byte offset overflowed at 0x{current:x}"))?;
            decoded.push(instruction);
        }
        Ok(decoded)
    }

    pub fn discover_direct_call_targets(
        &self,
        bytes: &[u8],
        base_address: u64,
    ) -> Result<Vec<u64>> {
        let mut targets = std::collections::BTreeSet::new();
        let mut offset = 0usize;
        let mut current = base_address;
        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            let instruction = match self.decode_instruction_with_len(remaining, current) {
                Ok(instruction) => instruction,
                Err(err) if offset == 0 => return Err(err),
                Err(_) => break,
            };
            if instruction.flow_kind == DecodedFlowKind::Call {
                if let Some(target) = instruction.direct_target {
                    targets.insert(target);
                }
            }
            if instruction.length == 0 || instruction.length > remaining.len() {
                break;
            }
            current = checked_instruction_fallthrough(current, instruction.length as u64)?;
            offset = offset.checked_add(instruction.length).ok_or_else(|| {
                anyhow!("direct-call discovery byte offset overflowed at 0x{current:x}")
            })?;
        }
        Ok(targets.into_iter().collect())
    }

    fn decode_instruction_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<(u64, u64)>,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_instruction_with_context(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                self.native_backend.as_ref(),
                bytes,
                address,
                context_override,
            ),
        }
    }

    pub(super) fn decode_instruction_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<DecodedInstruction> {
        if bytes.is_empty() {
            return Err(RuntimeSleighError::DecodeNoMatch {
                language: self.entry.entry_id.clone(),
                address,
            }
            .into());
        }
        match self.status {
            RuntimeFrontendStatus::RegisteredCompileOnly => {
                Err(RuntimeSleighError::UnsupportedGeneratedSemantic {
                    language: self.entry.entry_id.clone(),
                    status: self.status,
                }
                .into())
            }
            RuntimeFrontendStatus::ExecutableCandidate => engine::decode_instruction(
                &self.entry,
                self.compiled.as_ref().ok_or_else(|| {
                    anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                })?,
                self.native_backend.as_ref(),
                bytes,
                address,
            ),
        }
    }
}

fn packed_context_commit_word_to_u64(word_index: u32, value: u32) -> Result<u64> {
    let shift = word_index
        .checked_mul(32)
        .ok_or_else(|| anyhow!("context commit word index {word_index} shift overflows"))?;
    let shifted = u64::from(value).checked_shl(shift).ok_or_else(|| {
        anyhow!("context commit word index {word_index} exceeds packed u64 context")
    })?;
    Ok(shifted)
}

#[cfg(test)]
mod tests {
    use super::packed_context_commit_word_to_u64;

    #[test]
    fn context_commit_word_shift_fails_closed_above_packed_u64() {
        assert_eq!(
            packed_context_commit_word_to_u64(1, 0x8000_0000).expect("word 1"),
            0x8000_0000_0000_0000
        );
        assert!(
            packed_context_commit_word_to_u64(2, 1).is_err(),
            "word 2 must not wrap or silently clear in the u64 context cache"
        );
    }
}
