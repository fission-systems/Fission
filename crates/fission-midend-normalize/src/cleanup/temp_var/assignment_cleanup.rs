use super::super::utils::{
    expr_has_side_effects, expr_mentions_var, is_dead_local_clobber_name, is_trivial_temp_name,
    retain_unmarked_stmts,
};
use crate::analysis::defuse::DefUseMap;
use crate::prelude::*;
use crate::{HashMap, HashSet};

pub fn eliminate_dead_temp_assigns(
    stmts: &mut Vec<PreHirStmt>,
    _preserved_temps: &HashSet<&str>,
) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    eliminate_dead_temp_assigns_recursive(stmts, &use_counts)
}

fn eliminate_dead_temp_assigns_recursive(
    stmts: &mut Vec<PreHirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for (idx, stmt) in stmts.iter().enumerate() {
        let (name, rhs) = match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if is_trivial_temp_name(name) => (name, rhs),
            _ => continue,
        };

        let uses = use_counts.get(name.as_str()).copied().unwrap_or(0);
        let side_effects = expr_has_side_effects(rhs);
        if uses == 0 && !side_effects {
            to_remove[idx] = true;
            changed = true;
        }
    }

    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= eliminate_dead_temp_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_counts,
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |= eliminate_dead_temp_assigns_recursive(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            use_counts,
                        );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |= eliminate_dead_temp_assigns_recursive(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            use_counts,
                        );
                    }
                }
                changed |= eliminate_dead_temp_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_counts,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_dead_temp_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    use_counts,
                );
                changed |= eliminate_dead_temp_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    use_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_dead_temp_assigns_recursive(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        use_counts,
                    );
                }
                changed |= eliminate_dead_temp_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    use_counts,
                );
            }
            _ => {}
        }
    }

    changed
}

pub fn eliminate_redundant_var_assigns(stmts: &mut Vec<PreHirStmt>) -> bool {
    eliminate_redundant_var_assigns_recursive(stmts)
}

/// Drop pure `x = x` and adjacent duplicate assigns. Must recurse into nested
/// Block/If/loop bodies — structured O0 functions wrap the real body in a
/// single outer Block, so a top-level-only scan never sees the noise.
fn eliminate_redundant_var_assigns_recursive(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for idx in 0..stmts.len() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = &stmts[idx]
        else {
            continue;
        };

        if matches!(rhs, PreHirExpr::Var(rhs_name) if rhs_name == name) {
            to_remove[idx] = true;
            changed = true;
            continue;
        }

        if idx == 0
            || to_remove[idx - 1]
            || expr_has_side_effects(rhs)
            || expr_mentions_var(rhs, name)
        {
            continue;
        }

        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(prev_name),
            rhs: prev_rhs,
        } = &stmts[idx - 1]
        else {
            continue;
        };

        if prev_name == name && redundant_assign_rhs_equal(prev_rhs, rhs) {
            to_remove[idx - 1] = true;
            changed = true;
        }
    }

    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= eliminate_redundant_var_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |=
                            eliminate_redundant_var_assigns_recursive(
                                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |=
                            eliminate_redundant_var_assigns_recursive(
                                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                            );
                    }
                }
                changed |= eliminate_redundant_var_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_redundant_var_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= eliminate_redundant_var_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        eliminate_redundant_var_assigns_recursive(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        );
                }
                changed |= eliminate_redundant_var_assigns_recursive(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }

    changed
}

fn redundant_assign_rhs_equal(lhs: &PreHirExpr, rhs: &PreHirExpr) -> bool {
    lhs == rhs
        || matches!(
            (lhs, rhs),
            (PreHirExpr::Const(lhs_value, _), PreHirExpr::Const(rhs_value, _)) if lhs_value == rhs_value
        )
}

pub fn eliminate_dead_local_clobber_assigns(func: &mut PreHirFunction) -> bool {
    // Build a whole-function use map so sibling branches / nested blocks are
    // correctly accounted for.  Using a scoped `count_uses_in_stmt_list` on
    // each nested slice risks counting only the local slice and incorrectly
    // classifying a variable as dead when it is live in a sibling scope.
    let use_map = DefUseMap::build(&func.body);
    let local_types: HashMap<&str, &NirType> = func
        .locals
        .iter()
        .map(|b| (b.name.as_str(), &b.ty))
        .collect();
    let param_names: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    // Stack-backed locals (StackOffset / DerivedFromStackOffset origin) must
    // NEVER be silently removed even when their name is never read, because the
    // write itself may be observable through aliased pointers.
    let stack_backed_names: HashSet<&str> = func
        .locals
        .iter()
        .filter(|b| {
            matches!(
                b.origin,
                Some(NirBindingOrigin::StackOffset(_))
                    | Some(NirBindingOrigin::DerivedFromStackOffset(_))
            )
        })
        .map(|b| b.name.as_str())
        .collect();
    eliminate_dead_local_clobber_assigns_in_stmts(
        &mut func.body,
        &param_names,
        &local_types,
        &stack_backed_names,
        &use_map,
    )
}

fn eliminate_dead_local_clobber_assigns_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    param_names: &HashSet<&str>,
    local_types: &HashMap<&str, &NirType>,
    stack_backed_names: &HashSet<&str>,
    use_map: &DefUseMap,
) -> bool {
    // Recurse into nested bodies first (the use_map is already whole-function).
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
                eliminate_dead_local_clobber_assigns_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    eliminate_dead_local_clobber_assigns_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        param_names,
                        local_types,
                        stack_backed_names,
                        use_map,
                    );
                }
                eliminate_dead_local_clobber_assigns_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            _ => {}
        }
    }

    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    for (idx, stmt) in stmts.iter().enumerate() {
        let (name, rhs) = match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } => (name.as_str(), rhs),
            _ => continue,
        };
        if !is_dead_local_clobber_name(name)
            || param_names.contains(name)
            || name.starts_with("slot_")
            || expr_has_side_effects(rhs)
        {
            continue;
        }
        // Stack-backed locals (StackOffset / DerivedFromStackOffset) must never
        // be removed even when unused: their writes may be observable through
        // aliased pointers.  This is the authoritative semantic guard that
        // replaces the old hex-offset cut-off.
        if stack_backed_names.contains(name) {
            continue;
        }
        if matches!(
            local_types.get(name).copied(),
            Some(NirType::Aggregate { .. } | NirType::Ptr(_))
        ) {
            continue;
        }
        // Use the whole-function use map — not a local slice — so sibling
        // branches that read this name are correctly counted.
        let uses = use_map.use_count.get(name).copied().unwrap_or(0);
        if uses == 0 {
            to_remove[idx] = true;
            changed = true;
        }
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }
    changed
}
