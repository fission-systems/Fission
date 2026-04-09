use super::super::*;
use std::collections::HashSet;

pub(crate) fn apply_for_loop_folding(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;

    // Apply to children first
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= apply_for_loop_folding(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_for_loop_folding(then_body);
                changed |= apply_for_loop_folding(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= apply_for_loop_folding(&mut case.body);
                }
                changed |= apply_for_loop_folding(default);
            }
            _ => {}
        }
    }

    if stmts.is_empty() {
        return changed;
    }

    let mut new_stmts = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        if let Some((mut inits, cond, update, body, consumed_init_count)) =
            try_collapse_while_to_for_algorithmic(stmts, i)
        {
            for _ in 0..consumed_init_count {
                if !new_stmts.is_empty() {
                    new_stmts.pop();
                }
            }

            new_stmts.push(HirStmt::For {
                init: inits.pop(),
                cond: Some(cond),
                update: update.map(Box::new),
                body,
            });
            changed = true;
        } else {
            new_stmts.push(stmts[i].clone());
        }
        i += 1;
    }
    *stmts = new_stmts;
    changed
}

fn try_collapse_while_to_for_algorithmic(
    stmts: &[HirStmt],
    idx: usize,
) -> Option<(
    Vec<Box<HirStmt>>,
    HirExpr,
    Option<HirStmt>,
    Vec<HirStmt>,
    usize,
)> {
    let stmt = &stmts[idx];

    let (cond, mut body) = match stmt {
        HirStmt::While { cond, body } => (cond.clone(), body.clone()),
        _ => return None,
    };

    let mut vars = HashSet::new();
    collect_cond_vars(&cond, &mut vars);
    if vars.is_empty() {
        return None;
    }

    // ALGORITHMIC SAFETY CHECK:
    // In Fission AST, a `Continue` statement explicitly bypasses the rest of the loop
    // and jumps to the condition block. If we extract an `update` statement into the For loop update field,
    // the semantic effect of `Continue` changes to jump to the `update` block instead!
    // Thus, an algorithmic check MUST ensure no inner `Continue` statements break the backward dominance of update.
    if stmt_list_contains_continue(&body) {
        return None;
    }

    let mut update = None;
    if let Some(last) = body.last() {
        if is_var_modification(last, &vars) {
            update = Some(body.pop().unwrap());
        }
    }

    // ALGORITHMIC UPWARD CODE MOTION CHECK:
    // Scan backwards strictly enforcing independence (dataflow preservation).
    let mut inits = Vec::new();
    let mut consumed_idx = 0;

    let mut scan_idx = idx;
    while scan_idx > 0 {
        scan_idx -= 1;
        let prev_stmt = &stmts[scan_idx];

        if is_var_modification(prev_stmt, &vars) {
            inits.push(Box::new(prev_stmt.clone()));
            consumed_idx += 1;
            break;
        } else {
            // Block upward reach if the statement contains side effects, control flow, or reads our vars.
            // Since we do not have a robust dependency checker here, breaking early is the only sound algorithmic choice.
            break;
        }
    }

    if inits.is_empty() && update.is_none() {
        return None;
    }

    Some((inits, cond, update, body, consumed_idx))
}

pub(super) fn stmt_list_contains_continue_pub(stmts: &[HirStmt]) -> bool {
    stmt_list_contains_continue(stmts)
}

fn stmt_list_contains_continue(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Continue => return true,
            HirStmt::Block(b) => {
                if stmt_list_contains_continue(b) {
                    return true;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if stmt_list_contains_continue(then_body) || stmt_list_contains_continue(else_body)
                {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    if stmt_list_contains_continue(&c.body) {
                        return true;
                    }
                }
                if stmt_list_contains_continue(default) {
                    return true;
                }
            }
            // `Continue` in a nested loop refers to the nested loop, not the outer one.
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {}
            _ => {}
        }
    }
    false
}

fn collect_cond_vars(expr: &HirExpr, vars: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => collect_cond_vars(expr, vars),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_cond_vars(lhs, vars);
            collect_cond_vars(rhs, vars);
        }
        HirExpr::PtrOffset { base, .. } => collect_cond_vars(base, vars),
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_cond_vars(arg, vars);
            }
        }
        _ => {}
    }
}

fn is_var_modification(stmt: &HirStmt, vars: &HashSet<String>) -> bool {
    let HirStmt::Assign { lhs, .. } = stmt else {
        return false;
    };
    let HirLValue::Var(name) = lhs else {
        return false;
    };
    vars.contains(name)
}
