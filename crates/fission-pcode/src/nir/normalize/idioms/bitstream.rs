use super::super::*;
use std::collections::HashMap;

pub(crate) fn apply_bitstream_idioms(func: &mut HirFunction) -> bool {
    let state_roots = build_slot_state_roots(func);
    let default_state = infer_default_state_expr(&state_roots);
    rewrite_bitstream_stmt_list(&mut func.body, &state_roots, default_state.as_ref())
}

fn build_slot_state_roots(func: &HirFunction) -> HashMap<String, HirExpr> {
    let mut roots = HashMap::new();
    for binding in &func.locals {
        let Some(initializer) = &binding.initializer else {
            continue;
        };
        let Some(root) = peel_state_root_expr(initializer) else {
            continue;
        };
        roots.insert(binding.name.clone(), root);
    }
    roots
}

fn infer_default_state_expr(state_roots: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    let mut roots = state_roots.values();
    let first = roots.next()?.clone();
    if roots.all(|root| *root == first) {
        Some(first)
    } else {
        None
    }
}

fn peel_state_root_expr(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Cast { expr, .. } => peel_state_root_expr(expr),
        HirExpr::PtrOffset { base, .. } => Some((**base).clone()),
        HirExpr::Var(_) => Some(expr.clone()),
        _ => None,
    }
}

fn rewrite_bitstream_stmt_list(
    stmts: &mut Vec<HirStmt>,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let Some(rewritten) = rewrite_flush_bits_if(&stmts[idx], state_roots, default_state) {
            stmts[idx] = rewritten;
            changed = true;
        }
        if idx + 1 < stmts.len()
            && let Some((call_target, value, width, state)) =
                match_write_bits_pair(&stmts[idx], &stmts[idx + 1], state_roots, default_state)
        {
            stmts.splice(
                idx..=idx + 1,
                [HirStmt::Expr(HirExpr::Call {
                    target: call_target,
                    args: vec![state, value, width],
                    ty: NirType::Unknown,
                })],
            );
            changed = true;
            continue;
        }
        match &mut stmts[idx] {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= rewrite_bitstream_stmt_list(body, state_roots, default_state);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        rewrite_bitstream_stmt_list(&mut case.body, state_roots, default_state);
                }
                changed |= rewrite_bitstream_stmt_list(default, state_roots, default_state);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_bitstream_stmt_list(then_body, state_roots, default_state);
                changed |= rewrite_bitstream_stmt_list(else_body, state_roots, default_state);
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
        idx += 1;
    }
    changed
}

fn rewrite_flush_bits_if(
    stmt: &HirStmt,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<HirStmt> {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() || !is_flush_condition(cond) {
        return None;
    }
    if !then_body.iter().any(is_output_store_stmt) {
        return None;
    }
    if !then_body.iter().any(is_pointer_increment_stmt) {
        return None;
    }
    if !then_body.iter().any(is_shift_byte_stmt) {
        return None;
    }
    if !then_body.iter().any(is_bitcount_adjust_stmt) {
        return None;
    }
    let state = infer_state_for_stmts(then_body, state_roots, default_state)?;
    Some(HirStmt::If {
        cond: cond.clone(),
        then_body: vec![HirStmt::Expr(HirExpr::Call {
            target: "FLUSH_BITS".to_string(),
            args: vec![state],
            ty: NirType::Unknown,
        })],
        else_body: Vec::new(),
    })
}

fn match_write_bits_pair(
    first: &HirStmt,
    second: &HirStmt,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<(String, HirExpr, HirExpr, HirExpr)> {
    let (bitcount_key, value) = parse_write_bits_accumulator(first)?;
    let width = parse_bitcount_increment(second, &bitcount_key)?;
    let state =
        infer_state_for_stmts(&[first.clone(), second.clone()], state_roots, default_state)?;
    let call_target = if is_table_lookup_expr(&value) && is_table_lookup_expr(&width) {
        "EMIT_CODE"
    } else {
        "WRITE_BITS"
    };
    Some((call_target.to_string(), value, width, state))
}

fn parse_write_bits_accumulator(stmt: &HirStmt) -> Option<(String, HirExpr)> {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let accum_key = lvalue_location_key(lhs)?;
    let HirExpr::Binary {
        op: HirBinaryOp::Or | HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return None;
    };
    let (value, bitcount_key) = if let Some(parsed) = parse_shifted_value(lhs, rhs, &accum_key) {
        parsed
    } else if let Some(parsed) = parse_shifted_value(rhs, lhs, &accum_key) {
        parsed
    } else {
        return None;
    };
    Some((bitcount_key, value))
}

fn parse_shifted_value<'a>(
    candidate: &'a HirExpr,
    other: &'a HirExpr,
    accum_key: &str,
) -> Option<(HirExpr, String)> {
    if expr_location_key(other).as_deref() != Some(accum_key) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = candidate
    else {
        return None;
    };
    let bitcount_key = expr_location_key(rhs)?;
    Some(((**lhs).clone(), bitcount_key))
}

fn parse_bitcount_increment(stmt: &HirStmt, bitcount_key: &str) -> Option<HirExpr> {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    if lvalue_location_key(lhs).as_deref() != Some(bitcount_key) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return None;
    };
    if expr_location_key(lhs).as_deref() == Some(bitcount_key) {
        return Some((**rhs).clone());
    }
    if expr_location_key(rhs).as_deref() == Some(bitcount_key) {
        return Some((**lhs).clone());
    }
    None
}

fn infer_state_for_stmts(
    stmts: &[HirStmt],
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<HirExpr> {
    if let Some(default_state) = default_state {
        return Some(default_state.clone());
    }
    for stmt in stmts {
        if let Some(state) = infer_state_from_stmt(stmt, state_roots) {
            return Some(state);
        }
    }
    None
}

fn infer_state_from_stmt(
    stmt: &HirStmt,
    state_roots: &HashMap<String, HirExpr>,
) -> Option<HirExpr> {
    match stmt {
        HirStmt::Assign { lhs, rhs } => infer_state_from_lvalue(lhs, state_roots)
            .or_else(|| infer_state_from_expr(rhs, state_roots)),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            infer_state_from_expr(expr, state_roots)
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for stmt in body {
                if let Some(state) = infer_state_from_stmt(stmt, state_roots) {
                    return Some(state);
                }
            }
            None
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => infer_state_from_expr(cond, state_roots)
            .or_else(|| infer_state_for_stmts(then_body, state_roots, None))
            .or_else(|| infer_state_for_stmts(else_body, state_roots, None)),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => infer_state_from_expr(expr, state_roots).or_else(|| {
            for case in cases {
                if let Some(state) = infer_state_for_stmts(&case.body, state_roots, None) {
                    return Some(state);
                }
            }
            infer_state_for_stmts(default, state_roots, None)
        }),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => None,
    }
}

fn infer_state_from_lvalue(
    lhs: &HirLValue,
    state_roots: &HashMap<String, HirExpr>,
) -> Option<HirExpr> {
    match lhs {
        HirLValue::Var(var) => state_roots.get(var).cloned(),
        HirLValue::Deref { ptr, .. } => infer_state_from_expr(ptr, state_roots),
        HirLValue::Index { base, index, .. } => infer_state_from_expr(base, state_roots)
            .or_else(|| infer_state_from_expr(index, state_roots)),
    }
}

fn infer_state_from_expr(
    expr: &HirExpr,
    state_roots: &HashMap<String, HirExpr>,
) -> Option<HirExpr> {
    match expr {
        HirExpr::Var(var) => state_roots.get(var).cloned(),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => infer_state_from_expr(expr, state_roots),
        HirExpr::Binary { lhs, rhs, .. } => infer_state_from_expr(lhs, state_roots)
            .or_else(|| infer_state_from_expr(rhs, state_roots)),
        HirExpr::Call { args, .. } => {
            for arg in args {
                if let Some(state) = infer_state_from_expr(arg, state_roots) {
                    return Some(state);
                }
            }
            None
        }
        HirExpr::Index { base, index, .. } => infer_state_from_expr(base, state_roots)
            .or_else(|| infer_state_from_expr(index, state_roots)),
        HirExpr::Const(_, _) => None,
    }
}

fn is_flush_condition(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Lt | HirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        } if is_const_int(lhs, 7) => !matches!(rhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Le | HirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } if is_const_int(lhs, 8) => !matches!(rhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Lt | HirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        } if is_const_int(rhs, 8) => !matches!(lhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Le | HirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } if is_const_int(rhs, 7) => !matches!(lhs.as_ref(), HirExpr::Const(_, _)),
        _ => false,
    }
}

fn is_output_store_stmt(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Deref { .. } | HirLValue::Index { .. },
            ..
        }
    )
}

fn is_pointer_increment_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Add | HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    (expr_location_key(lhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(rhs, 1))
        || (expr_location_key(rhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(lhs, 1))
}

fn is_shift_byte_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op:
            HirBinaryOp::Shr | HirBinaryOp::Sar | HirBinaryOp::Shl | HirBinaryOp::Div | HirBinaryOp::Mul,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    let matches_self = expr_location_key(lhs).as_deref() == Some(lhs_key.as_str());
    if !matches_self {
        return false;
    }
    is_const_int(rhs, 8) || is_const_int(rhs, 256)
}

fn is_bitcount_adjust_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Sub | HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    expr_location_key(lhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(rhs, 8)
}

fn is_table_lookup_expr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Index { .. } | HirExpr::Load { .. } => true,
        HirExpr::Cast { expr, .. } => is_table_lookup_expr(expr),
        _ => false,
    }
}

fn is_const_int(expr: &HirExpr, expected: i64) -> bool {
    matches!(expr, HirExpr::Const(value, _) if *value == expected)
}

fn lvalue_location_key(lhs: &HirLValue) -> Option<String> {
    match lhs {
        HirLValue::Var(name) => Some(name.clone()),
        HirLValue::Index { base, index, .. } => {
            Some(format!("{}[{}]", print_expr(base), print_expr(index)))
        }
        HirLValue::Deref { .. } => None,
    }
}

fn expr_location_key(expr: &HirExpr) -> Option<String> {
    match expr {
        HirExpr::Var(name) => Some(name.clone()),
        HirExpr::Index { base, index, .. } => {
            Some(format!("{}[{}]", print_expr(base), print_expr(index)))
        }
        HirExpr::Cast { expr, .. } => expr_location_key(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => expr_location_key(base),
        _ => None,
    }
}
