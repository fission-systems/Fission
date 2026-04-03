use super::*;

pub(super) fn build_address_to_index_map(pcode: &PcodeFunction) -> HashMap<u64, usize> {
    let mut address_to_index = HashMap::new();
    for (idx, block) in pcode.blocks.iter().enumerate() {
        address_to_index.entry(block.start_address).or_insert(idx);
    }
    address_to_index
}

const DUPLICATE_BLOCK_KEY_TAG: u64 = 0x8000_0000_0000_0000;

pub(super) fn build_block_target_keys(pcode: &PcodeFunction) -> Vec<u64> {
    let mut seen = HashMap::<u64, u32>::new();
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

fn resolve_instruction_local_branch_target_index(
    pcode: &PcodeFunction,
    _block_idx: usize,
    op: &PcodeOp,
    vn: &Varnode,
) -> Option<usize> {
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
    let target_seq = if delta > 0 {
        op.seq_num.checked_add(delta as u32)?
    } else {
        op.seq_num.checked_sub(delta.unsigned_abs())?
    };

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
