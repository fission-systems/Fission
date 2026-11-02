use crate::prelude::*;

pub fn apply_switch_norm_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    if process_statement_list(&mut func.body) {
        changed = true;
    }
    changed
}

fn process_statement_list(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;

    // 1. Recurse into nested blocks first (bottom-up)
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let PreHirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(init_body),
                        );
                    }
                }
                if let Some(update_stmt) = update {
                    if let PreHirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(update_body),
                        );
                    }
                }
                changed |= process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
                changed |=
                    process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                        &mut case.body,
                    ));
                }
                changed |=
                    process_statement_list(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            }
            _ => {}
        }
    }

    // 2. Process switch guard folding at the current level
    let mut i = 0;
    while i < stmts.len() {
        if let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        {
            if let Some((var_name, op, limit)) = match_var_const_comparison(cond) {
                // The switch sits on one arm; `guard` is the predicate that
                // selects that arm and `other` is the arm that is not it.
                let folded = match (get_single_switch(then_body), get_single_switch(else_body)) {
                    (Some(switch), None) => {
                        fold_guarded_switch(&var_name, op, limit, switch, else_body)
                    }
                    (None, Some(switch)) => negate_cmp_op(op).and_then(|guard| {
                        fold_guarded_switch(&var_name, guard, limit, switch, then_body)
                    }),
                    _ => None,
                };
                if let Some(folded) = folded {
                    stmts[i] = folded;
                    changed = true;
                }
            }
        }
        i += 1;
    }

    changed
}

fn strip_casts(expr: &PreHirExpr) -> &PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &PreHirExpr) -> Option<String> {
    match strip_casts(expr) {
        PreHirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn get_single_switch(
    stmts: &[PreHirStmt],
) -> Option<(&PreHirExpr, &[PreHirSwitchCase], &[PreHirStmt])> {
    let block = if stmts.len() == 1 {
        match &stmts[0] {
            PreHirStmt::Block(inner) => inner.as_slice(),
            _ => stmts,
        }
    } else {
        stmts
    };
    if block.len() == 1 {
        if let PreHirStmt::Switch {
            expr,
            cases,
            default,
        } = &block[0]
        {
            return Some((expr, cases, default));
        }
    }
    None
}

fn is_cmp_op(op: PreHirBinaryOp) -> bool {
    matches!(
        op,
        PreHirBinaryOp::Lt
            | PreHirBinaryOp::Le
            | PreHirBinaryOp::Gt
            | PreHirBinaryOp::Ge
            | PreHirBinaryOp::SLt
            | PreHirBinaryOp::SLe
            | PreHirBinaryOp::SGt
            | PreHirBinaryOp::SGe
            | PreHirBinaryOp::Eq
            | PreHirBinaryOp::Ne
    )
}

fn swap_cmp_op(op: PreHirBinaryOp) -> Option<PreHirBinaryOp> {
    match op {
        PreHirBinaryOp::Lt => Some(PreHirBinaryOp::Gt),
        PreHirBinaryOp::Le => Some(PreHirBinaryOp::Ge),
        PreHirBinaryOp::Gt => Some(PreHirBinaryOp::Lt),
        PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Le),
        PreHirBinaryOp::SLt => Some(PreHirBinaryOp::SGt),
        PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGe),
        PreHirBinaryOp::SGt => Some(PreHirBinaryOp::SLt),
        PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLe),
        PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Eq),
        PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Ne),
        _ => None,
    }
}

fn negate_cmp_op(op: PreHirBinaryOp) -> Option<PreHirBinaryOp> {
    match op {
        PreHirBinaryOp::Lt => Some(PreHirBinaryOp::Ge),
        PreHirBinaryOp::Le => Some(PreHirBinaryOp::Gt),
        PreHirBinaryOp::Gt => Some(PreHirBinaryOp::Le),
        PreHirBinaryOp::Ge => Some(PreHirBinaryOp::Lt),
        PreHirBinaryOp::SLt => Some(PreHirBinaryOp::SGe),
        PreHirBinaryOp::SLe => Some(PreHirBinaryOp::SGt),
        PreHirBinaryOp::SGt => Some(PreHirBinaryOp::SLe),
        PreHirBinaryOp::SGe => Some(PreHirBinaryOp::SLt),
        PreHirBinaryOp::Eq => Some(PreHirBinaryOp::Ne),
        PreHirBinaryOp::Ne => Some(PreHirBinaryOp::Eq),
        _ => None,
    }
}

/// Largest guard range whose cases are enumerated to prove coverage.
const MAX_COVERED_GUARD_RANGE: i64 = 4096;

/// Whether `v <op> limit` holds, or `None` when the constants do not pin it
/// down (an unsigned compare against a value that may be sign-extended).
fn guard_admits(op: PreHirBinaryOp, limit: i64, v: i64) -> Option<bool> {
    match op {
        PreHirBinaryOp::Eq => Some(v == limit),
        PreHirBinaryOp::Ne => Some(v != limit),
        PreHirBinaryOp::SLt => Some(v < limit),
        PreHirBinaryOp::SLe => Some(v <= limit),
        PreHirBinaryOp::SGt => Some(v > limit),
        PreHirBinaryOp::SGe => Some(v >= limit),
        PreHirBinaryOp::Lt | PreHirBinaryOp::Le | PreHirBinaryOp::Gt | PreHirBinaryOp::Ge => {
            if v < 0 || limit < 0 {
                return None;
            }
            Some(match op {
                PreHirBinaryOp::Lt => v < limit,
                PreHirBinaryOp::Le => v <= limit,
                PreHirBinaryOp::Gt => v > limit,
                _ => v >= limit,
            })
        }
        _ => None,
    }
}

/// Whether every value satisfying the guard is one of `labels`, so that no
/// guarded value can reach the switch's `default`.
fn guard_is_covered_by(op: PreHirBinaryOp, limit: i64, labels: &[i64]) -> bool {
    if op == PreHirBinaryOp::Eq {
        return labels.contains(&limit);
    }
    // Unsigned `x < L` / `x <= L` admits exactly 0..=top.
    let top = match op {
        PreHirBinaryOp::Lt => limit.checked_sub(1),
        PreHirBinaryOp::Le => Some(limit),
        _ => None,
    };
    let Some(top) = top.filter(|&top| limit >= 0 && top < MAX_COVERED_GUARD_RANGE) else {
        return false;
    };
    (0..=top).all(|v| labels.contains(&v))
}

/// Folds `if (guard(x)) { switch (x) {..} } else { other }` into one switch
/// when that preserves behaviour for every value of `x`:
/// - every case label must satisfy the guard, otherwise the label would run
///   its body where the original ran `other`;
/// - values that fail the guard run `other`, so the folded `default` must be
///   `other`; values that pass it but match no case run the switch's own
///   `default`. One block can serve as both only when the two are equal, or
///   when the switch has no `default` and the cases cover every guarded value.
fn fold_guarded_switch(
    var_name: &str,
    guard: PreHirBinaryOp,
    limit: i64,
    (sw_expr, sw_cases, sw_default): (&PreHirExpr, &[PreHirSwitchCase], &[PreHirStmt]),
    other: &[PreHirStmt],
) -> Option<PreHirStmt> {
    if get_var_name(sw_expr).as_deref() != Some(var_name) {
        return None;
    }
    let labels: Vec<i64> = sw_cases
        .iter()
        .flat_map(|c| c.values.iter().copied())
        .collect();
    if !labels
        .iter()
        .all(|&v| guard_admits(guard, limit, v) == Some(true))
    {
        return None;
    }
    let default: Vec<PreHirStmt> = if sw_default == other {
        sw_default.to_vec()
    } else if sw_default.is_empty() && guard_is_covered_by(guard, limit, &labels) {
        other.to_vec()
    } else {
        return None;
    };
    Some(PreHirStmt::Switch {
        expr: sw_expr.clone(),
        cases: sw_cases.to_vec(),
        default: default.into(),
    })
}

fn match_var_const_comparison(expr: &PreHirExpr) -> Option<(String, PreHirBinaryOp, i64)> {
    let expr = strip_casts(expr);
    match expr {
        PreHirExpr::Binary { op, lhs, rhs, .. } => match (strip_casts(lhs), strip_casts(rhs)) {
            (PreHirExpr::Var(name), PreHirExpr::Const(val, _)) => {
                if is_cmp_op(*op) {
                    Some((name.clone(), *op, *val))
                } else {
                    None
                }
            }
            (PreHirExpr::Const(val, _), PreHirExpr::Var(name)) => {
                if let Some(swapped_op) = swap_cmp_op(*op) {
                    Some((name.clone(), swapped_op, *val))
                } else {
                    None
                }
            }
            _ => None,
        },
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: inner,
            ..
        } => {
            let (name, inner_op, val) = match_var_const_comparison(inner)?;
            let negated_op = negate_cmp_op(inner_op)?;
            Some((name, negated_op, val))
        }
        _ => None,
    }
}
