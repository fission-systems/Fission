//! Loop-shape folds: seeded `while` to `for`, `while (1) { if (!c) break; }`
//! to `while (c)`.

use super::*;

/// `i = init; while (cond) { body…; i = update; }` → `for (i = init; cond; i = update)`.
///
/// Presentation-only: init/update must be presentation-pure. Body may contain
/// calls (loop body is not reordered). Rejects multi-def induction or missing tail.
pub(super) fn fold_seed_while_to_for(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_seed_while_to_for(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_seed_while_to_for(then_body);
                changed |= fold_seed_while_to_for(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_seed_while_to_for(&mut case.body);
                }
                changed |= fold_seed_while_to_for(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let Some((ind_name, _)) = pure_var_assign(&stmts[i]) else {
            i += 1;
            continue;
        };
        let ind_name = ind_name.to_string();
        let HirStmt::While { cond, body } = &stmts[i + 1] else {
            i += 1;
            continue;
        };
        if body.is_empty() {
            i += 1;
            continue;
        }
        let Some((upd_name, _)) = pure_var_assign(body.last().unwrap()) else {
            i += 1;
            continue;
        };
        if upd_name != ind_name {
            i += 1;
            continue;
        }
        // Cond should mention induction (avoid folding unrelated seed+while).
        if count_uses_in_expr(cond, &ind_name) == 0 {
            i += 1;
            continue;
        }
        // Body (excluding trailing update) must not re-assign induction or
        // contain uncaptured `continue` (which in `while` skips trailing update,
        // whereas in `for` it runs `For.update`).
        let body_core = &body[..body.len() - 1];
        if body_core.iter().any(|s| assigns_var_name(s, &ind_name))
            || has_uncaptured_continue(body_core)
        {
            i += 1;
            continue;
        }

        let init_stmt = stmts[i].clone();
        let HirStmt::While { cond, body } = stmts[i + 1].clone() else {
            unreachable!();
        };
        let update_stmt = body.last().cloned().unwrap();
        let for_body: Vec<HirStmt> = body[..body.len() - 1].to_vec();
        stmts[i] = HirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: for_body,
        };
        stmts.remove(i + 1);
        changed = true;
        // Re-scan from same index (new For may nest folds later via recursion).
        i += 1;
    }
    changed
}

pub(super) fn has_uncaptured_continue(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        HirStmt::Continue => true,
        HirStmt::Block(body) => has_uncaptured_continue(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => has_uncaptured_continue(then_body) || has_uncaptured_continue(else_body),
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| has_uncaptured_continue(&case.body))
                || has_uncaptured_continue(default)
        }
        HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => false,
        _ => false,
    })
}

pub(super) fn assigns_var_name(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(n),
            ..
        } => n == name,
        HirStmt::Block(b) | HirStmt::While { body: b, .. } | HirStmt::DoWhile { body: b, .. } => {
            b.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().is_some_and(|s| assigns_var_name(s, name))
                || update.as_ref().is_some_and(|s| assigns_var_name(s, name))
                || body.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| assigns_var_name(s, name))
                || else_body.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| assigns_var_name(s, name)))
                || default.iter().any(|s| assigns_var_name(s, name))
        }
        _ => false,
    }
}

/// `while (1) { if (!cond) break; body… }` → `while (cond) { body… }`.
pub(super) fn fold_while_true_break_guard(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_while_true_break_guard(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_while_true_break_guard(then_body);
                changed |= fold_while_true_break_guard(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_while_true_break_guard(&mut case.body);
                }
                changed |= fold_while_true_break_guard(default);
            }
            _ => {}
        }
    }

    for stmt in stmts.iter_mut() {
        let HirStmt::While { cond, body } = stmt else {
            continue;
        };
        if !expr_is_constant_true(cond) {
            continue;
        }
        if body.is_empty() {
            continue;
        }
        // Leading: if (<guard>) break;
        let guard_cond = match &body[0] {
            HirStmt::If {
                cond: g,
                then_body,
                else_body,
            } if else_body.is_empty() && matches!(then_body.as_slice(), [HirStmt::Break]) => {
                g.clone()
            }
            _ => continue,
        };

        // while (1) { if (!c) break; … } → while (c)
        // while (1) { if (c) break; … }  → while (!c)
        let new_cond = match peel_not(&guard_cond) {
            Some(inner) => inner,
            None => goto_recovery::invert_cond(guard_cond),
        };
        *cond = new_cond;
        body.remove(0);
        changed = true;
    }
    changed
}

pub(super) fn expr_is_constant_true(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(v, _) => *v != 0,
        HirExpr::Cast { expr, .. } => expr_is_constant_true(expr),
        _ => false,
    }
}

pub(super) fn peel_not(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => Some(expr.as_ref().clone()),
        // `x == 0` / `x == false` → peel to truthiness of x as break-on-zero guard.
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if matches!(rhs.as_ref(), HirExpr::Const(0, _)) => Some(lhs.as_ref().clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if matches!(lhs.as_ref(), HirExpr::Const(0, _)) => Some(rhs.as_ref().clone()),
        _ => None,
    }
}
