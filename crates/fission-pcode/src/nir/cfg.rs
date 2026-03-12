use super::*;

pub(super) fn build_successor_index_map(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
) -> Vec<Vec<usize>> {
    pcode.blocks
        .iter()
        .enumerate()
        .map(|(idx, block)| {
            let mut succs = Vec::new();
            match block_terminator_op(block) {
                Some(op) if op.opcode == PcodeOpcode::Return => {}
                Some(op) if op.opcode == PcodeOpcode::Branch => {
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
                    }
                }
                Some(op) if op.opcode == PcodeOpcode::CBranch => {
                    if let Some(target) = op.inputs.first().and_then(branch_target_address) {
                        if let Some(target_idx) = address_to_index.get(&target) {
                            succs.push(*target_idx);
                        }
                    }
                    if idx + 1 < pcode.blocks.len() {
                        succs.push(idx + 1);
                    }
                }
                Some(op) if op.opcode == PcodeOpcode::BranchInd => {}
                _ => {
                    if idx + 1 < pcode.blocks.len() {
                        succs.push(idx + 1);
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
    } else {
        None
    }
}

pub(super) fn block_label(address: u64) -> String {
    format!("block_{:x}", address)
}

pub(super) fn fold_logical_chain(mut exprs: Vec<HirExpr>, op: HirBinaryOp) -> HirExpr {
    debug_assert!(matches!(op, HirBinaryOp::LogicalAnd | HirBinaryOp::LogicalOr));
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
