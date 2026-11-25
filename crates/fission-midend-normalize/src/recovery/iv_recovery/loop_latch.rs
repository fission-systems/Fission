use super::loop_expressions::strip_casts;
use crate::prelude::*;

pub(super) fn tail_meaningful_stmts(
    body: &[PreHirStmt],
    count: usize,
) -> Option<Vec<(usize, PreHirStmt)>> {
    let mut tail = Vec::new();
    for (idx, stmt) in body.iter().enumerate().rev() {
        if matches!(stmt, PreHirStmt::Label(_)) {
            continue;
        }
        tail.push((idx, stmt.clone()));
        if tail.len() == count {
            tail.reverse();
            return Some(tail);
        }
    }
    None
}

pub(super) fn parse_self_add_assign(stmt: &PreHirStmt) -> Option<(String, PreHirExpr)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs: addend,
        ..
    } = strip_casts(rhs)
    else {
        return None;
    };
    if matches!(strip_casts(lhs.as_ref()), PreHirExpr::Var(var) if var == name) {
        Some((name.clone(), (*addend).as_ref().clone()))
    } else if matches!(strip_casts(addend.as_ref()), PreHirExpr::Var(var) if var == name) {
        Some((name.clone(), (*lhs).as_ref().clone()))
    } else {
        None
    }
}

pub(super) fn parse_break_eq_var(stmt: &PreHirStmt) -> Option<(PreHirExpr, String)> {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() || !matches!(then_body.as_slice(), [PreHirStmt::Break]) {
        return None;
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = strip_casts(cond)
    else {
        return None;
    };
    if let PreHirExpr::Var(var) = strip_casts(rhs.as_ref()) {
        Some(((*lhs).as_ref().clone(), var.clone()))
    } else if let PreHirExpr::Var(var) = strip_casts(lhs.as_ref()) {
        Some(((*rhs).as_ref().clone(), var.clone()))
    } else {
        None
    }
}
