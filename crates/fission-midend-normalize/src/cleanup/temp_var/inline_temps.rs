use super::super::utils::{
    count_var_uses_in_stmt, is_trivial_temp_name, replace_var_in_stmt, retain_unmarked_stmts,
};
use super::inline_safety::{
    count_uses_until_redef, expr_is_low_cost_inline_candidate, expr_is_pure_copy_rhs,
    expr_prefers_stable_materialization, find_inline_forward_target,
    stmt_uses_var_in_predicate_position,
};
use super::predicate_copies::collapse_adjacent_pure_copy_into_if_with_counts;
use crate::analysis::defuse::DefUseMap;
use crate::analysis::preservation::should_skip_inline_for_preserved_temp;
use crate::prelude::*;
use crate::{HashMap, HashSet};

pub fn inline_single_use_temps(
    stmts: &mut Vec<PreHirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    // Whole-function use counts: nested scopes must see post-loop uses so
    // loop-carried temps (def in loop, use after) are not falsely single-use.
    let use_counts = DefUseMap::build(stmts).use_count;
    inline_single_use_temps_recursive(stmts, preserved_temps, &use_counts)
}

fn inline_single_use_temps_recursive(
    stmts: &mut Vec<PreHirStmt>,
    preserved_temps: &HashSet<&str>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let (name, rhs) = match &stmts[idx] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if is_trivial_temp_name(name) => (name.clone(), rhs.clone()),
            _ => {
                idx += 1;
                continue;
            }
        };
        if should_skip_inline_for_preserved_temp(&name, preserved_temps) {
            idx += 1;
            continue;
        }

        let prefers_stable_materialization = expr_prefers_stable_materialization(&rhs);
        let Some(target_idx) =
            find_inline_forward_target(stmts, idx, &name, prefers_stable_materialization)
        else {
            idx += 1;
            continue;
        };
        let target_uses = count_var_uses_in_stmt(&stmts[target_idx], &name);
        let total_uses = use_counts.get(name.as_str()).copied().unwrap_or(0);
        // Pure Var/Const/Cast-of-Var copies: allow when all whole-function uses
        // are covered by the sequential live range up to the forward target.
        // This inlines `t = a; if (t <= 0)` inside loops without treating
        // multi-def or post-loop uses as single-use (loop-carried / CDQ-safe).
        let pure_copy = expr_is_pure_copy_rhs(&rhs);
        let sequential_uses = if pure_copy {
            count_uses_until_redef(stmts, idx, &name)
        } else {
            0
        };
        let use_budget_ok = if pure_copy && sequential_uses > 0 {
            sequential_uses == target_uses && total_uses == sequential_uses
        } else {
            total_uses == target_uses
        };
        if !use_budget_ok {
            idx += 1;
            continue;
        }
        let predicate_sensitive = stmt_uses_var_in_predicate_position(&stmts[target_idx], &name);
        let low_cost_inline = expr_is_low_cost_inline_candidate(&rhs);
        if target_uses > 1 && prefers_stable_materialization {
            idx += 1;
            continue;
        }
        if predicate_sensitive && !low_cost_inline {
            idx += 1;
            continue;
        }
        if target_uses > 1 && !low_cost_inline {
            idx += 1;
            continue;
        }
        replace_var_in_stmt(&mut stmts[target_idx], &name, &rhs);
        to_remove[idx] = true;
        changed = true;
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= inline_single_use_temps_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    preserved_temps,
                    use_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |= inline_single_use_temps_recursive(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            preserved_temps,
                            use_counts,
                        );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |= inline_single_use_temps_recursive(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            preserved_temps,
                            use_counts,
                        );
                    }
                }
                changed |= inline_single_use_temps_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    preserved_temps,
                    use_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_single_use_temps_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    preserved_temps,
                    use_counts,
                );
                changed |= inline_single_use_temps_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    preserved_temps,
                    use_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_single_use_temps_recursive(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        preserved_temps,
                        use_counts,
                    );
                }
                changed |= inline_single_use_temps_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    preserved_temps,
                    use_counts,
                );
            }
            _ => {}
        }
    }

    changed
}
