//! Block-level control-flow facts derived from lifted p-code.
//!
//! The successor map built here is the one NIR structuring uses, and
//! [`structuring_cfg_edges`] exports it keyed by address for the parity probes
//! in [`crate::cfg`]. Nothing in this module reads the midend IR, call
//! summaries, or a loaded binary; those callers live in `fission-pcode`.

use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

type HashMap<K, V> = std::collections::HashMap<K, V, rustc_hash::FxBuildHasher>;

const DUPLICATE_BLOCK_KEY_TAG: u64 = 0x8000_0000_0000_0000;

pub fn build_address_to_index_map(pcode: &PcodeFunction) -> HashMap<u64, usize> {
    let mut address_to_index = HashMap::default();
    for (idx, block) in pcode.blocks.iter().enumerate() {
        address_to_index.entry(block.start_address).or_insert(idx);
    }
    address_to_index
}

pub fn build_block_target_keys(pcode: &PcodeFunction) -> Vec<u64> {
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

pub fn encode_duplicate_block_key(start_address: u64, ordinal: u32) -> u64 {
    debug_assert!(ordinal > 0);
    DUPLICATE_BLOCK_KEY_TAG
        | ((u64::from(ordinal) & 0x7fff) << 48)
        | (start_address & 0x0000_ffff_ffff_ffff)
}

/// Allocate a deterministic target identity for a node-split clone.
///
/// A clone has the same p-code start address as its source, but it is a
/// distinct CFG node and therefore needs a distinct emitted label. Start with
/// the first duplicate ordinal after the real blocks at that address and
/// continue until the key is absent from the identities already allocated.
/// This keeps clone labels in the same namespace as duplicate-address block
/// labels without relying on a synthetic address or a function-specific rule.
pub fn allocate_virtual_block_target_key(
    pcode: &PcodeFunction,
    existing_keys: &[u64],
    original_idx: usize,
) -> u64 {
    let start_address = pcode
        .blocks
        .get(original_idx)
        .map(|block| block.start_address)
        .unwrap_or(0);
    let mut ordinal = pcode
        .blocks
        .iter()
        .filter(|block| block.start_address == start_address)
        .count()
        .max(1) as u32;
    loop {
        let key = encode_duplicate_block_key(start_address, ordinal);
        if !existing_keys.contains(&key) {
            return key;
        }
        ordinal = ordinal.saturating_add(1);
    }
}

/// Map a code address onto the block that actually contains it.
///
/// A branch may target the middle of a block, so the candidate is the greatest
/// block start at or below `address`. That candidate is only correct if the
/// address is genuinely covered by the block: a tail call (`jmp other_func`)
/// leaves the function entirely, and without the coverage check its target
/// silently collapses onto the last block that happens to start below it --
/// for a single-block leaf function, onto the function's own entry, turning
/// `return other(x)` into `goto entry`.
///
/// Coverage is exact rather than a range test: every instruction in a block
/// stamps its address onto the ops it lowers to, and a branch can only target
/// an instruction boundary.
pub fn canonical_block_start_for_address(pcode: &PcodeFunction, address: u64) -> Option<u64> {
    let mut starts = pcode
        .blocks
        .iter()
        .map(|block| block.start_address)
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let idx = starts.partition_point(|start| *start <= address);
    let start = idx.checked_sub(1).map(|idx| starts[idx])?;
    if start == address {
        return Some(start);
    }
    pcode
        .blocks
        .iter()
        .filter(|block| block.start_address == start)
        .any(|block| block.ops.iter().any(|op| op.address == address))
        .then_some(start)
}

pub fn canonical_block_index_for_address(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    address: u64,
) -> Option<usize> {
    let canonical = canonical_block_start_for_address(pcode, address)?;
    address_to_index.get(&canonical).copied()
}

pub fn duplicate_block_start_count(pcode: &PcodeFunction) -> usize {
    pcode
        .blocks
        .len()
        .saturating_sub(build_address_to_index_map(pcode).len())
}

pub fn build_successor_index_map(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    layout_fallthrough: &[Option<usize>],
) -> Vec<Vec<usize>> {
    let block_id_to_position: HashMap<u32, usize> = pcode
        .blocks
        .iter()
        .enumerate()
        .map(|(position, block)| (block.index, position))
        .collect();
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
                Some(op) if op.opcode == PcodeOpcode::BranchInd => {
                    // Indirect-branch discovery runs after lifting and records
                    // proven targets on the block as logical block IDs. The
                    // opcode itself carries only the computed destination, so
                    // reconstructing this edge map from the terminator alone
                    // silently disconnects every jump-table case.
                    succs.extend(
                        block
                            .successors
                            .iter()
                            .filter_map(|successor| block_id_to_position.get(successor).copied()),
                    );
                }
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

pub fn build_predecessor_index_map(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::new(); successors.len()];
    for (idx, succs) in successors.iter().enumerate() {
        for succ in succs {
            predecessors[*succ].push(idx);
        }
    }
    predecessors
}

pub fn build_layout_fallthrough_map(pcode: &PcodeFunction) -> Vec<Option<usize>> {
    (0..pcode.blocks.len())
        .map(|idx| (idx + 1 < pcode.blocks.len()).then_some(idx + 1))
        .collect()
}

pub fn block_terminator_op(block: &crate::pcode::PcodeBasicBlock) -> Option<&PcodeOp> {
    let idx = block.ops.iter().enumerate().rposition(|(idx, op)| {
        let is_control = matches!(
            op.opcode,
            PcodeOpcode::Branch
                | PcodeOpcode::CBranch
                | PcodeOpcode::BranchInd
                | PcodeOpcode::Return
        );
        is_control && !is_instruction_local_forward_branch(block, idx, op)
    })?;
    block.ops.get(idx)
}

/// Whether a branch is an instruction-local skip rather than a basic-block
/// terminator. SLEIGH represents conditional moves and similar instruction
/// internals as a forward `Branch`/`CBranch` followed by more p-code in the
/// same block. Treating that branch as an inter-block terminator resolves its
/// target back onto the current block and fabricates a CFG self-edge.
pub fn is_instruction_local_forward_branch(
    block: &crate::pcode::PcodeBasicBlock,
    op_idx: usize,
    op: &PcodeOp,
) -> bool {
    matches!(op.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch)
        && op.inputs.first().is_some_and(|target| {
            same_block_forward_branch_target_op_idx(block, op_idx, block.ops.len(), op, target)
                .is_some()
        })
}

pub fn const_offset(vn: &Varnode) -> Option<i64> {
    if vn.is_constant {
        Some(vn.constant_val)
    } else {
        None
    }
}

pub fn branch_target_address(vn: &Varnode) -> Option<u64> {
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

pub fn resolve_branch_target_index(
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

pub fn instruction_local_branch_target_seq(op: &PcodeOp, vn: &Varnode) -> Option<u32> {
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
pub fn same_block_forward_branch_target_op_idx(
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
        && search
            .iter()
            .all(|candidate| candidate.address == from_addr)
        && !block
            .ops
            .iter()
            .any(|candidate| candidate.address == target_addr)
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

pub fn block_label(address: u64) -> String {
    if address & DUPLICATE_BLOCK_KEY_TAG != 0 {
        let ordinal = (address >> 48) & 0x7fff;
        let raw = address & 0x0000_ffff_ffff_ffff;
        format!("block_{raw:x}_dup{ordinal}")
    } else {
        format!("block_{:x}", address)
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
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

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
    fn tail_call_target_outside_the_function_does_not_resolve() {
        // `jmp other_func` leaves the function: with only one block, the
        // greatest-start-below rule would hand back the entry, turning the
        // tail call into `goto entry` -- a fabricated self-loop.
        let pcode = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0xf110,
                successors: vec![],
                ops: vec![
                    op(0, 0xf114, PcodeOpcode::Copy, vec![]),
                    op(1, 0xf124, PcodeOpcode::Branch, vec![]),
                ],
            }],
        };
        assert_eq!(canonical_block_start_for_address(&pcode, 0x15380), None);
        assert_eq!(
            canonical_block_start_for_address(&pcode, 0xf110),
            Some(0xf110)
        );
    }

    #[test]
    fn branch_into_the_middle_of_a_block_still_resolves() {
        let pcode = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x4000,
                successors: vec![],
                ops: vec![
                    op(0, 0x4000, PcodeOpcode::Copy, vec![]),
                    op(1, 0x4008, PcodeOpcode::Copy, vec![]),
                    op(2, 0x400c, PcodeOpcode::Return, vec![]),
                ],
            }],
        };
        assert_eq!(
            canonical_block_start_for_address(&pcode, 0x4008),
            Some(0x4000)
        );
        assert_eq!(canonical_block_start_for_address(&pcode, 0x4010), None);
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
                op(
                    1,
                    0x40169f,
                    PcodeOpcode::CBranch,
                    vec![target.clone(), cond],
                ),
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

    #[test]
    fn same_block_forward_branch_does_not_create_cfg_self_edge() {
        let target = Varnode {
            space_id: 3,
            offset: 0x4008,
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
        let pcode = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4000,
                    successors: vec![1],
                    ops: vec![
                        op(0, 0x4000, PcodeOpcode::Copy, vec![]),
                        op(1, 0x4004, PcodeOpcode::CBranch, vec![target, cond]),
                        // The guarded cmov body and its target are still in
                        // this PcodeBasicBlock. The block's real successor is
                        // the following block, not block 0 itself.
                        op(2, 0x4004, PcodeOpcode::Copy, vec![]),
                        op(3, 0x4008, PcodeOpcode::Copy, vec![]),
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4010,
                    successors: vec![],
                    ops: vec![op(4, 0x4010, PcodeOpcode::Return, vec![])],
                },
            ],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let layout_fallthrough = build_layout_fallthrough_map(&pcode);

        assert_eq!(
            build_successor_index_map(&pcode, &address_to_index, &layout_fallthrough),
            vec![vec![1], vec![]],
            "instruction-local forward branch must not become a function CFG self-edge"
        );
    }

    #[test]
    fn backward_branch_at_block_boundary_remains_a_cfg_edge() {
        let target = Varnode {
            space_id: 3,
            offset: 0x5000,
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
        let pcode = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x5000,
                    successors: vec![1],
                    ops: vec![op(0, 0x5000, PcodeOpcode::CBranch, vec![target, cond])],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x5010,
                    successors: vec![],
                    ops: vec![op(1, 0x5010, PcodeOpcode::Return, vec![])],
                },
            ],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let layout_fallthrough = build_layout_fallthrough_map(&pcode);

        assert_eq!(
            build_successor_index_map(&pcode, &address_to_index, &layout_fallthrough),
            vec![vec![0, 1], vec![]],
            "a backward branch to the block header is a real loop edge"
        );
    }
}

#[cfg(test)]
mod indirect_successor_tests {
    use super::*;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    fn branch_ind_block(index: u32, start_address: u64, successors: Vec<u32>) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index,
            start_address,
            successors,
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::BranchInd,
                address: start_address,
                output: None,
                inputs: vec![Varnode::constant(0, 8)],
                asm_mnemonic: None,
            }],
        }
    }

    #[test]
    fn branch_ind_preserves_explicit_successors_with_non_contiguous_ids() {
        let pcode = PcodeFunction {
            blocks: vec![
                branch_ind_block(10, 0x1000, vec![30, 30, 999]),
                PcodeBasicBlock {
                    index: 20,
                    start_address: 0x1010,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x1010,
                        output: None,
                        inputs: vec![],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 30,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![],
                },
            ],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let successors =
            build_successor_index_map(&pcode, &address_to_index, &[Some(1), None, None]);

        assert_eq!(successors, vec![vec![2], vec![], vec![]]);
        assert_eq!(
            structuring_cfg_edges(&pcode),
            vec![crate::cfg::AddressEdge {
                from: 0x1000,
                to: 0x1020,
            }]
        );
    }
}
