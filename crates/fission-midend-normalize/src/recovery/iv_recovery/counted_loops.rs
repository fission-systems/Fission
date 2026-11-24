use super::super::for_loops::stmt_list_contains_continue_pub;
use super::loop_expressions::invert_condition;
use super::merged_induction::try_split_merged_dual_iv_tail;
use super::pointer_cursor_loops::try_guarded_dowhile_pointer_iv_upgrade;
use super::recurrences::{
    find_init_before, find_loop_variable_dataflow, is_iv_update_dataflow, loop_variant_vars,
};
use super::row_stride_fill::try_recover_row_stride_fill_inner_loop;
use super::variable_uses::count_var_uses_in_stmt;
use crate::prelude::*;

/// Try to upgrade a `While` loop at `stmts[loop_idx]` to a `For` loop using
/// SCEV-lite IV detection.  Returns `true` if a transformation was applied.
fn try_scev_upgrade(stmts: &mut Vec<PreHirStmt>, loop_idx: usize) -> bool {
    let (is_for, init, cond, body) = match &stmts[loop_idx] {
        PreHirStmt::While { cond, body } => (false, None, cond.clone(), body.clone()),
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } if update.is_none() => (
            true,
            init.clone(),
            cond.as_ref().cloned().unwrap(),
            body.clone(),
        ),
        _ => return false,
    };

    // Safety: no Continue in body (semantics of `update` would change).
    if stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let loop_variant = loop_variant_vars(&body);

    let (var, update_idx) =
        match find_loop_variable_dataflow(stmts, loop_idx, &body, &cond, &loop_variant) {
            Some(res) => res,
            None => return false,
        };

    // Update must be the last statement in body (or we'd change semantics).
    let is_last = update_idx == body.len() - 1;
    if !is_last {
        let has_subsequent_uses = body[update_idx + 1..]
            .iter()
            .any(|stmt| count_var_uses_in_stmt(stmt, &var) > 0);
        if has_subsequent_uses {
            return false;
        }
    }

    if is_for {
        let mut new_body = body.clone();
        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut new_body).remove(update_idx);
        let update_stmt = body[update_idx].clone();

        stmts[loop_idx] = PreHirStmt::For {
            init,
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: new_body,
        };
        return true;
    } else {
        let init_idx = match find_init_before(stmts, loop_idx, &var) {
            Some(i) => i,
            None => return false,
        };

        // Apply transformation.
        let init_stmt = stmts[init_idx].clone();
        let mut new_body = body.clone();
        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut new_body).remove(update_idx);
        let update_stmt = body[update_idx].clone();

        // Remove the init statement *before* the loop.
        stmts.remove(init_idx);
        // loop_idx shifts down by 1.
        let loop_idx = loop_idx - 1;

        stmts[loop_idx] = PreHirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: new_body,
        };
        return true;
    }
}

/// Replace `goto label` with `continue` within `stmts`, without recursing into
/// nested loops (where `continue` would bind to the inner loop, not the outer).
fn replace_gotos_with_continue_shallow(stmts: &mut Vec<PreHirStmt>, label: &str) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Goto(target) if target == label => {
                *stmt = PreHirStmt::Continue;
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                replace_gotos_with_continue_shallow(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    label,
                );
                replace_gotos_with_continue_shallow(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    label,
                );
            }
            PreHirStmt::Block(body) => replace_gotos_with_continue_shallow(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                label,
            ),
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    replace_gotos_with_continue_shallow(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        label,
                    );
                }
                replace_gotos_with_continue_shallow(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    label,
                );
            }
            // Do NOT recurse into nested loops: continue binds to the inner loop there.
            _ => {}
        }
    }
}

/// Try to upgrade a `for (;;)` loop at `stmts[loop_idx]` to a proper counted
/// `for` loop when the body ends with the tail pattern:
///
/// ```text
/// Label(L)              ← update-section label (forward goto target)
/// iv_var = iv_var ± k   ← IV increment
/// if (break_cond) break ← exit check
/// ```
///
/// The body may also contain `goto L` (acting as `continue`), which are
/// replaced with `continue` statements after the transformation.
/// An init assignment `iv_var = <val>` must immediately precede the loop.
pub(super) fn try_upgrade_infinite_for_with_tail_update(
    stmts: &mut Vec<PreHirStmt>,
    loop_idx: usize,
) -> bool {
    let body = match &stmts[loop_idx] {
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        } => body.clone(),
        _ => return false,
    };

    let n = body.len();
    if n < 3 {
        return false;
    }

    // body[n-3]: Label(L)  body[n-2]: iv_update  body[n-1]: if(break_cond) break;
    let update_label = match &body[n - 3] {
        PreHirStmt::Label(l) => l.clone(),
        _ => return false,
    };

    let iv_update = body[n - 2].clone();
    let iv_name = match &iv_update {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => name.clone(),
        _ => return false,
    };

    let break_cond = match &body[n - 1] {
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } if else_body.is_empty() && matches!(then_body.as_slice(), [PreHirStmt::Break]) => {
            cond.clone()
        }
        _ => return false,
    };

    // Safety: no explicit `continue` in the body (hoisting the update would change semantics).
    if stmt_list_contains_continue_pub(&body) {
        return false;
    }

    // Validate that iv_update is a linear recurrence on iv_name.
    let loop_variant = loop_variant_vars(&body);
    let update_ok = match &iv_update {
        PreHirStmt::Assign { rhs, .. } => {
            is_iv_update_dataflow(rhs, &iv_name, &loop_variant, &body, 0)
        }
        _ => false,
    };
    if !update_ok {
        return false;
    }

    // Find the init assignment immediately before the loop.
    let init_idx = match find_init_before(stmts, loop_idx, &iv_name) {
        Some(i) => i,
        None => return false,
    };

    // Build the new body: strip the tail 3 statements and replace goto→continue.
    let mut new_body = body[..n - 3].to_vec();
    replace_gotos_with_continue_shallow(&mut new_body, &update_label);

    // Negate the break condition to get the loop-continuation condition.
    let loop_cond = invert_condition(break_cond);

    // Apply transformation: lift init out of stmts, adjust loop_idx, rebuild For.
    let init_stmt = stmts[init_idx].clone();
    stmts.remove(init_idx);
    let loop_idx = loop_idx - 1; // shifted after init removal

    stmts[loop_idx] = PreHirStmt::For {
        init: Some(Box::new(init_stmt)),
        cond: Some(loop_cond),
        update: Some(Box::new(iv_update)),
        body: new_body.into(),
    };
    true
}

pub(super) fn apply_scev_upgrade_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    locals: &mut Vec<PreHirBinding>,
    active_guards: &mut Vec<PreHirExpr>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], PreHirStmt::While { .. }) {
            if try_scev_upgrade(stmts, i) {
                changed = true;
                continue;
            }
        } else if matches!(&stmts[i], PreHirStmt::DoWhile { .. })
            && try_guarded_dowhile_pointer_iv_upgrade(stmts, locals, i, active_guards)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            PreHirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_recover_row_stride_fill_inner_loop(stmts, i, locals)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            PreHirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_split_merged_dual_iv_tail(stmts, i, locals)
        {
            changed = true;
            continue;
        } else if matches!(
            &stmts[i],
            PreHirStmt::For {
                cond: None,
                update: None,
                ..
            }
        ) && try_upgrade_infinite_for_with_tail_update(stmts, i)
        {
            changed = true;
            continue;
        }
        match &mut stmts[i] {
            PreHirStmt::If {
                then_body,
                else_body,
                cond,
            } => {
                active_guards.push(cond.clone());
                changed |= apply_scev_upgrade_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    locals,
                    active_guards,
                );
                active_guards.pop();
                changed |= apply_scev_upgrade_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    locals,
                    active_guards,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= apply_scev_upgrade_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    locals,
                    active_guards,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_scev_upgrade_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        locals,
                        active_guards,
                    );
                }
                changed |= apply_scev_upgrade_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    locals,
                    active_guards,
                );
            }
            _ => {}
        }
        i += 1;
    }
    changed
}
