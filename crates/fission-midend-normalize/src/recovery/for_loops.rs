use crate::HashSet;
use crate::prelude::*;

pub fn apply_for_loop_folding(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;

    // Apply to children first
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= apply_for_loop_folding(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    apply_for_loop_folding(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
                changed |=
                    apply_for_loop_folding(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= apply_for_loop_folding(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                        &mut case.body,
                    ));
                }
                changed |=
                    apply_for_loop_folding(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
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

            new_stmts.push(PreHirStmt::For {
                init: inits.pop(),
                cond: Some(cond),
                update: update.map(Box::new),
                body: body.into(),
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
    stmts: &[PreHirStmt],
    idx: usize,
) -> Option<(
    Vec<Box<PreHirStmt>>,
    PreHirExpr,
    Option<PreHirStmt>,
    Vec<PreHirStmt>,
    usize,
)> {
    let stmt = &stmts[idx];

    let (cond, mut body) = match stmt {
        PreHirStmt::While { cond, body } => (cond.clone(), (**body).clone()),
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

pub(super) fn stmt_list_contains_continue_pub(stmts: &[PreHirStmt]) -> bool {
    stmt_list_contains_continue(stmts)
}

fn stmt_list_contains_continue(stmts: &[PreHirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Continue => return true,
            PreHirStmt::Block(b) => {
                if stmt_list_contains_continue(b) {
                    return true;
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if stmt_list_contains_continue(then_body) || stmt_list_contains_continue(else_body)
                {
                    return true;
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
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
            PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {}
            _ => {}
        }
    }
    false
}

fn collect_cond_vars(expr: &PreHirExpr, vars: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            vars.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => collect_cond_vars(expr, vars),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_cond_vars(lhs, vars);
            collect_cond_vars(rhs, vars);
        }
        PreHirExpr::PtrOffset { base, .. } => collect_cond_vars(base, vars),
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_cond_vars(arg, vars);
            }
        }
        _ => {}
    }
}

fn is_var_modification(stmt: &PreHirStmt, vars: &HashSet<String>) -> bool {
    let PreHirStmt::Assign { lhs, .. } = stmt else {
        return false;
    };
    let PreHirLValue::Var(name) = lhs else {
        return false;
    };
    vars.contains(name)
}
