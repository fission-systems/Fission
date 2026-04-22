use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

mod aarch64;
mod backend;
mod x86;

use backend::common::UNIQUE_SPACE_ID;

#[derive(Debug, Clone)]
pub struct SleighLifter {
    backend: backend::BackendKind,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiftDecodeContract {
    pub instruction_limit: usize,
    pub stop_at_indirect_branch: bool,
}

impl LiftDecodeContract {
    pub const fn strict_function(instruction_limit: usize) -> Self {
        Self {
            instruction_limit,
            stop_at_indirect_branch: true,
        }
    }

    pub const fn decomp_function(instruction_limit: usize) -> Self {
        Self {
            instruction_limit,
            stop_at_indirect_branch: false,
        }
    }

    pub const fn is_terminal_control_flow(self, opcode: PcodeOpcode) -> bool {
        matches!(opcode, PcodeOpcode::Return)
            || (self.stop_at_indirect_branch && matches!(opcode, PcodeOpcode::BranchInd))
    }
}

pub fn is_terminal_control_flow(opcode: PcodeOpcode) -> bool {
    LiftDecodeContract::strict_function(DEFAULT_FUNCTION_INSTRUCTION_LIMIT)
        .is_terminal_control_flow(opcode)
}

fn cfg_build_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
        || std::env::var_os("FISSION_PREVIEW_DEBUG").is_some()
        || std::env::var_os("FISSION_SLEIGH_CFG_DIAG").is_some()
}

fn cfg_build_diag_log(entry_address: u64, message: &str) {
    if !cfg_build_diag_enabled() {
        return;
    }

    eprintln!("[CFG-DIAG] {message}");

    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some()
        || std::env::var_os("FISSION_SLEIGH_CFG_DIAG").is_some()
    {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{entry_address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, format!("[cfg-build] {message}\n").as_bytes())
            });
    }
}

fn format_varnode_diag(vn: &Varnode) -> String {
    format!(
        "space={} off=0x{:x} size={} const={} val={}",
        vn.space_id, vn.offset, vn.size, vn.is_constant, vn.constant_val
    )
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

    cfg_build_diag_log(
        entry_address,
        &format!("start entry=0x{:x} op_count={}", entry_address, ops.len()),
    );

    let mut addr_to_op_idx: HashMap<u64, usize> = HashMap::new();
    for (idx, op) in ops.iter().enumerate() {
        addr_to_op_idx.entry(op.address).or_insert(idx);
    }

    let mut block_starts: BTreeSet<usize> = BTreeSet::new();
    block_starts.insert(0);

    for (idx, op) in ops.iter().enumerate() {
        if backend::is_cfg_split_opcode(op.opcode) {
            if idx + 1 < ops.len() {
                block_starts.insert(idx + 1);
            }
            if let Some(target) = backend::direct_control_target(op) {
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
        let mut branch_target = None;
        let mut branch_input = None;
        if let Some(last) = block_ops.last() {
            match last.opcode {
                PcodeOpcode::Branch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) = backend::direct_control_target(last) {
                        branch_target = Some(target);
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        } else {
                            cfg_build_diag_log(
                                entry_address,
                                &format!(
                                    "branch_target_unmapped block_idx={} block_start=0x{:x} seq=0x{:x} target=0x{:x} input={}",
                                    block_idx,
                                    last.address,
                                    last.seq_num,
                                    target,
                                    branch_input.as_deref().unwrap_or("<none>")
                                ),
                            );
                        }
                    } else {
                        cfg_build_diag_log(
                            entry_address,
                            &format!(
                                "branch_target_missing block_idx={} block_start=0x{:x} seq=0x{:x} input={}",
                                block_idx,
                                last.address,
                                last.seq_num,
                                branch_input.as_deref().unwrap_or("<none>")
                            ),
                        );
                    }
                }
                PcodeOpcode::CBranch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) = backend::direct_control_target(last) {
                        branch_target = Some(target);
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        } else {
                            cfg_build_diag_log(
                                entry_address,
                                &format!(
                                    "cbranch_true_target_unmapped block_idx={} block_start=0x{:x} seq=0x{:x} target=0x{:x} input={}",
                                    block_idx,
                                    last.address,
                                    last.seq_num,
                                    target,
                                    branch_input.as_deref().unwrap_or("<none>")
                                ),
                            );
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

            if matches!(last.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
                && successors.is_empty()
            {
                cfg_build_diag_log(
                    entry_address,
                    &format!(
                        "control_block_no_successors block_idx={} block_start=0x{:x} seq=0x{:x} opcode={:?} target={} input={}",
                        block_idx,
                        last.address,
                        last.seq_num,
                        last.opcode,
                        branch_target
                            .map(|v| format!("0x{v:x}"))
                            .unwrap_or_else(|| "<none>".to_string()),
                        branch_input.as_deref().unwrap_or("<none>")
                    ),
                );
            }
        }

        let start_address = block_ops
            .first()
            .map(|op| op.address)
            .unwrap_or(entry_address);
        let succ_starts = successors
            .iter()
            .filter_map(|succ| {
                starts
                    .get(*succ as usize)
                    .and_then(|start_idx| ops.get(*start_idx))
            })
            .map(|op| format!("0x{:x}", op.address))
            .collect::<Vec<_>>();
        cfg_build_diag_log(
            entry_address,
            &format!(
                "block_finalize block_idx={} start=0x{:x} succ_block_idxs={:?} succ_starts={:?} op_count={}",
                block_idx,
                start_address,
                successors,
                succ_starts,
                block_ops.len()
            ),
        );

        blocks.push(PcodeBasicBlock {
            index: block_idx as u32,
            start_address,
            successors,
            ops: block_ops,
        });
    }

    cfg_build_diag_log(
        entry_address,
        &format!("done entry=0x{:x} blocks={}", entry_address, blocks.len()),
    );

    blocks
}

impl SleighLifter {
    pub fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs/languages")
    }

    fn find_spec_path_recursive(dir: &Path, language_name: &str) -> Option<PathBuf> {
        let mut entries = fs::read_dir(dir).ok()?.collect::<Result<Vec<_>, _>>().ok()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = Self::find_spec_path_recursive(&path, language_name) {
                    return Some(found);
                }
                continue;
            }

            let is_target = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == format!("{language_name}.slaspec"))
                .unwrap_or(false);
            if is_target {
                return Some(path);
            }
        }

        None
    }

    pub fn find_spec_path_for(language_name: &str) -> Option<PathBuf> {
        Self::find_spec_path_recursive(&Self::spec_dir(), language_name)
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::find_spec_path_for(language_name)
            .unwrap_or_else(|| Self::spec_dir().join(format!("{}.slaspec", language_name)))
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

        let backend = backend::BackendKind::for_language(language_name);

        Ok(Self { backend })
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
    ) -> Result<LiftedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract(
            bytes,
            entry_address,
            LiftDecodeContract::strict_function(instruction_limit),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: LiftDecodeContract,
    ) -> Result<LiftedPcodeFunction> {
        let _lift = tracing::trace_span!(
            "sleigh_lift_raw",
            entry_address = entry_address,
            instruction_limit = contract.instruction_limit,
            stop_at_indirect_branch = contract.stop_at_indirect_branch
        )
        .entered();
        let lifted = self.backend.lift_ops_with_contract(
            bytes,
            entry_address,
            contract,
            Self::emit_trace_copy,
        )?;
        debug_assert!(lifted.consumed_bytes <= bytes.len());

        Ok(LiftedPcodeFunction {
            function: PcodeFunction {
                blocks: build_cfg_blocks(entry_address, lifted.ops),
            },
            decoded_instructions: lifted.decoded_instructions,
            stop_reason: lifted.stop_reason,
        })
    }

    pub fn decode_and_lift_with_len(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)> {
        self.backend
            .decode_and_lift_with_len(bytes, address, Self::emit_trace_copy)
    }

    fn emit_trace_copy(insn: &[u8], address: u64) -> PcodeOp {
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
    fn decomp_lift_contract_continues_past_branchind() {
        let lifter = SleighLifter::new_for_language("x86-64").expect("x86-64 sleigh lifter");
        let bytes = [0xFF, 0xE0, 0x90];
        let lifted = lifter
            .lift_raw_pcode_function_with_decode_contract(
                &bytes,
                0x2100,
                LiftDecodeContract::decomp_function(16),
            )
            .expect("lift function across branchind");

        assert!(lifted.decoded_instructions >= 2);
        assert_eq!(lifted.stop_reason, LiftStopReason::InputExhausted);
        assert!(lifted.function.blocks.len() >= 2);
        assert!(lifted.function.blocks[0]
            .ops
            .iter()
            .any(|op| op.opcode == PcodeOpcode::BranchInd));
    }

    #[test]
    fn backend_lift_contract_keeps_trace_order_and_consumed_bytes() {
        let backend = super::backend::BackendKind::X86;
        let bytes = [0x90, 0x90];
        let lifted = backend
            .lift_ops_with_contract(
                &bytes,
                0x4100,
                LiftDecodeContract::strict_function(16),
                super::SleighLifter::emit_trace_copy,
            )
            .expect("lift ops through backend contract");

        assert_eq!(lifted.decoded_instructions, 2);
        assert_eq!(lifted.stop_reason, LiftStopReason::InputExhausted);
        assert_eq!(lifted.consumed_bytes, 2);

        let trace_ops = lifted
            .ops
            .iter()
            .filter(|op| op.asm_mnemonic.as_deref() == Some("INSN_RAW"))
            .collect::<Vec<_>>();
        assert_eq!(trace_ops.len(), 2);
        assert_eq!(trace_ops[0].address, 0x4100);
        assert_eq!(trace_ops[1].address, 0x4101);

        assert!(lifted.ops.windows(2).all(|w| w[0].seq_num < w[1].seq_num));
    }

    #[test]
    fn backend_lift_contract_reports_decode_failure_address() {
        let backend = super::backend::BackendKind::X86;
        let err = backend
            .lift_ops_with_contract(
                &[0x90, 0x0F],
                0x4200,
                LiftDecodeContract::strict_function(16),
                super::SleighLifter::emit_trace_copy,
            )
            .expect_err("expected decode failure on truncated 0x0F escape");

        let msg = format!("{err:#}");
        assert!(msg.contains("decode failed at 0x4201"));
        assert!(msg.contains("truncated 0x0F escape opcode"));
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

        let zf_offset = x86::common::X86_EFLAGS_BASE + 6;
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

    #[test]
    fn spec_lookup_finds_x86_in_arch_subdirectory() {
        let path = SleighLifter::find_spec_path_for("x86-64").expect("x86-64 spec path");
        assert!(path.ends_with("specs/languages/x86/x86-64.slaspec"));
    }

    #[test]
    fn spec_lookup_finds_aarch64_in_arch_subdirectory() {
        let path = SleighLifter::find_spec_path_for("AARCH64").expect("AARCH64 spec path");
        assert!(path.ends_with("specs/languages/aarch64/AARCH64.slaspec"));
    }
}
