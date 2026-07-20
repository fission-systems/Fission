use super::*;

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
        self.decode_and_lift_with_context_override(bytes, address, None)
    }

    pub fn decode_and_lift_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
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
                bytes,
                address,
                context_override,
            ),
        }
    }

    pub fn decode_instruction_and_lift_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
    ) -> Result<(
        DecodedInstruction,
        Vec<PcodeOp>,
        u64,
        RuntimeExecutionDetails,
    )> {
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
            RuntimeFrontendStatus::ExecutableCandidate => {
                engine::decode_instruction_and_lift_with_details(
                    &self.entry,
                    self.compiled.as_ref().ok_or_else(|| {
                        anyhow!("missing compiled frontend for {}", self.entry.entry_id)
                    })?,
                    bytes,
                    address,
                    context_override,
                )
            }
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

    /// Ghidra-compatible FID (Function ID) full and specific hashes for a
    /// function's instruction extent (e.g. `DecodedPcodeFunction.instructions`).
    ///
    /// See `compiled_table::fid_hash` for the algorithm and its current
    /// limitations (no relocation-awareness yet).
    /// `resolve_register_offset` resolves a Ghidra-style register name (as
    /// carried by a decoded `BoundOperand::NamedVarnode`) to its SLEIGH
    /// register-space offset -- typically
    /// `fission_pcode::midend::cspec::RegisterModel::lookup_name`, supplied
    /// by the caller rather than depended on directly (`fission-pcode`
    /// depends on `fission-sleigh`, not the other way around).
    ///
    /// Returns `None` if the extent is too short to hash reliably (fewer
    /// than `compiled_table::fid_hash::FID_SHORT_CODE_UNIT_LIMIT` code
    /// units after skipping alignment padding) or if this frontend has no
    /// compiled-table engine available. On success, returns
    /// `(full_count, full_hash, specific_count, specific_hash)` mirroring
    /// Ghidra's `FidHashQuad` field order.
    pub fn fid_hashes(
        &self,
        extent: &[DecodedInstruction],
        resolve_register_offset: &dyn Fn(&str) -> Option<i64>,
    ) -> Option<(u16, u64, u8, u64)> {
        let compiled = self.compiled.as_ref()?;
        let hashes = spine::compiled_table::fid_hash::compute_fid_hashes(
            compiled,
            extent,
            resolve_register_offset,
        )?;
        Some((
            hashes.full_count,
            hashes.full_hash,
            hashes.specific_count,
            hashes.specific_hash,
        ))
    }

    pub fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> Result<Vec<DecodedInstruction>> {
        self.decode_window_with_context_override(bytes, address, limit, None)
    }

    pub fn decode_window_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
        initial_context_override: Option<PackedContextOverride>,
    ) -> Result<Vec<DecodedInstruction>> {
        if limit == 0 || bytes.is_empty() {
            return Ok(Vec::new());
        }

        // Pending ContextCommit overrides: address -> packed context override.
        // Populated after each instruction's globalset / ContextCommit ops.
        let mut pending_overrides: std::collections::BTreeMap<u64, PackedContextOverride> =
            std::collections::BTreeMap::new();
        let mut decoded = Vec::with_capacity(limit.min(64));
        let mut offset = 0usize;
        let mut current = address;
        while offset < bytes.len() && decoded.len() < limit {
            let remaining = &bytes[offset..];

            // Apply any pending ContextCommit overrides for this address.
            let ctx_override = match (initial_context_override, pending_overrides.get(&current)) {
                (Some(base), Some(pending)) => Some(base.merge_override(*pending)),
                (Some(base), None) => Some(base),
                (None, Some(pending)) => Some(*pending),
                (None, None) => None,
            };

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
                let entry = pending_overrides.entry(*target_addr).or_default();
                entry.merge_commit_word(*word_index, *mask_u32, *value_u32)?;
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

    pub fn decode_instruction_with_context_override(
        &self,
        bytes: &[u8],
        address: u64,
        context_override: Option<PackedContextOverride>,
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
                bytes,
                address,
            ),
        }
    }
}
