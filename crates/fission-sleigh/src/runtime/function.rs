use super::*;
use fission_pcode::cfg::{AddressCfgSnapshot, AddressEdge};
use std::collections::BTreeSet;

fn is_cfg_split_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch | PcodeOpcode::CBranch | PcodeOpcode::BranchInd | PcodeOpcode::Return
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectControlTarget {
    Address(u64),
    OpIndex(usize),
}

fn direct_control_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op.inputs.first().and_then(|vn| {
            if vn.is_constant {
                if vn.offset != 0 {
                    Some(vn.offset)
                } else if vn.constant_val >= 0 {
                    Some(vn.constant_val as u64)
                } else {
                    None
                }
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn direct_control_target_with_symbolic_internal_label(op: &PcodeOp) -> Option<u64> {
    direct_control_target(op).or_else(|| match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op
            .inputs
            .first()
            .filter(|vn| !vn.is_constant && vn.offset != 0)
            .map(|vn| vn.offset),
        _ => None,
    })
}

fn relative_pcode_target_seq(op: &PcodeOp, vn: &Varnode) -> Option<u32> {
    if vn.space_id != 0 || !vn.is_constant {
        return None;
    }
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

fn direct_control_target_for_cfg(
    op: &PcodeOp,
    addr_to_op_idx: &HashMap<u64, usize>,
    op_seq_to_idx: &HashMap<(u64, u32), usize>,
) -> Option<DirectControlTarget> {
    if matches!(op.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch) {
        if let Some(target_idx) = op
            .inputs
            .first()
            .and_then(|input| relative_pcode_target_seq(op, input))
            .and_then(|target_seq| op_seq_to_idx.get(&(op.address, target_seq)))
            .copied()
        {
            return Some(DirectControlTarget::OpIndex(target_idx));
        }
    }

    let target = direct_control_target_with_symbolic_internal_label(op)?;
    if let Some(&target_idx) = addr_to_op_idx.get(&target) {
        Some(DirectControlTarget::OpIndex(target_idx))
    } else {
        Some(DirectControlTarget::Address(target))
    }
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

fn instruction_metadata_from_ops(ops: &[PcodeOp]) -> (Vec<u64>, BTreeMap<u64, u64>) {
    let mut addrs: Vec<u64> = ops.iter().map(|op| op.address).collect();
    addrs.sort_unstable();
    addrs.dedup();
    let mut lengths = BTreeMap::new();
    for (idx, &addr) in addrs.iter().enumerate() {
        let len = if idx + 1 < addrs.len() {
            addrs[idx + 1].checked_sub(addr).unwrap_or(1).max(1)
        } else {
            1
        };
        lengths.insert(addr, len);
    }
    (addrs, lengths)
}

/// Build pcode blocks using Ghidra-style instruction-level CFG boundaries.
pub fn build_cfg_blocks(
    entry_address: u64,
    reachable_instruction_addresses: &[u64],
    instruction_lengths: &BTreeMap<u64, u64>,
    ops: Vec<PcodeOp>,
    indirect_targets: &BTreeSet<u64>,
    inferred_indirect_edges: &BTreeMap<u64, Vec<u64>>,
    block_entry_hints: &BTreeSet<u64>,
) -> Vec<PcodeBasicBlock> {
    build_cfg_blocks_with_hints(
        entry_address,
        reachable_instruction_addresses,
        instruction_lengths,
        ops,
        indirect_targets,
        inferred_indirect_edges,
        &InstructionCfgHints {
            block_leaders: block_entry_hints.clone(),
            ..InstructionCfgHints::default()
        },
    )
}

/// Build pcode blocks using Ghidra-style instruction-level CFG boundaries.
pub fn build_cfg_blocks_with_hints(
    entry_address: u64,
    reachable_instruction_addresses: &[u64],
    instruction_lengths: &BTreeMap<u64, u64>,
    ops: Vec<PcodeOp>,
    indirect_targets: &BTreeSet<u64>,
    inferred_indirect_edges: &BTreeMap<u64, Vec<u64>>,
    cfg_hints: &InstructionCfgHints,
) -> Vec<PcodeBasicBlock> {
    if ops.is_empty() {
        return Vec::new();
    }

    cfg_build_diag_log(
        entry_address,
        &format!(
            "start entry=0x{:x} op_count={} instr_count={}",
            entry_address,
            ops.len(),
            reachable_instruction_addresses.len()
        ),
    );

    let reachable: BTreeSet<u64> = reachable_instruction_addresses.iter().copied().collect();
    let snapshot = build_instruction_cfg_snapshot(
        entry_address,
        reachable_instruction_addresses,
        instruction_lengths,
        &ops,
        indirect_targets,
        inferred_indirect_edges,
        cfg_hints,
        false,
    );

    let leaders = snapshot.block_starts;
    let leader_to_index: HashMap<u64, u32> = leaders
        .iter()
        .enumerate()
        .map(|(idx, leader)| (*leader, idx as u32))
        .collect();

    let mut blocks: Vec<PcodeBasicBlock> = leaders
        .iter()
        .enumerate()
        .map(|(idx, &start_address)| PcodeBasicBlock {
            index: idx as u32,
            start_address,
            successors: Vec::new(),
            ops: Vec::new(),
        })
        .collect();

    for op in ops {
        let Some(leader) = leader_for_target(op.address, &leaders, &reachable) else {
            continue;
        };
        let Some(&block_idx) = leader_to_index.get(&leader) else {
            continue;
        };
        blocks[block_idx as usize].ops.push(op);
    }

    for edge in snapshot.edges {
        let Some(&from_idx) = leader_to_index.get(&edge.from) else {
            continue;
        };
        let Some(&to_idx) = leader_to_index.get(&edge.to) else {
            continue;
        };
        push_successor(&mut blocks[from_idx as usize].successors, to_idx);
    }

    for block in &mut blocks {
        block.successors.sort_unstable();
        block.successors.dedup();
    }

    blocks
}

/// Convenience wrapper for unit tests that only provide flattened pcode ops.
pub fn build_cfg_blocks_from_ops(
    entry_address: u64,
    ops: Vec<PcodeOp>,
    indirect_targets: &BTreeSet<u64>,
) -> Vec<PcodeBasicBlock> {
    let (reachable, lengths) = instruction_metadata_from_ops(&ops);
    build_cfg_blocks(
        entry_address,
        &reachable,
        &lengths,
        ops,
        indirect_targets,
        &BTreeMap::new(),
        &BTreeSet::new(),
    )
}

fn instruction_fallthrough(address: u64, instruction_lengths: &BTreeMap<u64, u64>) -> Option<u64> {
    let len = instruction_lengths.get(&address).copied()?;
    address.checked_add(len)
}

fn build_op_index_maps(ops: &[PcodeOp]) -> HashMap<(u64, u32), usize> {
    let mut op_seq_to_idx = HashMap::new();
    for (idx, op) in ops.iter().enumerate() {
        op_seq_to_idx.insert((op.address, op.seq_num), idx);
    }
    op_seq_to_idx
}

fn control_op_branch_target_address(
    op: &PcodeOp,
    ops: &[PcodeOp],
    op_seq_to_idx: &HashMap<(u64, u32), usize>,
) -> Option<u64> {
    if !matches!(op.opcode, PcodeOpcode::Branch | PcodeOpcode::CBranch) {
        return None;
    }
    if let Some(input) = op.inputs.first() {
        if let Some(target_seq) = relative_pcode_target_seq(op, input) {
            if let Some(&idx) = op_seq_to_idx.get(&(op.address, target_seq)) {
                return Some(ops[idx].address);
            }
        }
    }
    direct_control_target_with_symbolic_internal_label(op)
}

fn control_op_splits_instruction_cfg(
    op: &PcodeOp,
    ops: &[PcodeOp],
    ops_at: &[&PcodeOp],
    op_seq_to_idx: &HashMap<(u64, u32), usize>,
    instruction_lengths: &BTreeMap<u64, u64>,
) -> bool {
    match op.opcode {
        PcodeOpcode::Return | PcodeOpcode::BranchInd => true,
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            if let Some(target_addr) = control_op_branch_target_address(op, ops, op_seq_to_idx) {
                if target_addr == op.address {
                    return false;
                }
            }
            let Some(fallthrough) = instruction_fallthrough(op.address, instruction_lengths) else {
                return true;
            };
            let branch_target = control_op_branch_target_address(op, ops, op_seq_to_idx)
                .or_else(|| direct_control_target_with_symbolic_internal_label(op));
            match branch_target {
                Some(target) if target == op.address => false,
                Some(target) if target != fallthrough => true,
                Some(_) if op.opcode == PcodeOpcode::CBranch => {
                    cbranch_equal_target_is_cfg_split(op.address, fallthrough, ops_at)
                }
                Some(_) => false,
                None => true,
            }
        }
        _ => false,
    }
}

fn instruction_cfg_control_terminator<'a>(
    ops_at: &[&'a PcodeOp],
    ops: &[PcodeOp],
    op_seq_to_idx: &HashMap<(u64, u32), usize>,
    instruction_lengths: &BTreeMap<u64, u64>,
) -> Option<&'a PcodeOp> {
    ops_at.iter().copied().find(|op| {
        is_cfg_split_opcode(op.opcode)
            && control_op_splits_instruction_cfg(
                op,
                ops,
                ops_at,
                op_seq_to_idx,
                instruction_lengths,
            )
    })
}

fn instruction_control_terminator<'a>(ops: &[&'a PcodeOp]) -> Option<&'a PcodeOp> {
    // Prefer the first control-flow op within an instruction. Some ISAs (for example
    // ARM Thumb IT) emit CBranch before Return/CallOther in the same instruction;
    // the last control op would hide the fallthrough edge.
    ops.iter()
        .copied()
        .find(|op| is_cfg_split_opcode(op.opcode))
}

fn cbranch_shares_instruction_with_return(ops_at: &[&PcodeOp]) -> bool {
    let Some(cbranch_idx) = ops_at
        .iter()
        .position(|op| op.opcode == PcodeOpcode::CBranch)
    else {
        return false;
    };
    ops_at[cbranch_idx..]
        .iter()
        .any(|op| op.opcode == PcodeOpcode::Return)
}

fn cbranch_equal_target_is_cfg_split(addr: u64, fallthrough: u64, ops_at: &[&PcodeOp]) -> bool {
    fallthrough != addr && cbranch_shares_instruction_with_return(ops_at)
}

fn leader_for_target(target: u64, leaders: &[u64], reachable: &BTreeSet<u64>) -> Option<u64> {
    if !reachable.contains(&target) {
        return None;
    }
    if leaders.binary_search(&target).is_ok() {
        return Some(target);
    }
    let idx = leaders.partition_point(|leader| *leader <= target);
    if idx == 0 {
        return None;
    }
    Some(leaders[idx - 1])
}

fn push_address_edge(edges: &mut Vec<AddressEdge>, from: u64, to: u64) {
    let edge = AddressEdge { from, to };
    if !edges.contains(&edge) {
        edges.push(edge);
    }
}

/// Instruction-level CFG aligned with Ghidra `BasicBlockModel` (call fall-through, nop leaders).
pub fn build_instruction_cfg_snapshot(
    entry_address: u64,
    reachable_instruction_addresses: &[u64],
    instruction_lengths: &BTreeMap<u64, u64>,
    ops: &[PcodeOp],
    indirect_targets: &BTreeSet<u64>,
    inferred_indirect_edges: &BTreeMap<u64, Vec<u64>>,
    cfg_hints: &InstructionCfgHints,
    stop_at_indirect_branch: bool,
) -> AddressCfgSnapshot {
    let reachable: BTreeSet<u64> = reachable_instruction_addresses.iter().copied().collect();
    if reachable.is_empty() {
        return AddressCfgSnapshot {
            model: "pcode_instruction_cfg".to_string(),
            function_address: entry_address,
            block_starts: Vec::new(),
            edges: Vec::new(),
            exit_blocks: Vec::new(),
        };
    }

    let mut ops_by_addr: HashMap<u64, Vec<&PcodeOp>> = HashMap::new();
    for op in ops {
        ops_by_addr.entry(op.address).or_default().push(op);
    }
    let op_seq_to_idx = build_op_index_maps(ops);

    let mut leaders = BTreeSet::new();
    if reachable.contains(&entry_address) {
        leaders.insert(entry_address);
    } else if let Some(&first) = reachable_instruction_addresses.first() {
        leaders.insert(first);
    }
    for target in indirect_targets {
        if reachable.contains(target) {
            leaders.insert(*target);
        }
    }
    for hint in &cfg_hints.block_leaders {
        if reachable.contains(hint) {
            leaders.insert(*hint);
        }
    }

    let noreturn = &cfg_hints.noreturn_callsites;

    for &addr in reachable_instruction_addresses {
        let Some(ops_at) = ops_by_addr.get(&addr) else {
            continue;
        };

        let Some(term) =
            instruction_cfg_control_terminator(ops_at, ops, &op_seq_to_idx, instruction_lengths)
        else {
            continue;
        };

        match term.opcode {
            PcodeOpcode::Branch => {
                if let Some(target) = direct_control_target_with_symbolic_internal_label(term) {
                    leaders.insert(target);
                }
            }
            PcodeOpcode::CBranch => {
                let branch_target = direct_control_target_with_symbolic_internal_label(term);
                if let Some(fallthrough) = instruction_fallthrough(addr, instruction_lengths) {
                    if branch_target != Some(fallthrough) {
                        if let Some(target) = branch_target {
                            leaders.insert(target);
                        }
                        if reachable.contains(&fallthrough) {
                            leaders.insert(fallthrough);
                        }
                    } else if cbranch_equal_target_is_cfg_split(addr, fallthrough, ops_at) {
                        if reachable.contains(&fallthrough) {
                            leaders.insert(fallthrough);
                        }
                    }
                } else if let Some(target) = branch_target {
                    leaders.insert(target);
                }
            }
            PcodeOpcode::BranchInd => {
                if !stop_at_indirect_branch {
                    if let Some(targets) = inferred_indirect_edges.get(&addr) {
                        for target in targets {
                            if reachable.contains(target) {
                                leaders.insert(*target);
                            }
                        }
                    }
                    for target in indirect_targets {
                        if reachable.contains(target) {
                            leaders.insert(*target);
                        }
                    }
                }
            }
            PcodeOpcode::Return => {}
            _ => {}
        }
    }

    let block_starts: Vec<u64> = leaders
        .into_iter()
        .filter(|leader| reachable.contains(leader))
        .collect();

    let mut edges = Vec::new();
    let mut exit_blocks = Vec::new();

    for (idx, &leader) in block_starts.iter().enumerate() {
        let next_leader = block_starts.get(idx + 1).copied();
        let block_addrs: Vec<u64> = reachable_instruction_addresses
            .iter()
            .copied()
            .filter(|addr| *addr >= leader && next_leader.is_none_or(|next| *addr < next))
            .collect();
        let Some(&last_addr) = block_addrs.last() else {
            continue;
        };

        let mut successors = Vec::new();
        if let Some(ops_at) = ops_by_addr.get(&last_addr) {
            if let Some(term) =
                instruction_cfg_control_terminator(ops_at, ops, &op_seq_to_idx, instruction_lengths)
            {
                match term.opcode {
                    PcodeOpcode::Branch => {
                        if let Some(target) =
                            direct_control_target_with_symbolic_internal_label(term)
                        {
                            if let Some(dst) = leader_for_target(target, &block_starts, &reachable)
                            {
                                successors.push(dst);
                            }
                        }
                    }
                    PcodeOpcode::CBranch => {
                        let branch_target =
                            direct_control_target_with_symbolic_internal_label(term);
                        if let Some(fallthrough) =
                            instruction_fallthrough(last_addr, instruction_lengths)
                        {
                            if branch_target != Some(fallthrough) {
                                if let Some(target) = branch_target {
                                    if let Some(dst) =
                                        leader_for_target(target, &block_starts, &reachable)
                                    {
                                        successors.push(dst);
                                    }
                                }
                                if let Some(dst) =
                                    leader_for_target(fallthrough, &block_starts, &reachable)
                                {
                                    successors.push(dst);
                                }
                            } else if let Some(ops_at) = ops_by_addr.get(&last_addr) {
                                if cbranch_equal_target_is_cfg_split(last_addr, fallthrough, ops_at)
                                {
                                    if let Some(dst) =
                                        leader_for_target(fallthrough, &block_starts, &reachable)
                                    {
                                        successors.push(dst);
                                    }
                                } else if let Some(dst) =
                                    leader_for_target(fallthrough, &block_starts, &reachable)
                                {
                                    // cmov-style CBranch: pcode models conditional dataflow, not
                                    // an instruction-level CFG split, but execution still falls through.
                                    successors.push(dst);
                                }
                            }
                        } else if let Some(target) = branch_target {
                            if let Some(dst) = leader_for_target(target, &block_starts, &reachable)
                            {
                                successors.push(dst);
                            }
                        }
                    }
                    PcodeOpcode::Return => {}
                    PcodeOpcode::BranchInd => {
                        if !stop_at_indirect_branch {
                            let mut indirect = inferred_indirect_edges
                                .get(&last_addr)
                                .cloned()
                                .unwrap_or_default();
                            indirect.extend(
                                indirect_targets
                                    .iter()
                                    .copied()
                                    .filter(|target| reachable.contains(target)),
                            );
                            indirect.sort_unstable();
                            indirect.dedup();
                            for target in indirect {
                                if let Some(dst) =
                                    leader_for_target(target, &block_starts, &reachable)
                                {
                                    successors.push(dst);
                                }
                            }
                        }
                    }
                    _ => {
                        if !noreturn.contains(&last_addr) {
                            if let Some(fallthrough) =
                                instruction_fallthrough(last_addr, instruction_lengths)
                            {
                                if let Some(dst) =
                                    leader_for_target(fallthrough, &block_starts, &reachable)
                                {
                                    successors.push(dst);
                                }
                            }
                        }
                    }
                }
            } else if !noreturn.contains(&last_addr) {
                if let Some(fallthrough) = instruction_fallthrough(last_addr, instruction_lengths) {
                    if let Some(dst) = leader_for_target(fallthrough, &block_starts, &reachable) {
                        successors.push(dst);
                    }
                }
            }
        } else if !noreturn.contains(&last_addr) {
            if let Some(fallthrough) = instruction_fallthrough(last_addr, instruction_lengths) {
                if let Some(dst) = leader_for_target(fallthrough, &block_starts, &reachable) {
                    successors.push(dst);
                }
            }
        }

        successors.sort_unstable();
        successors.dedup();
        if successors.is_empty() {
            exit_blocks.push(leader);
        }
        for dst in successors {
            push_address_edge(&mut edges, leader, dst);
        }
    }

    for (from_instr, to) in &cfg_hints.flow_edges {
        if !reachable.contains(from_instr) || !reachable.contains(to) {
            continue;
        }
        let Some(from_leader) = leader_for_target(*from_instr, &block_starts, &reachable) else {
            continue;
        };
        let Some(to_leader) = leader_for_target(*to, &block_starts, &reachable) else {
            continue;
        };
        push_address_edge(&mut edges, from_leader, to_leader);
    }

    let mut snapshot = AddressCfgSnapshot {
        model: "pcode_instruction_cfg".to_string(),
        function_address: entry_address,
        block_starts,
        edges,
        exit_blocks,
    };
    snapshot.canonicalize();
    snapshot
}

#[cfg(test)]
mod instruction_cfg_tests {
    use super::*;

    fn const_vn(val: u64) -> Varnode {
        Varnode {
            space_id: 0,
            offset: val,
            size: 8,
            is_constant: true,
            constant_val: val as i64,
        }
    }

    #[test]
    fn instruction_cfg_includes_nop_only_leader_block() {
        let entry = 0x1000u64;
        let nop = 0x1004u64;
        let tail = 0x1005u64;
        let reachable = vec![entry, nop, tail];
        let mut lengths = BTreeMap::new();
        lengths.insert(entry, 4);
        lengths.insert(nop, 1);
        lengths.insert(tail, 1);

        let ops = vec![
            PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::CBranch,
                address: entry,
                output: None,
                inputs: vec![const_vn(0x2000)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Return,
                address: tail,
                output: None,
                inputs: vec![],
                asm_mnemonic: None,
            },
        ];

        let snapshot = build_instruction_cfg_snapshot(
            entry,
            &reachable,
            &lengths,
            &ops,
            &BTreeSet::new(),
            &BTreeMap::new(),
            &InstructionCfgHints::default(),
            true,
        );

        assert!(snapshot.block_starts.contains(&nop));
        assert!(snapshot
            .edges
            .iter()
            .any(|edge| edge.from == entry && edge.to == nop));
    }
}
