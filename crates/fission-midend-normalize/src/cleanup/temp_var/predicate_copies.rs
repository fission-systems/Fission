use super::super::utils::{count_var_uses_in_stmt, is_trivial_temp_name, replace_var_in_stmt};
use super::inline_safety::{
    count_uses_until_redef, expr_is_pure_copy_rhs, if_arm_use_sees_changed_inputs,
};
use crate::HashMap;
use crate::analysis::defuse::DefUseMap;
use crate::prelude::*;

/// Adjacent pure copy into the next if: `t = a; if (… t …)` → `if (… a …)`
/// when `t` is a trivial temp, `a` is pure, and every use of `t` is on that if
/// (whole-function and sequential budgets match).
///
/// Measured on power exit tests after folding `t == 0 || t < 0` → `t <= 0`.
pub fn collapse_adjacent_pure_copy_into_if(stmts: &mut Vec<PreHirStmt>) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    collapse_adjacent_pure_copy_into_if_with_counts(stmts, &use_counts)
}

pub(super) fn collapse_adjacent_pure_copy_into_if_with_counts(
    stmts: &mut Vec<PreHirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = collapse_adjacent_pure_copy_into_if_linear(stmts, use_counts);
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            use_counts,
                        );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            use_counts,
                        );
                    }
                }
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    use_counts,
                );
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    use_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        use_counts,
                    );
                }
                changed |= collapse_adjacent_pure_copy_into_if_with_counts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    use_counts,
                );
            }
            _ => {}
        }
    }
    changed
}

fn collapse_adjacent_pure_copy_into_if_linear(
    stmts: &mut Vec<PreHirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    if stmts.len() < 2 {
        return false;
    }
    let mut changed = false;
    let mut i = 0usize;
    while i + 1 < stmts.len() {
        let (t_name, rhs) = match &stmts[i] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(t),
                rhs,
            } if is_trivial_temp_name(t) && expr_is_pure_copy_rhs(rhs) => (t.clone(), rhs.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        let PreHirStmt::If { .. } = &stmts[i + 1] else {
            i += 1;
            continue;
        };
        let if_uses = count_var_uses_in_stmt(&stmts[i + 1], &t_name);
        if if_uses == 0 {
            i += 1;
            continue;
        }
        let sequential = count_uses_until_redef(stmts, i, &t_name);
        let total = use_counts.get(t_name.as_str()).copied().unwrap_or(0);
        // All uses of t are on this if (no post-loop / multi-def residual uses).
        if sequential != if_uses || total != sequential {
            i += 1;
            continue;
        }
        // Free vars of rhs must not be redefined between def and if (adjacent:
        // none), nor in an arm ahead of a use there.
        if if_arm_use_sees_changed_inputs(&stmts[i + 1], &t_name, &stmts[i]) {
            i += 1;
            continue;
        }
        replace_var_in_stmt(&mut stmts[i + 1], &t_name, &rhs);
        stmts.remove(i);
        changed = true;
        // Do not advance i — new stmt at i may also match.
    }
    changed
}
