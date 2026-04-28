use super::*;
use std::collections::BTreeSet;

fn is_cfg_split_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Branch | PcodeOpcode::CBranch | PcodeOpcode::BranchInd | PcodeOpcode::Return
    )
}

fn direct_control_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => op
            .inputs
            .first()
            .filter(|vn| vn.is_constant)
            .map(|vn| vn.constant_val as u64),
        _ => None,
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
        if is_cfg_split_opcode(op.opcode) {
            if idx + 1 < ops.len() {
                block_starts.insert(idx + 1);
            }
            if let Some(target) = direct_control_target(op) {
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
                    if let Some(target) = direct_control_target(last) {
                        branch_target = Some(target);
                        if let Some(&target_idx) = addr_to_op_idx.get(&target) {
                            push_successor(&mut successors, op_to_block[target_idx]);
                        }
                    }
                }
                PcodeOpcode::CBranch => {
                    branch_input = last.inputs.first().map(format_varnode_diag);
                    if let Some(target) = direct_control_target(last) {
                        branch_target = Some(target);
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
