pub(super) mod common;

use anyhow::{anyhow, bail, Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode};

use super::{LiftDecodeContract, LiftStopReason, aarch64, x86};

#[derive(Debug, Clone)]
pub(super) struct LiftedOps {
    pub(super) ops: Vec<PcodeOp>,
    pub(super) decoded_instructions: usize,
    pub(super) stop_reason: LiftStopReason,
    pub(super) consumed_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DecodeContext<'a> {
    pub(super) insn: &'a [u8],
    pub(super) address: u64,
    pub(super) decoded_len: u64,
    pub(super) seq_start: u32,
    temp_seed: u64,
}

impl<'a> DecodeContext<'a> {
    pub(super) fn new(insn: &'a [u8], address: u64, decoded_len: u64) -> Self {
        Self {
            insn,
            address,
            decoded_len,
            seq_start: 1,
            temp_seed: address.wrapping_shl(6),
        }
    }

    fn semantic_temp_base(self, kind: BackendKind) -> u64 {
        match kind {
            BackendKind::Aarch64 => 0xC000_0000_0000_0000u64.wrapping_add(self.temp_seed),
            BackendKind::X86 => 0xE100_0000_0000_0000u64.wrapping_add(self.temp_seed),
        }
    }

    fn resequence(self, ops: &mut [PcodeOp]) {
        let mut seq = self.seq_start;
        for op in ops {
            op.seq_num = seq;
            seq = seq.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackendKind {
    Aarch64,
    X86,
}

pub(super) fn is_cfg_split_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch | PcodeOpcode::CBranch | PcodeOpcode::BranchInd | PcodeOpcode::Return
    )
}

pub(super) fn direct_control_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op
            .inputs
            .first()
            .filter(|vn| vn.is_constant)
            .map(|vn| vn.constant_val as u64),
        _ => None,
    }
}

impl BackendKind {
    pub(super) fn for_language(language_name: &str) -> Self {
        if language_name.starts_with("AARCH64") {
            Self::Aarch64
        } else {
            // Keep x86-family as the default fallback path for now.
            Self::X86
        }
    }

    pub(super) fn decode_len(self, bytes: &[u8]) -> Result<u64> {
        match self {
            Self::Aarch64 => {
                if bytes.len() < 4 {
                    bail!("AArch64 needs 4 bytes, got {}", bytes.len());
                }
                Ok(4)
            }
            Self::X86 => x86::decode_len(bytes),
        }
    }

    pub(super) fn decode_ops(self, ctx: &DecodeContext<'_>) -> Vec<PcodeOp> {
        match self {
            Self::Aarch64 => {
                let mut ops = aarch64::decode_semantic_with_state(
                    ctx.insn,
                    ctx.address,
                    ctx.seq_start,
                    ctx.semantic_temp_base(self),
                );
                let has_cf = ops.iter().any(|op| {
                    matches!(
                        op.opcode,
                        PcodeOpcode::Branch
                            | PcodeOpcode::CBranch
                            | PcodeOpcode::BranchInd
                            | PcodeOpcode::Return
                            | PcodeOpcode::Call
                            | PcodeOpcode::CallInd
                    )
                });

                if !has_cf {
                    if let Some(mut flow) = aarch64::decode_control(ctx.insn, ctx.address) {
                        ops.append(&mut flow);
                    }
                }

                ctx.resequence(&mut ops);

                ops
            }
            Self::X86 => {
                let mut ops = x86::decode_semantic_with_state(
                    ctx.insn,
                    ctx.address,
                    ctx.seq_start,
                    ctx.semantic_temp_base(self),
                );
                if let Some(mut flow) = x86::decode_control(ctx.insn, ctx.address, ctx.decoded_len)
                {
                    ops.append(&mut flow);
                }

                ctx.resequence(&mut ops);

                ops
            }
        }
    }

    pub(super) fn decode_and_lift_with_len(
        self,
        bytes: &[u8],
        address: u64,
        emit_trace_copy: fn(&[u8], u64) -> PcodeOp,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        if bytes.is_empty() {
            bail!("No instruction bytes available at 0x{:x}", address);
        }

        let decoded_len = self.decode_len(bytes)?;
        let decoded_len_usize =
            usize::try_from(decoded_len).context("decoded_len does not fit usize")?;
        let insn = &bytes[..decoded_len_usize];
        let decode_ctx = DecodeContext::new(insn, address, decoded_len);

        let mut ops = Vec::with_capacity(8);
        ops.push(emit_trace_copy(insn, address));
        let mut decoded_ops = self.decode_ops(&decode_ctx);
        ops.append(&mut decoded_ops);

        Ok((ops, decoded_len))
    }

    pub(super) fn lift_ops_with_contract(
        self,
        bytes: &[u8],
        entry_address: u64,
        contract: LiftDecodeContract,
        emit_trace_copy: fn(&[u8], u64) -> PcodeOp,
    ) -> Result<LiftedOps> {
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
        let mut stop_reason = LiftStopReason::InputExhausted;

        while offset < bytes.len() && instruction_count < contract.instruction_limit {
            let remaining = &bytes[offset..];
            let (mut ins_ops, decoded_len) = self
                .decode_and_lift_with_len(remaining, current, emit_trace_copy)
                .map_err(|err| anyhow!("decode failed at 0x{:x}: {:#}", current, err))?;

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }

            let step = usize::try_from(decoded_len).context("decoded length does not fit usize")?;
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
                stop_reason = LiftStopReason::TerminalControlFlow;
                break;
            }
        }

        if instruction_count >= contract.instruction_limit && offset < bytes.len() {
            stop_reason = LiftStopReason::InstructionLimit;
        }

        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        Ok(LiftedOps {
            ops,
            decoded_instructions: instruction_count,
            stop_reason,
            consumed_bytes: offset,
        })
    }
}
