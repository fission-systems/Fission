//! Statement-level folds: redundant blocks, pure if/else value joins,
//! empty-arm cleanup.

use super::*;

pub(super) fn flatten_redundant_blocks(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in std::mem::take(stmts) {
        match stmt {
            HirStmt::Block(mut body) => {
                changed |= flatten_redundant_blocks(&mut body);
                // Unwrap brace-only wrappers into the parent sequence for HIR.
                out.extend(body);
                changed = true;
            }
            mut other => {
                match &mut other {
                    HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                        changed |= flatten_redundant_blocks(body);
                    }
                    HirStmt::If {
                        then_body,
                        else_body,
                        ..
                    } => {
                        changed |= flatten_redundant_blocks(then_body);
                        changed |= flatten_redundant_blocks(else_body);
                    }
                    HirStmt::For { body, .. } => {
                        changed |= flatten_redundant_blocks(body);
                    }
                    HirStmt::Switch { cases, default, .. } => {
                        for case in cases {
                            changed |= flatten_redundant_blocks(&mut case.body);
                        }
                        changed |= flatten_redundant_blocks(default);
                    }
                    _ => {}
                }
                out.push(other);
            }
        }
    }
    *stmts = out;
    changed
}

/// `if (c) { x = a; } else { x = b; }` → `x = c ? a : b` when c/a/b are presentation-pure.
pub(super) fn fold_if_else_pure_same_var_assign(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_if_else_pure_same_var_assign(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_if_else_pure_same_var_assign(then_body);
                changed |= fold_if_else_pure_same_var_assign(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_if_else_pure_same_var_assign(&mut case.body);
                }
                changed |= fold_if_else_pure_same_var_assign(default);
            }
            _ => {}
        }
    }

    for stmt in stmts.iter_mut() {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            continue;
        };
        if else_body.is_empty() || !expr_is_presentation_pure(cond) {
            continue;
        }
        let Some((then_name, then_rhs)) = single_var_assign(then_body) else {
            continue;
        };
        let Some((else_name, else_rhs)) = single_var_assign(else_body) else {
            continue;
        };
        if then_name != else_name {
            continue;
        }
        if !expr_is_presentation_pure(then_rhs) || !expr_is_presentation_pure(else_rhs) {
            continue;
        }
        // Avoid self-referential select: `x = c ? x : e` would be wrong if x is live-in.
        if expr_mentions_var(cond, then_name)
            || expr_mentions_var(then_rhs, then_name)
            || expr_mentions_var(else_rhs, then_name)
        {
            continue;
        }
        let ty = expr_result_type(then_rhs);
        *stmt = HirStmt::Assign {
            lhs: HirLValue::Var(then_name.to_string()),
            rhs: HirExpr::Select {
                cond: Box::new(cond.clone()),
                then_expr: Box::new(then_rhs.clone()),
                else_expr: Box::new(else_rhs.clone()),
                ty,
            },
        };
        changed = true;
    }
    changed
}

/// `if (c) { return a; } return b;` → `return c ? a : b` when operands are pure.
pub(super) fn fold_if_return_fallthrough_return(
    stmts: &mut Vec<HirStmt>,
    goto_targets: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_if_return_fallthrough_return(b, goto_targets);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_if_return_fallthrough_return(then_body, goto_targets);
                changed |= fold_if_return_fallthrough_return(else_body, goto_targets);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_if_return_fallthrough_return(&mut case.body, goto_targets);
                }
                changed |= fold_if_return_fallthrough_return(default, goto_targets);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        if !else_body.is_empty() && !body_is_effectively_empty(else_body) {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(cond) {
            i += 1;
            continue;
        }
        let Some(then_ret) = single_return_expr(then_body) else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(then_ret) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut skips_goto_target = false;
        while j < stmts.len() {
            let HirStmt::Label(label) = &stmts[j] else {
                break;
            };
            skips_goto_target |= goto_targets.contains(label);
            j += 1;
        }
        // The fold drops these labels; a `goto` elsewhere in the function that
        // still targets one of them would be left dangling.
        if skips_goto_target {
            i += 1;
            continue;
        }
        let Some(HirStmt::Return(Some(else_ret))) = stmts.get(j) else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(else_ret) {
            i += 1;
            continue;
        }
        let ty = expr_result_type(then_ret);
        let select = HirExpr::Select {
            cond: Box::new(cond.clone()),
            then_expr: Box::new(then_ret.clone()),
            else_expr: Box::new(else_ret.clone()),
            ty,
        };
        // Drop labels between if and fallthrough return, then replace both.
        stmts.drain(i..=j);
        stmts.insert(i, HirStmt::Return(Some(select)));
        changed = true;
        // Stay at i to allow chained folds on the new return.
        i += 1;
    }
    changed
}

/// `x = seed; if (c) { x = a; }` → `x = c ? a : seed` (presentation-pure, empty else).
/// Cond/then may mention `x`; they are rewritten with `seed` substituted.
pub(super) fn fold_seed_if_overwrite_assign(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_seed_if_overwrite_assign(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_seed_if_overwrite_assign(then_body);
                changed |= fold_seed_if_overwrite_assign(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_seed_if_overwrite_assign(&mut case.body);
                }
                changed |= fold_seed_if_overwrite_assign(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let HirStmt::Assign {
            lhs: HirLValue::Var(seed_name),
            rhs: seed_rhs,
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(seed_rhs) || expr_mentions_var(seed_rhs, seed_name) {
            i += 1;
            continue;
        }
        let seed_name = seed_name.clone();
        let seed_rhs = seed_rhs.clone();

        let j = i + 1;
        let Some(HirStmt::If {
            cond,
            then_body,
            else_body,
        }) = stmts.get(j)
        else {
            i += 1;
            continue;
        };
        if !else_body.is_empty() && !body_is_effectively_empty(else_body) {
            i += 1;
            continue;
        }
        let Some((then_name, then_rhs)) = single_var_assign(then_body) else {
            i += 1;
            continue;
        };
        if then_name != seed_name {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(cond) || !expr_is_presentation_pure(then_rhs) {
            i += 1;
            continue;
        }

        let mut cond = cond.clone();
        let mut then_rhs = then_rhs.clone();
        replace_var_in_expr(&mut cond, &seed_name, &seed_rhs);
        replace_var_in_expr(&mut then_rhs, &seed_name, &seed_rhs);
        // After substitution, reject residual self-reference (shouldn't happen for pure seeds).
        if expr_mentions_var(&cond, &seed_name)
            || expr_mentions_var(&then_rhs, &seed_name)
            || expr_mentions_var(&seed_rhs, &seed_name)
        {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(&cond) || !expr_is_presentation_pure(&then_rhs) {
            i += 1;
            continue;
        }

        let ty = expr_result_type(&then_rhs);
        let select = HirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(then_rhs),
            else_expr: Box::new(seed_rhs),
            ty,
        };
        stmts.drain(i..=j);
        stmts.insert(
            i,
            HirStmt::Assign {
                lhs: HirLValue::Var(seed_name),
                rhs: select,
            },
        );
        changed = true;
        i += 1;
    }
    changed
}

/// Drop empty `else {}` arms (including nested).
pub(super) fn strip_empty_else_arms(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= strip_empty_else_arms(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= strip_empty_else_arms(then_body);
                changed |= strip_empty_else_arms(else_body);
                if !else_body.is_empty() && body_is_effectively_empty(else_body) {
                    else_body.clear();
                    changed = true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= strip_empty_else_arms(&mut case.body);
                }
                changed |= strip_empty_else_arms(default);
            }
            _ => {}
        }
    }
    changed
}

/// `if (c) {} else { body }` → `if (!c) { body }` (labels-only then counts as empty).
pub(super) fn fold_empty_then_invert_else(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_empty_then_invert_else(b);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= fold_empty_then_invert_else(then_body);
                changed |= fold_empty_then_invert_else(else_body);
                if body_is_effectively_empty(then_body)
                    && !else_body.is_empty()
                    && !body_is_effectively_empty(else_body)
                {
                    *cond = goto_recovery::invert_cond(std::mem::replace(
                        cond,
                        HirExpr::Const(0, NirType::Bool),
                    ));
                    std::mem::swap(then_body, else_body);
                    else_body.clear();
                    changed = true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_empty_then_invert_else(&mut case.body);
                }
                changed |= fold_empty_then_invert_else(default);
            }
            _ => {}
        }
    }
    changed
}
