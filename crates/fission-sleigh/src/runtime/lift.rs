use super::*;

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

        let mut ops = Vec::new();
        let mut offset = 0usize;
        let mut current = entry_address;
        let mut global_seq = 0u32;
        let mut instruction_count = 0usize;
        let mut stop_reason = DecodeStopReason::InputExhausted;

        while offset < bytes.len() && instruction_count < contract.instruction_limit {
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

            for op in &mut ins_ops {
                op.seq_num = global_seq;
                global_seq = global_seq.saturating_add(1);
            }
            let terminates = ins_ops
                .last()
                .map(|op| contract.is_terminal_control_flow(op.opcode))
                .unwrap_or(false);

            ops.extend(ins_ops);
            offset = offset.saturating_add(step);
            current = current.saturating_add(decoded_len);
            instruction_count = instruction_count.saturating_add(1);

            if terminates {
                stop_reason = DecodeStopReason::TerminalControlFlow;
                break;
            }
        }

        if instruction_count >= contract.instruction_limit && offset < bytes.len() {
            stop_reason = DecodeStopReason::InstructionLimit;
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
