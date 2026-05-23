use super::*;
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

pub fn build_cfg_blocks(
    entry_address: u64,
    ops: Vec<PcodeOp>,
    indirect_targets: &BTreeSet<u64>,
) -> Vec<PcodeBasicBlock> {
    if ops.is_empty() {
        return Vec::new();
    }

    cfg_build_diag_log(
        entry_address,
        &format!("start entry=0x{:x} op_count={}", entry_address, ops.len()),
    );

    let mut addr_to_op_idx: HashMap<u64, usize> = HashMap::new();
    let mut op_seq_to_idx: HashMap<(u64, u32), usize> = HashMap::new();
    for (idx, op) in ops.iter().enumerate() {
        addr_to_op_idx.entry(op.address).or_insert(idx);
        op_seq_to_idx.insert((op.address, op.seq_num), idx);
    }

    let mut block_starts: BTreeSet<usize> = BTreeSet::new();
    block_starts.insert(0);

    for target in indirect_targets {
        if let Some(&target_idx) = addr_to_op_idx.get(target) {
            block_starts.insert(target_idx);
        }
    }

    for (idx, op) in ops.iter().enumerate() {
        if is_cfg_split_opcode(op.opcode) {
            if idx + 1 < ops.len() {
                block_starts.insert(idx + 1);
            }
            if let Some(DirectControlTarget::OpIndex(target_idx)) =
                direct_control_target_for_cfg(op, &addr_to_op_idx, &op_seq_to_idx)
            {
                block_starts.insert(target_idx);
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
        let block_ops = ops[*start..end].to_vec();

        let mut successors = Vec::new();
        let mut branch_target = None;
        let mut branch_input = None;
        if let Some(last) = block_ops.last() {
            match last.opcode {
                PcodeOpcode::Branch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) =
                        direct_control_target_for_cfg(last, &addr_to_op_idx, &op_seq_to_idx)
                    {
                        match target {
                            DirectControlTarget::OpIndex(target_idx) => {
                                branch_target = Some(ops[target_idx].address);
                                push_successor(&mut successors, op_to_block[target_idx]);
                            }
                            DirectControlTarget::Address(target) => {
                                branch_target = Some(target);
                            }
                        }
                    }
                }
                PcodeOpcode::CBranch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) =
                        direct_control_target_for_cfg(last, &addr_to_op_idx, &op_seq_to_idx)
                    {
                        match target {
                            DirectControlTarget::OpIndex(target_idx) => {
                                branch_target = Some(ops[target_idx].address);
                                push_successor(&mut successors, op_to_block[target_idx]);
                            }
                            DirectControlTarget::Address(target) => {
                                branch_target = Some(target);
                            }
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
        blocks.push(PcodeBasicBlock {
            index: block_idx as u32,
            start_address,
            successors,
            ops: block_ops,
        });
    }

    blocks
}
