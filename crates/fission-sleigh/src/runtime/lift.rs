use super::*;
use std::collections::{BTreeMap, VecDeque};

fn internal_byte_offset(entry_address: u64, bytes_len: usize, address: u64) -> Option<usize> {
    let rel = address.checked_sub(entry_address)?;
    let offset = usize::try_from(rel).ok()?;
    (offset < bytes_len).then_some(offset)
}

fn direct_pcode_branch_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            let target = op.inputs.first()?;
            if target.is_constant {
                if target.offset != 0 {
                    Some(target.offset)
                } else if target.constant_val >= 0 {
                    Some(target.constant_val as u64)
                } else {
                    None
                }
            } else if target.offset != 0 {
                Some(target.offset)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn enqueue_internal_target(
    queue: &mut VecDeque<u64>,
    entry_address: u64,
    bytes_len: usize,
    target: u64,
) {
    if internal_byte_offset(entry_address, bytes_len, target).is_some() && !queue.contains(&target)
    {
        queue.push_back(target);
    }
}

impl RuntimeSleighFrontend {
    pub fn lift_raw_pcode_function(
        &self,
        bytes: &[u8],
        entry_address: u64,
    ) -> Result<PcodeFunction> {
        Ok(self
            .lift_raw_pcode_function_with_contract(
                bytes,
                entry_address,
                DEFAULT_FUNCTION_INSTRUCTION_LIMIT,
            )?
            .function)
    }

    pub fn lift_raw_pcode_function_with_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        instruction_limit: usize,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract(
            bytes,
            entry_address,
            DecodeContract::strict_function(instruction_limit),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
    ) -> Result<DecodedPcodeFunction> {
        if bytes.is_empty() {
            bail!("No function bytes available at 0x{:x}", entry_address);
        }
        if contract.instruction_limit == 0 {
            bail!("instruction_limit must be > 0");
        }

        let mut decoded = BTreeMap::<u64, Vec<PcodeOp>>::new();
        let mut queue = VecDeque::from([entry_address]);
        let mut stop_reason = DecodeStopReason::InputExhausted;

        while let Some(current) = queue.pop_front() {
            if decoded.contains_key(&current) {
                continue;
            }
            if decoded.len() >= contract.instruction_limit {
                stop_reason = DecodeStopReason::InstructionLimit;
                break;
            }
            let Some(offset) = internal_byte_offset(entry_address, bytes.len(), current) else {
                continue;
            };
            let remaining = &bytes[offset..];
            let (mut ins_ops, decoded_len) = self
                .decode_and_lift_with_len(remaining, current)
                .map_err(|err| anyhow!("decode failed at 0x{:x}: {:#}", current, err))?;

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = usize::try_from(decoded_len)?;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            let terminal = ins_ops
                .last()
                .map(|op| contract.is_terminal_control_flow(op.opcode))
                .unwrap_or(false);
            let last_opcode = ins_ops.last().map(|op| op.opcode);
            let direct_target = ins_ops.last().and_then(direct_pcode_branch_target);
            let fallthrough = current.saturating_add(decoded_len);

            match last_opcode {
                Some(PcodeOpcode::Branch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                }
                Some(PcodeOpcode::CBranch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                Some(PcodeOpcode::Return) => {
                    stop_reason = DecodeStopReason::TerminalControlFlow;
                }
                Some(PcodeOpcode::BranchInd) if contract.stop_at_indirect_branch => {
                    stop_reason = DecodeStopReason::TerminalControlFlow;
                }
                _ if !terminal => {
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                _ => {}
            }

            decoded.insert(current, std::mem::take(&mut ins_ops));
        }

        let instruction_count = decoded.len();
        let mut ops = Vec::new();
        let mut global_seq = 0u32;
        for mut ins_ops in decoded.into_values() {
            for op in &mut ins_ops {
                op.seq_num = global_seq;
                global_seq = global_seq.saturating_add(1);
            }
            ops.extend(ins_ops);
        }

        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        let function = PcodeFunction {
            blocks: build_cfg_blocks(entry_address, ops),
        };
        function
            .validate()
            .map_err(|err| RuntimeSleighError::InvalidPcodeShape {
                language: self.entry.entry_id.clone(),
                reason: err.to_string(),
            })?;

        Ok(DecodedPcodeFunction {
            function,
            decoded_instructions: instruction_count,
            stop_reason,
        })
    }
}
