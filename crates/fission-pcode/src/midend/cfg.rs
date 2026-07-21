use super::*;
use fission_loader::loader::LoadedBinary;

pub(super) fn build_address_to_index_map(pcode: &PcodeFunction) -> HashMap<u64, usize> {
    let mut address_to_index = HashMap::default();
    for (idx, block) in pcode.blocks.iter().enumerate() {
        address_to_index.entry(block.start_address).or_insert(idx);
    }
    address_to_index
}

const DUPLICATE_BLOCK_KEY_TAG: u64 = 0x8000_0000_0000_0000;

pub(super) fn build_block_target_keys(pcode: &PcodeFunction) -> Vec<u64> {
    let mut seen: HashMap<u64, u32> = HashMap::default();
    pcode
        .blocks
        .iter()
        .map(|block| {
            let ordinal = seen.entry(block.start_address).or_insert(0);
            let key = if *ordinal == 0 {
                block.start_address
            } else {
                encode_duplicate_block_key(block.start_address, *ordinal)
            };
            *ordinal += 1;
            key
        })
        .collect()
}

fn encode_duplicate_block_key(start_address: u64, ordinal: u32) -> u64 {
    debug_assert!(ordinal > 0);
    DUPLICATE_BLOCK_KEY_TAG
        | ((u64::from(ordinal) & 0x7fff) << 48)
        | (start_address & 0x0000_ffff_ffff_ffff)
}

pub(super) fn canonical_block_start_for_address(
    pcode: &PcodeFunction,
    address: u64,
) -> Option<u64> {
    let mut starts = pcode
        .blocks
        .iter()
        .map(|block| block.start_address)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let idx = starts.partition_point(|start| *start <= address);
    idx.checked_sub(1).map(|idx| starts[idx])
}

pub(super) fn canonical_block_index_for_address(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    address: u64,
) -> Option<usize> {
    let canonical = canonical_block_start_for_address(pcode, address)?;
    address_to_index.get(&canonical).copied()
}

pub(super) fn duplicate_block_start_count(pcode: &PcodeFunction) -> usize {
    pcode
        .blocks
        .len()
        .saturating_sub(build_address_to_index_map(pcode).len())
}

pub(super) fn build_successor_index_map(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    layout_fallthrough: &[Option<usize>],
) -> Vec<Vec<usize>> {
    pcode
        .blocks
        .iter()
        .enumerate()
        .map(|(idx, block)| {
            let mut succs = Vec::new();
            match block_terminator_op(block) {
                Some(op) if op.opcode == PcodeOpcode::Return => {}
                Some(op) if op.opcode == PcodeOpcode::Branch && op.inputs.len() == 1 => {
                    if let Some(target_idx) = op.inputs.first().and_then(|input| {
                        resolve_branch_target_index(pcode, address_to_index, idx, op, input)
                    }) {
                        succs.push(target_idx);
                    }
                }
                Some(op)
                    if op.opcode == PcodeOpcode::CBranch
                        || (op.opcode == PcodeOpcode::Branch && op.inputs.len() >= 2) =>
                {
                    if let Some(target_idx) = op.inputs.first().and_then(|input| {
                        resolve_branch_target_index(pcode, address_to_index, idx, op, input)
                    }) {
                        succs.push(target_idx);
                    }
                    if let Some(next_idx) = layout_fallthrough[idx] {
                        succs.push(next_idx);
                    }
                }
                Some(op) if op.opcode == PcodeOpcode::BranchInd => {}
                _ => {
                    if let Some(next_idx) = layout_fallthrough[idx] {
                        succs.push(next_idx);
                    }
                }
            }
            succs.sort_unstable();
            succs.dedup();
            succs
        })
        .collect()
}

/// Extra `(from_block_idx, to_block_idx)` edges for C++ exception-handling
/// landing pads (see `fission_loader::loader::elf::lsda`) that
/// `build_successor_index_map`'s terminator-op-based derivation can never
/// discover on its own: a landing pad is reachable only via the personality
/// routine unwinding into it at runtime, not any `Branch`/`CBranch`/
/// `BranchInd`/`Return`/fallthrough in the function's own p-code -- so
/// without this, it ends up with an empty predecessor list and gets treated
/// as dead code by structuring's orphan-block elimination.
///
/// Scoped strictly to `binary.eh_lsda`, keyed by this p-code's own function
/// entry address (the lowest-addressed block, which is always the decode
/// entry point -- see `build_cfg_blocks_with_hints`'s address-sorted block
/// ordering in `fission-sleigh`). A function with no LSDA entry -- which is
/// every function in every binary without C++ exception handling -- gets
/// zero edges here, so this can't change behavior for anything else.
pub(super) fn lsda_extra_edges(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    binary: Option<&LoadedBinary>,
) -> Vec<(usize, usize)> {
    let Some(binary) = binary else {
        return Vec::new();
    };
    let Some(entry_address) = pcode.blocks.first().map(|block| block.start_address) else {
        return Vec::new();
    };
    let Some(info) = binary.eh_lsda.get(&entry_address) else {
        return Vec::new();
    };
    info.call_sites
        .iter()
        .filter_map(|call_site| {
            let landing_pad = call_site.landing_pad?;
            let from = canonical_block_index_for_address(pcode, address_to_index, call_site.start)?;
            let to = canonical_block_index_for_address(pcode, address_to_index, landing_pad)?;
            Some((from, to))
        })
        .collect()
}

pub(super) fn build_predecessor_index_map(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::new(); successors.len()];
    for (idx, succs) in successors.iter().enumerate() {
        for succ in succs {
            predecessors[*succ].push(idx);
        }
    }
    predecessors
}

pub(super) fn build_layout_fallthrough_map(pcode: &PcodeFunction) -> Vec<Option<usize>> {
    (0..pcode.blocks.len())
        .map(|idx| (idx + 1 < pcode.blocks.len()).then_some(idx + 1))
        .collect()
}

pub(super) fn block_terminator_op(block: &crate::pcode::PcodeBasicBlock) -> Option<&PcodeOp> {
    let idx = block.ops.iter().rposition(|op| {
        matches!(
            op.opcode,
            PcodeOpcode::Branch
                | PcodeOpcode::CBranch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Return
        )
    })?;
    block.ops.get(idx)
}

pub(super) fn const_offset(vn: &Varnode) -> Option<i64> {
    if vn.is_constant {
        Some(vn.constant_val)
    } else {
        None
    }
}

pub(super) fn branch_target_address(vn: &Varnode) -> Option<u64> {
    if vn.is_constant {
        if vn.offset != 0 {
            Some(vn.offset)
        } else if vn.constant_val >= 0 {
            Some(vn.constant_val as u64)
        } else {
            None
        }
    } else if vn.offset != 0 {
        Some(vn.offset)
    } else {
        None
    }
}

pub(super) fn resolve_branch_target_index(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    block_idx: usize,
    op: &PcodeOp,
    vn: &Varnode,
) -> Option<usize> {
    resolve_instruction_local_branch_target_index(pcode, block_idx, op, vn).or_else(|| {
        let target = branch_target_address(vn)?;
        canonical_block_index_for_address(pcode, address_to_index, target)
    })
}

pub(super) fn instruction_local_branch_target_seq(op: &PcodeOp, vn: &Varnode) -> Option<u32> {
    if vn.space_id != 0 || !vn.is_constant {
        return None;
    }
    // Ghidra snippet emulation interprets relative branch offsets as signed int32.
    let raw = if vn.offset != 0 {
        vn.offset as u32
    } else {
        vn.constant_val as u32
    };
    let delta = i32::from_le_bytes(raw.to_le_bytes());
    if delta == 0 {
        return None;
    }
    if delta > 0 {
        op.seq_num.checked_add(delta as u32)
    } else {
        op.seq_num.checked_sub(delta.unsigned_abs())
    }
}

/// Resolve a forward branch that stays inside `block` to a later op index.
///
/// Two SLEIGH encodings appear for cmov / instruction-local control flow:
/// 1. Relative p-code deltas in the constant space (`space_id == 0`, `is_constant`)
/// 2. Absolute instruction addresses in the code space (e.g. x86 cmov: CBranch
///    to the next instruction address, `is_constant == false`, non-zero offset)
///
/// Case (2) is common on x86 after epilogue-reordered cmov sequences and must
/// not be dropped: otherwise the guarded Copy always executes and collapses
/// clamps / min/max to the last unconditional assignment.
pub(super) fn same_block_forward_branch_target_op_idx(
    block: &crate::pcode::PcodeBasicBlock,
    from_op_idx: usize,
    end_idx: usize,
    op: &PcodeOp,
    target_vn: &Varnode,
) -> Option<usize> {
    if from_op_idx >= end_idx || from_op_idx >= block.ops.len() {
        return None;
    }
    let search = &block.ops[from_op_idx + 1..end_idx.min(block.ops.len())];
    if search.is_empty() {
        return None;
    }

    if let Some(target_seq) = instruction_local_branch_target_seq(op, target_vn) {
        if let Some(pos) = search
            .iter()
            .position(|candidate| candidate.seq_num == target_seq)
        {
            return Some(from_op_idx + 1 + pos);
        }
    }

    let target_addr = branch_target_address(target_vn)?;
    let from_addr = op.address;
    // Only treat absolute targets as same-block skips when they land strictly
    // after the current instruction address (cmov fall-through to the next
    // machine instruction). Same-address relative micro-op skips stay on the
    // seq-delta path above.
    if target_addr <= from_addr {
        return None;
    }
    if let Some(pos) = search
        .iter()
        .position(|candidate| candidate.address == target_addr)
    {
        return Some(from_op_idx + 1 + pos);
    }
    // Tail-of-block cmov: SLEIGH often targets the *next basic block's* start
    // address (epilogue / ret) while the guarded Copy still sits at the end of
    // *this* block at the *same machine address* as the CBranch. Require:
    // - remaining ops share `from_addr` (cmov micro-ops of one insn)
    // - target is not an op address in this block
    // - target is strictly after from_addr
    //
    // Example (saturating_add cmovl):
    //   block_k @ 0x40169f: CBranch -> 0x4016a2; Copy eax <- INT_MIN
    //   block_k+1 @ 0x4016a2: pop/ret
    let limit = end_idx.min(block.ops.len());
    if from_op_idx + 1 < limit
        && search.iter().all(|candidate| candidate.address == from_addr)
        && !block.ops.iter().any(|candidate| candidate.address == target_addr)
    {
        return Some(limit);
    }
    None
}

fn resolve_instruction_local_branch_target_index(
    pcode: &PcodeFunction,
    _block_idx: usize,
    op: &PcodeOp,
    vn: &Varnode,
) -> Option<usize> {
    let target_seq = instruction_local_branch_target_seq(op, vn)?;

    pcode
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| {
            block
                .ops
                .first()
                .is_some_and(|first| first.address == op.address && first.seq_num == target_seq)
        })
        .map(|(idx, _)| idx)
        .or_else(|| {
            pcode
                .blocks
                .iter()
                .enumerate()
                .find(|(_, block)| {
                    block.start_address == op.address
                        && block
                            .ops
                            .iter()
                            .any(|candidate| candidate.seq_num == target_seq)
                })
                .map(|(idx, _)| idx)
        })
}

pub(super) fn block_label(address: u64) -> String {
    if address & DUPLICATE_BLOCK_KEY_TAG != 0 {
        let ordinal = (address >> 48) & 0x7fff;
        let raw = address & 0x0000_ffff_ffff_ffff;
        format!("block_{raw:x}_dup{ordinal}")
    } else {
        format!("block_{:x}", address)
    }
}

pub(super) fn fold_logical_chain(mut exprs: Vec<HirExpr>, op: HirBinaryOp) -> HirExpr {
    debug_assert!(matches!(
        op,
        HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr
    ));
    if exprs.is_empty() {
        return HirExpr::Const(
            if op == HirBinaryOp::LogicalAnd { 1 } else { 0 },
            NirType::Bool,
        );
    }
    let first = exprs.remove(0);
    exprs.into_iter().fold(first, |lhs, rhs| HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    })
}

pub(super) fn negate_expr(expr: HirExpr) -> HirExpr {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

pub(super) fn strip_casts(expr: &HirExpr) -> HirExpr {
    match expr {
        HirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other.clone(),
    }
}

pub(super) fn simplify_logical_expr(expr: HirExpr) -> HirExpr {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::LogicalOr,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ty,
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::LogicalAnd,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ty,
            }
        }
        HirExpr::Unary { op, expr, ty } => HirExpr::Unary {
            op,
            expr: Box::new(simplify_logical_expr(*expr)),
            ty,
        },
        other => other,
    }
}

/// Address-keyed CFG edges using the same successor map as NIR structuring.
pub fn structuring_cfg_edges(pcode: &PcodeFunction) -> Vec<crate::cfg::AddressEdge> {
    let address_to_index = build_address_to_index_map(pcode);
    let layout_fallthrough = build_layout_fallthrough_map(pcode);
    let successors = build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
    let mut edges = Vec::new();
    for (idx, succs) in successors.iter().enumerate() {
        let Some(from) = pcode.blocks.get(idx) else {
            continue;
        };
        for succ in succs {
            let Some(to) = pcode.blocks.get(*succ) else {
                continue;
            };
            edges.push(crate::cfg::AddressEdge {
                from: from.start_address,
                to: to.start_address,
            });
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}

#[cfg(test)]
mod same_block_forward_tests {
    use super::*;
    use crate::pcode::{PcodeBasicBlock, PcodeOp, PcodeOpcode, Varnode};

    fn op(seq: u32, addr: u64, opcode: PcodeOpcode, inputs: Vec<Varnode>) -> PcodeOp {
        PcodeOp {
            seq_num: seq,
            opcode,
            address: addr,
            output: None,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn absolute_code_space_forward_target_resolves() {
        let target = Varnode {
            space_id: 3,
            offset: 0x4010,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let cond = Varnode {
            space_id: 2,
            offset: 1,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let block = PcodeBasicBlock {
            index: 0,
            start_address: 0x4000,
            successors: vec![],
            ops: vec![
                op(0, 0x4000, PcodeOpcode::Copy, vec![]),
                op(1, 0x400c, PcodeOpcode::CBranch, vec![target.clone(), cond]),
                op(2, 0x400c, PcodeOpcode::Copy, vec![]),
                op(3, 0x4010, PcodeOpcode::Return, vec![]),
            ],
        };
        let idx = same_block_forward_branch_target_op_idx(
            &block,
            1,
            block.ops.len(),
            &block.ops[1],
            &target,
        );
        assert_eq!(idx, Some(3), "expected absolute next-insn target");
    }

    #[test]
    fn absolute_tail_of_block_cmov_skip_resolves_to_block_end() {
        let target = Varnode {
            space_id: 3,
            offset: 0x4016a2,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let cond = Varnode {
            space_id: 2,
            offset: 1,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let eax = Varnode {
            space_id: 4,
            offset: 0,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let imm = Varnode::constant(i64::from(i32::MIN), 4);
        let block = PcodeBasicBlock {
            index: 3,
            start_address: 0x401698,
            successors: vec![4],
            ops: vec![
                op(0, 0x401698, PcodeOpcode::Copy, vec![]),
                op(1, 0x40169f, PcodeOpcode::CBranch, vec![target.clone(), cond]),
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x40169f,
                    output: Some(eax),
                    inputs: vec![imm],
                    asm_mnemonic: None,
                },
            ],
        };
        let idx = same_block_forward_branch_target_op_idx(
            &block,
            1,
            block.ops.len(),
            &block.ops[1],
            &target,
        );
        assert_eq!(
            idx,
            Some(block.ops.len()),
            "tail cmov should skip remaining ops through block end"
        );
    }
}

#[cfg(test)]
mod lsda_extra_edges_tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;
    use fission_loader::loader::elf::lsda::{LsdaCallSite, LsdaInfo};
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    fn empty_block(start_address: u64) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index: 0,
            start_address,
            successors: vec![],
            ops: vec![],
        }
    }

    fn test_binary() -> fission_loader::loader::LoadedBinary {
        LoadedBinaryBuilder::new("lsda_test".to_string(), DataBuffer::Heap(vec![]))
            .format("ELF64")
            .arch_spec("x86:LE:64:default")
            .is_64bit(true)
            .build()
            .expect("build test LoadedBinary")
    }

    /// The scenario that motivated this function: a call-site block (index
    /// 0) whose only edge to the landing-pad block (index 2) comes from
    /// `binary.eh_lsda`, not from any p-code branch/call/fallthrough
    /// `build_successor_index_map` could derive on its own -- see
    /// `crates/fission-loader/src/loader/elf/lsda.rs` and the real
    /// `guarded()`/`x64_dyn_lsda_test.elf` case this mirrors.
    #[test]
    fn resolves_landing_pad_edge_from_binary_eh_lsda() {
        let pcode = PcodeFunction {
            blocks: vec![
                empty_block(0x1000),
                empty_block(0x1010),
                empty_block(0x1020),
            ],
        };
        let address_to_index = build_address_to_index_map(&pcode);

        let mut binary = test_binary();
        binary.eh_lsda.insert(
            0x1000,
            LsdaInfo {
                lp_start: 0x1000,
                call_sites: vec![
                    LsdaCallSite {
                        start: 0x1000,
                        end: 0x1010,
                        landing_pad: Some(0x1020),
                        action_chain: vec![1],
                    },
                    LsdaCallSite {
                        start: 0x1010,
                        end: 0x1020,
                        landing_pad: None, // no edge: must be skipped, not (idx, None)
                        action_chain: vec![],
                    },
                ],
                type_table: vec![],
            },
        );

        let edges = lsda_extra_edges(&pcode, &address_to_index, Some(&binary));
        assert_eq!(edges, vec![(0, 2)]);
    }

    #[test]
    fn returns_empty_without_a_binary() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        assert!(lsda_extra_edges(&pcode, &address_to_index, None).is_empty());
    }

    #[test]
    fn returns_empty_when_function_has_no_lsda_entry() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let binary = test_binary(); // eh_lsda left empty
        assert!(lsda_extra_edges(&pcode, &address_to_index, Some(&binary)).is_empty());
    }

    /// `build_successor_index_map`'s own instruction-derived edges must
    /// stay untouched when there's no LSDA data for the function -- the
    /// merge in `builder/init.rs` only ever *adds* edges, on top of
    /// whatever this already computes.
    #[test]
    fn build_successor_index_map_unaffected_by_absent_lsda_data() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000), empty_block(0x1010)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let layout_fallthrough = build_layout_fallthrough_map(&pcode);
        let successors = build_successor_index_map(&pcode, &address_to_index, &layout_fallthrough);
        assert_eq!(successors, vec![vec![1], vec![]]);
    }
}
