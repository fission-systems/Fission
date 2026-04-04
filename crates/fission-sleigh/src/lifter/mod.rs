use std::path::{Path, PathBuf};
use std::collections::{BTreeSet, HashMap};

use anyhow::{anyhow, bail, Context, Result};
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

mod aarch64;
mod common;
mod x86;

use common::UNIQUE_SPACE_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchKind {
    Aarch64,
    X86,
}

#[derive(Debug, Clone)]
pub struct SleighLifter {
    arch: ArchKind,
}

const DEFAULT_FUNCTION_INSTRUCTION_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftStopReason {
    TerminalControlFlow,
    InputExhausted,
    InstructionLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiftedPcodeFunction {
    pub function: PcodeFunction,
    pub decoded_instructions: usize,
    pub stop_reason: LiftStopReason,
}

pub fn is_terminal_control_flow(opcode: PcodeOpcode) -> bool {
    matches!(opcode, PcodeOpcode::BranchInd | PcodeOpcode::Return)
}

fn is_control_flow_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch | PcodeOpcode::CBranch | PcodeOpcode::BranchInd | PcodeOpcode::Return
    )
}

fn control_target_address(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op
            .inputs
            .first()
            .filter(|vn| vn.is_constant)
            .map(|vn| vn.constant_val as u64),
        _ => None,
    }
}

fn push_successor(successors: &mut Vec<u32>, succ: u32) {
    if !successors.contains(&succ) {
        successors.push(succ);
    }
}

pub fn build_cfg_blocks(entry_address: u64, ops: Vec<PcodeOp>) -> Vec<PcodeBasicBlock> {
    if ops.is_empty() {
        return Vec::new();
    }

    let mut addr_to_op_idx: HashMap<u64, usize> = HashMap::new();
    for (idx, op) in ops.iter().enumerate() {
        addr_to_op_idx.entry(op.address).or_insert(idx);
    }

    let mut block_starts: BTreeSet<usize> = BTreeSet::new();
    block_starts.insert(0);

    for (idx, op) in ops.iter().enumerate() {
        if is_control_flow_opcode(op.opcode) {
            if idx + 1 < ops.len() {
                block_starts.insert(idx + 1);
            }
            if let Some(target) = control_target_address(op) {
                if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                    block_starts.insert(target_idx);
                }
            }
        }
    }

    let starts: Vec<usize> = block_starts.into_iter().collect();
    let mut op_to_block = vec![0u32; ops.len()];
    for (block_idx, start) in starts.iter().enumerate() {
        let end = starts.get(block_idx + 1).copied().unwrap_or(ops.len());
        for slot in &mut op_to_block[*start..end] {
            *slot = block_idx as u32;
        }
    }

    let mut blocks = Vec::with_capacity(starts.len());
    for (block_idx, start) in starts.iter().enumerate() {
        let end = starts.get(block_idx + 1).copied().unwrap_or(ops.len());
        let mut block_ops = ops[*start..end].to_vec();
        for (local_seq, op) in block_ops.iter_mut().enumerate() {
            op.seq_num = local_seq as u32;
        }

        let mut successors = Vec::new();
        if let Some(last) = block_ops.last() {
            match last.opcode {
                PcodeOpcode::Branch => {
                    if let Some(target) = control_target_address(last) {
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        }
                    }
                }
                PcodeOpcode::CBranch => {
                    if let Some(target) = control_target_address(last) {
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        }
                    }
                    if block_idx + 1 < starts.len() {
                        push_successor(&mut successors, (block_idx + 1) as u32);
                    }
                }
                PcodeOpcode::BranchInd | PcodeOpcode::Return => {}
                _ => {
                    if block_idx + 1 < starts.len() {
                        push_successor(&mut successors, (block_idx + 1) as u32);
                    }
                }
            }
        }

        let start_address = block_ops
            .first()
            .map(|op| op.address)
            .unwrap_or(entry_address);
        blocks.push(PcodeBasicBlock {
            index: block_idx as u32,
            start_address,
            successors,
            ops: block_ops,
        });
    }

    blocks
}

impl SleighLifter {
    pub fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs/languages")
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::spec_dir().join(format!("{}.slaspec", language_name))
    }

    pub fn new_for_language(language_name: &str) -> Result<Self> {
        let spec_path = Self::spec_path_for(language_name);
        if !spec_path.exists() {
            bail!(
                "Sleigh spec not found for language '{}': {}",
                language_name,
                spec_path.display()
            );
        }

        let arch = if language_name.starts_with("AARCH64") {
            ArchKind::Aarch64
        } else {
            // Keep x86-family as the default fallback path for now.
            ArchKind::X86
        };

        Ok(Self { arch })
    }

    pub fn new(spec_path: &Path) -> Result<Self> {
        let language = spec_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow!("Invalid Sleigh spec path: {}", spec_path.display()))?;
        Self::new_for_language(language)
    }

    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    pub fn lift_raw_pcode_function(&self, bytes: &[u8], entry_address: u64) -> Result<PcodeFunction> {
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
    ) -> Result<LiftedPcodeFunction> {
        if bytes.is_empty() {
            bail!("No function bytes available at 0x{:x}", entry_address);
        }
        if instruction_limit == 0 {
            bail!("instruction_limit must be > 0");
        }

        let mut ops = Vec::new();
        let mut offset = 0usize;
        let mut current = entry_address;
        let mut global_seq = 0u32;
        let mut instruction_count = 0usize;
        let mut stop_reason = LiftStopReason::InputExhausted;

        while offset < bytes.len() && instruction_count < instruction_limit {
            let remaining = &bytes[offset..];
            let (mut ins_ops, decoded_len) = self.decode_and_lift_with_len(remaining, current).map_err(|err| {
                anyhow!("decode failed at 0x{:x}: {:#}", current, err)
            })?;

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }

            let step = usize::try_from(decoded_len)
                .context("decoded length does not fit usize")?;
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
                .map(|op| is_terminal_control_flow(op.opcode))
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

        if instruction_count >= instruction_limit && offset < bytes.len() {
            stop_reason = LiftStopReason::InstructionLimit;
        }

        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        Ok(LiftedPcodeFunction {
            function: PcodeFunction {
                blocks: build_cfg_blocks(entry_address, ops),
            },
            decoded_instructions: instruction_count,
            stop_reason,
        })
    }

    pub fn decode_and_lift_with_len(&self, bytes: &[u8], address: u64) -> Result<(Vec<PcodeOp>, u64)> {
        if bytes.is_empty() {
            bail!("No instruction bytes available at 0x{:x}", address);
        }

        let decoded_len = self.decode_len(bytes)?;
        let decoded_len_usize = usize::try_from(decoded_len).context("decoded_len does not fit usize")?;
        let insn = &bytes[..decoded_len_usize];

        let mut ops = Vec::with_capacity(8);
        ops.push(self.emit_trace_copy(insn, address));
        match self.arch {
            ArchKind::Aarch64 => {
                let mut sem = aarch64::decode_semantic(insn, address);
                let has_cf = sem.iter().any(|op| {
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
                ops.append(&mut sem);
                if !has_cf {
                    if let Some(mut flow) = self.decode_control_flow(insn, address, decoded_len)? {
                        ops.append(&mut flow);
                    }
                }
            }
            ArchKind::X86 => {
                let mut sem = x86::decode_semantic(insn, address);
                ops.append(&mut sem);
                if let Some(mut flow) = self.decode_control_flow(insn, address, decoded_len)? {
                    ops.append(&mut flow);
                }
            }
        }

        Ok((ops, decoded_len))
    }

    fn decode_len(&self, bytes: &[u8]) -> Result<u64> {
        match self.arch {
            ArchKind::Aarch64 => {
                if bytes.len() < 4 {
                    bail!("AArch64 needs 4 bytes, got {}", bytes.len());
                }
                Ok(4)
            }
            ArchKind::X86 => x86::decode_len(bytes),
        }
    }

    fn emit_trace_copy(&self, insn: &[u8], address: u64) -> PcodeOp {
        let mut raw = 0u64;
        for (idx, b) in insn.iter().take(8).enumerate() {
            raw |= (*b as u64) << (idx * 8);
        }

        let const_raw = if raw > i64::MAX as u64 {
            i64::MAX
        } else {
            raw as i64
        };

        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: 0x7000_0000_0000_0000u64.wrapping_add(address),
                size: 8,
                is_constant: false,
                constant_val: 0,
            }),
            inputs: vec![Varnode::constant(const_raw, 8)],
            asm_mnemonic: Some("INSN_RAW".to_string()),
        }
    }

    fn decode_control_flow(&self, insn: &[u8], address: u64, decoded_len: u64) -> Result<Option<Vec<PcodeOp>>> {
        match self.arch {
            ArchKind::Aarch64 => Ok(aarch64::decode_control(insn, address)),
            ArchKind::X86 => Ok(x86::decode_control(insn, address, decoded_len)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn cfg_blocks_conditional_branch_has_target_and_fallthrough() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x10, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(2, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x110, 8), Varnode::constant(1, 1)],
            ),
            op(
                2,
                0x108,
                PcodeOpcode::IntAdd,
                Some(var(0x20, 4)),
                vec![Varnode::constant(3, 4), Varnode::constant(4, 4)],
            ),
            op(3, 0x10c, PcodeOpcode::Return, None, vec![]),
            op(
                4,
                0x110,
                PcodeOpcode::IntAdd,
                Some(var(0x30, 4)),
                vec![Varnode::constant(5, 4), Varnode::constant(6, 4)],
            ),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[1].start_address, 0x108);
        assert_eq!(blocks[2].start_address, 0x110);
        assert_eq!(blocks[0].successors, vec![2, 1]);
        assert!(blocks[1].successors.is_empty());
        assert!(blocks[2].successors.is_empty());
        assert_eq!(blocks[0].ops[0].seq_num, 0);
        assert_eq!(blocks[1].ops[0].seq_num, 0);
    }

    #[test]
    fn lift_contract_reports_instruction_limit_stop() {
        let lifter = SleighLifter::new_for_language("x86-64").expect("x86-64 sleigh lifter");
        let bytes = [0x90, 0x90];
        let lifted = lifter
            .lift_raw_pcode_function_with_contract(&bytes, 0x1000, 1)
            .expect("lift function with limit");

        assert_eq!(lifted.decoded_instructions, 1);
        assert_eq!(lifted.stop_reason, LiftStopReason::InstructionLimit);
        assert_eq!(lifted.function.blocks.len(), 1);
    }

    #[test]
    fn lift_contract_reports_terminal_control_flow_stop() {
        let lifter = SleighLifter::new_for_language("x86-64").expect("x86-64 sleigh lifter");
        let bytes = [0x90, 0xC3];
        let lifted = lifter
            .lift_raw_pcode_function_with_contract(&bytes, 0x2000, 16)
            .expect("lift function with return terminator");

        assert_eq!(lifted.decoded_instructions, 2);
        assert_eq!(lifted.stop_reason, LiftStopReason::TerminalControlFlow);
        assert_eq!(lifted.function.blocks.len(), 1);
        assert!(lifted.function.blocks[0]
            .ops
            .iter()
            .any(|op| op.opcode == PcodeOpcode::Return));
    }

    #[test]
    fn x86_cmp_flags_feed_jcc_predicate_path() {
        let lifter = SleighLifter::new_for_language("x86-64").expect("x86-64 sleigh lifter");
        let bytes = [0x39, 0xD8, 0x75, 0x01, 0x90, 0xC3];
        let lifted = lifter
            .lift_raw_pcode_function_with_contract(&bytes, 0x3000, 16)
            .expect("lift function with cmp+jne flow");

        let mut all_ops = Vec::new();
        for block in &lifted.function.blocks {
            all_ops.extend(block.ops.iter());
        }

        let zf_offset = super::common::X86_EFLAGS_BASE + 6;
        assert!(all_ops.iter().any(|op| {
            op.address == 0x3000
                && op
                    .output
                    .as_ref()
                    .map(|out| out.space_id == UNIQUE_SPACE_ID && out.offset == zf_offset)
                    .unwrap_or(false)
        }));

        let jcc_pred = all_ops
            .iter()
            .find(|op| op.address == 0x3002 && op.opcode == PcodeOpcode::BoolNegate)
            .expect("jne predicate build op");
        assert_eq!(jcc_pred.inputs.len(), 1);
        assert_eq!(jcc_pred.inputs[0].space_id, UNIQUE_SPACE_ID);
        assert_eq!(jcc_pred.inputs[0].offset, zf_offset);

        let cbranch = all_ops
            .iter()
            .find(|op| op.address == 0x3002 && op.opcode == PcodeOpcode::CBranch)
            .expect("jne cbranch op");
        assert_eq!(cbranch.inputs.len(), 2);
        assert_eq!(
            cbranch.inputs[1].offset,
            jcc_pred.output.as_ref().expect("predicate output").offset
        );
    }
}
