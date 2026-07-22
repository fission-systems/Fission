use crate::prelude::*;
use crate::HashSet;

pub fn apply_for_loop_folding(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;

    // Apply to children first
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= apply_for_loop_folding(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_for_loop_folding(then_body);
                changed |= apply_for_loop_folding(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
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

            new_stmts.push(DirStmt::For {
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
    stmts: &[DirStmt],
    idx: usize,
) -> Option<(
    Vec<Box<DirStmt>>,
    DirExpr,
    Option<DirStmt>,
    Vec<DirStmt>,
    usize,
)> {
    let stmt = &stmts[idx];

    let (cond, mut body) = match stmt {
        DirStmt::While { cond, body } => (cond.clone(), body.clone()),
        _ => return None,
    };

    let mut vars = HashSet::default();
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

pub(super) fn stmt_list_contains_continue_pub(stmts: &[DirStmt]) -> bool {
    stmt_list_contains_continue(stmts)
}

fn stmt_list_contains_continue(stmts: &[DirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            DirStmt::Continue => return true,
            DirStmt::Block(b) => {
                if stmt_list_contains_continue(b) {
                    return true;
                }
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if stmt_list_contains_continue(then_body) || stmt_list_contains_continue(else_body)
                {
                    return true;
                }
            }
            DirStmt::Switch { cases, default, .. } => {
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
            DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. } => {}
            _ => {}
        }
    }
    false
}

fn collect_cond_vars(expr: &DirExpr, vars: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            vars.insert(name.clone());
        }
        DirExpr::Cast { expr, .. }
        | DirExpr::Unary { expr, .. }
        | DirExpr::Load { ptr: expr, .. }
        | DirExpr::AggregateCopy { src: expr, .. } => collect_cond_vars(expr, vars),
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_cond_vars(lhs, vars);
            collect_cond_vars(rhs, vars);
        }
        DirExpr::PtrOffset { base, .. } => collect_cond_vars(base, vars),
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_cond_vars(arg, vars);
            }
        }
        _ => {}
    }
}

fn is_var_modification(stmt: &DirStmt, vars: &HashSet<String>) -> bool {
    let DirStmt::Assign { lhs, .. } = stmt else {
        return false;
    };
    let DirLValue::Var(name) = lhs else {
        return false;
    };
    vars.contains(name)
}
