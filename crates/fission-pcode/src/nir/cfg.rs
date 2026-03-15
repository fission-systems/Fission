use super::*;

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
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
                    }
                }
                Some(op)
                    if op.opcode == PcodeOpcode::CBranch
                        || (op.opcode == PcodeOpcode::Branch && op.inputs.len() >= 2) =>
                {
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
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
    let mut order = pcode
        .blocks
        .iter()
        .enumerate()
        .map(|(idx, block)| (block.start_address, idx))
        .collect::<Vec<_>>();
    order.sort_unstable_by_key(|(addr, _)| *addr);

    let mut layout_fallthrough = vec![None; pcode.blocks.len()];
    for pair in order.windows(2) {
        let (_, idx) = pair[0];
        let (_, next_idx) = pair[1];
        layout_fallthrough[idx] = Some(next_idx);
    }
    layout_fallthrough
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

pub(super) fn block_label(address: u64) -> String {
    format!("block_{:x}", address)
}

pub(super) fn fold_logical_chain(mut exprs: Vec<HirExpr>, op: HirBinaryOp) -> HirExpr {
    debug_assert!(matches!(
        op,
        HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr
    ));
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
