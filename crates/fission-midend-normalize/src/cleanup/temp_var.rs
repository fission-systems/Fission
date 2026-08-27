use super::super::analysis::defuse::DefUseMap;
use super::super::analysis::preservation::{
    should_block_trivial_return_collapse, should_keep_unused_temp_binding,
    should_skip_inline_for_preserved_temp,
};
use super::utils::*;
use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::wave_stats;
use fission_midend_prehir::util::{rename_vars_in_expr, rename_vars_in_stmts};

/// Reclaim a canonical `local_<stack-offset>` name after the binding that
/// originally occupied it has been removed by cleanup.
///
/// Builder name allocation must conservatively avoid every name it has seen,
/// so a real collision can produce `local_8_7`. At pipeline end the original
/// `local_8` may no longer exist, leaving a stale allocation suffix on the
/// surviving stack binding. Reclaiming the base here is a pure alpha rename:
/// it is admitted only for stack-derived bindings, only when the base is free,
/// and only when exactly one surviving binding claims that base.
pub fn canonicalize_orphaned_stack_slot_names(func: &mut PreHirFunction) -> bool {
    let occupied = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    let mut claimants: HashMap<String, Vec<String>> = HashMap::default();
    for binding in &func.locals {
        if !matches!(
            binding.origin,
            Some(NirBindingOrigin::StackOffset(_) | NirBindingOrigin::DerivedFromStackOffset(_))
        ) {
            continue;
        }
        let Some(base) = canonical_stack_slot_base(&binding.name) else {
            continue;
        };
        if occupied.contains(&base) {
            continue;
        }
        claimants
            .entry(base)
            .or_default()
            .push(binding.name.clone());
    }

    let mut renames = claimants
        .into_iter()
        .filter_map(|(base, names)| {
            (names.len() == 1).then(|| (names.into_iter().next().unwrap(), base))
        })
        .collect::<Vec<_>>();
    renames.sort();
    if renames.is_empty() {
        return false;
    }

    rename_vars_in_stmts(&mut func.body, &renames);
    for binding in func.params.iter_mut().chain(func.locals.iter_mut()) {
        if let Some(initializer) = &mut binding.initializer {
            rename_vars_in_expr(initializer, &renames);
        }
    }
    for binding in &mut func.locals {
        if let Some((_, replacement)) = renames.iter().find(|(name, _)| name == &binding.name) {
            binding.name = replacement.clone();
        }
    }
    true
}

fn canonical_stack_slot_base(name: &str) -> Option<String> {
    let suffix = name.strip_prefix("local_")?;
    let (offset, collision_id) = suffix.rsplit_once('_')?;
    if offset.is_empty()
        || collision_id.is_empty()
        || !offset.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !collision_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(format!("local_{offset}"))
}

pub fn collapse_trivial_assign_returns(
    stmts: &mut Vec<PreHirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    let mut changed = false;
    let mut blocked = 0usize;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let replacement = match (&stmts[idx], &stmts[idx + 1]) {
            (
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs,
                },
                PreHirStmt::Return(Some(PreHirExpr::Var(ret_name))),
            ) if name == ret_name => {
                // Collapse candidates:
                // - ABI return regs with pure RHS (`rax = param+5; return rax`)
                // - trivial temps (subject to preservation)
                // - any local with pure RHS, especially const (`w8 = 3; return w8`)
                let pure_rhs = is_pure_return_collapse_rhs(rhs);
                let is_temp = is_trivial_temp_name(name);
                let is_abi = is_abi_return_register_name(name);
                if is_abi {
                    if pure_rhs { Some(rhs.clone()) } else { None }
                } else if is_temp || pure_rhs {
                    if should_block_trivial_return_collapse(name, preserved_temps)
                        && !matches!(rhs, PreHirExpr::Const(_, _))
                    {
                        blocked += 1;
                        None
                    } else {
                        Some(rhs.clone())
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[idx + 1] = PreHirStmt::Return(Some(expr));
            to_remove[idx] = true;
            changed = true;
        }
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }

    // Recurse into nested structured regions so Block/if arms also fold.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    preserved_temps,
                ) {
                    changed = true;
                }
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    if collapse_trivial_assign_returns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        preserved_temps,
                    ) {
                        changed = true;
                    }
                }
                if collapse_trivial_assign_returns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    preserved_temps,
                ) {
                    changed = true;
                }
            }
            _ => {}
        }
    }

    wave_stats::add_preserved_temp_prune_blocked(blocked);
    changed
}

pub fn inline_single_use_temps(
    stmts: &mut Vec<PreHirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    // Whole-function use counts: nested scopes must see post-loop uses so
    // loop-carried temps (def in loop, use after) are not falsely single-use.
    let use_counts = DefUseMap::build(stmts).use_count;
    inline_single_use_temps_recursive(stmts, preserved_temps, &use_counts)
}

/// Adjacent pure copy into the next if: `t = a; if (… t …)` → `if (… a …)`
/// when `t` is a trivial temp, `a` is pure, and every use of `t` is on that if
/// (whole-function and sequential budgets match).
///
/// Measured on power exit tests after folding `t == 0 || t < 0` → `t <= 0`.
pub fn collapse_adjacent_pure_copy_into_if(stmts: &mut Vec<PreHirStmt>) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    collapse_adjacent_pure_copy_into_if_with_counts(stmts, &use_counts)
}

fn collapse_adjacent_pure_copy_into_if_with_counts(
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
        // Free vars of rhs must not be redefined between def and if (adjacent: none).
        replace_var_in_stmt(&mut stmts[i + 1], &t_name, &rhs);
        stmts.remove(i);
        changed = true;
        // Do not advance i — new stmt at i may also match.
    }
    changed
}

/// Collapse pure temp self-square chains:
/// `t = a; t = t * t; a = t`  →  `a = a * a`
/// when `t` is a trivial temp and has no other live uses in between.
///
/// Measured on x86 `imul` squaring for power-class loops (`base *= base`).
pub fn collapse_temp_self_square_assigns(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    changed |= collapse_temp_self_square_linear(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let PreHirStmt::Block(b) = &mut **i {
                        changed |= collapse_temp_self_square_assigns(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                        );
                    }
                }
                if let Some(u) = update {
                    if let PreHirStmt::Block(b) = &mut **u {
                        changed |= collapse_temp_self_square_assigns(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(b),
                        );
                    }
                }
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_temp_self_square_assigns(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    );
                }
                changed |= collapse_temp_self_square_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }
    changed
}

fn collapse_temp_self_square_linear(stmts: &mut Vec<PreHirStmt>) -> bool {
    if stmts.len() < 3 {
        return false;
    }
    let mut changed = false;
    let mut i = 0usize;
    while i + 2 < stmts.len() {
        let Some((t_name, a_name, ty)) = match_temp_self_square_window(stmts, i) else {
            i += 1;
            continue;
        };
        // Refuse if `t` is used outside this three-stmt window in this linear list.
        let other_t_uses: usize = stmts
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx < i || *idx > i + 2)
            .map(|(_, s)| count_var_uses_in_stmt(s, &t_name))
            .sum();
        if other_t_uses > 0 {
            i += 1;
            continue;
        }
        stmts[i] = PreHirStmt::Assign {
            lhs: PreHirLValue::Var(a_name.clone()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs: Box::new(PreHirExpr::Var(a_name.clone())),
                rhs: Box::new(PreHirExpr::Var(a_name)),
                ty,
            },
        };
        stmts.remove(i + 2);
        stmts.remove(i + 1);
        changed = true;
    }
    changed
}

fn match_temp_self_square_window(
    stmts: &[PreHirStmt],
    i: usize,
) -> Option<(String, String, NirType)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(t1),
        rhs: PreHirExpr::Var(a1),
    } = &stmts[i]
    else {
        return None;
    };
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(t2),
        rhs:
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Mul,
                lhs,
                rhs,
                ty,
            },
    } = &stmts[i + 1]
    else {
        return None;
    };
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(a2),
        rhs: PreHirExpr::Var(t3),
    } = &stmts[i + 2]
    else {
        return None;
    };
    let (PreHirExpr::Var(tl), PreHirExpr::Var(tr)) = (lhs.as_ref(), rhs.as_ref()) else {
        return None;
    };
    if t1 != t2 || t1 != t3 || t1 != tl || t1 != tr || a1 != a2 {
        return None;
    }
    if !is_trivial_temp_name(t1) || t1 == a1 {
        return None;
    }
    Some((t1.clone(), a1.clone(), ty.clone()))
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

fn count_uses_until_redef(stmts: &[PreHirStmt], def_idx: usize, name: &str) -> usize {
    let mut total = 0usize;
    for stmt in stmts.iter().skip(def_idx + 1) {
        if stmt_redefines_temp(stmt, name) {
            break;
        }
        total = total.saturating_add(count_var_uses_in_stmt(stmt, name));
    }
    total
}

fn expr_is_pure_copy_rhs(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr, .. } => {
            matches!(expr.as_ref(), PreHirExpr::Var(_) | PreHirExpr::Const(_, _))
        }
        _ => false,
    }
}

fn find_inline_forward_target(
    stmts: &[PreHirStmt],
    def_idx: usize,
    name: &str,
    stable_materialization: bool,
) -> Option<usize> {
    let mut scan_idx = def_idx + 1;
    while scan_idx < stmts.len() {
        let stmt = &stmts[scan_idx];
        let uses = count_var_uses_in_stmt(stmt, name);
        let redefines = stmt_redefines_temp(stmt, name);
        if redefines {
            return None;
        }
        if uses > 0 && stmt_allows_inline_target(stmt) {
            return Some(scan_idx);
        }
        if uses == 0 {
            if stmt_redefines_expr_dependency(stmt, &stmts[def_idx]) {
                return None;
            }
            if stmt_blocks_linear_inline_scan(stmt) {
                return None;
            }
            if stable_materialization && stmt_blocks_stable_inline_scan(stmt) {
                return None;
            }
            scan_idx += 1;
            continue;
        }
        if !stmt_allows_forward_scan(stmt) {
            return None;
        }
        return None;
    }
    None
}

fn stmt_redefines_expr_dependency(stmt: &PreHirStmt, defining_stmt: &PreHirStmt) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(defined_name),
        ..
    } = stmt
    else {
        return false;
    };
    let PreHirStmt::Assign { rhs, .. } = defining_stmt else {
        return false;
    };
    expr_mentions_var(rhs, defined_name)
}

fn stmt_blocks_linear_inline_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, PreHirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) => expr_has_side_effects(expr),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Block(_)
        | PreHirStmt::Switch { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Break
        | PreHirStmt::Continue => true,
    }
}

fn stmt_blocks_stable_inline_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, PreHirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        PreHirStmt::Label(_) => false,
        PreHirStmt::Return(None)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Block(_)
        | PreHirStmt::Switch { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => true,
    }
}

fn stmt_allows_forward_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(_),
            rhs,
        } => !expr_has_side_effects(rhs),
        PreHirStmt::Return(Some(expr)) => !expr_has_side_effects(expr),
        PreHirStmt::If { cond, .. } => !expr_has_side_effects(cond),
        PreHirStmt::Expr(expr) => !expr_has_side_effects(expr),
        _ => false,
    }
}

fn stmt_allows_inline_target(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::If { .. }
    )
}

fn stmt_redefines_temp(stmt: &PreHirStmt, name: &str) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } if lhs_name == name
    )
}

fn stmt_uses_var_in_predicate_position(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::If { cond, .. } => expr_contains_var(cond, name),
        PreHirStmt::While { cond, .. } | PreHirStmt::DoWhile { cond, .. } => {
            expr_contains_var(cond, name)
        }
        PreHirStmt::For {
            init, cond, update, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_contains_var(expr, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
        }
        PreHirStmt::Switch { expr, .. } => expr_contains_var(expr, name),
        PreHirStmt::Block(stmts) => stmts
            .iter()
            .any(|inner| stmt_uses_var_in_predicate_position(inner, name)),
        _ => false,
    }
}

fn expr_is_low_cost_inline_candidate(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_is_low_cost_inline_candidate(expr)
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::And
                    | PreHirBinaryOp::Or
                    | PreHirBinaryOp::Xor
                    | PreHirBinaryOp::Add
                    | PreHirBinaryOp::Sub
                    | PreHirBinaryOp::Shl
                    | PreHirBinaryOp::Shr
                    | PreHirBinaryOp::Sar
                    | PreHirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

fn expr_prefers_stable_materialization(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. } => expr_prefers_stable_materialization(expr),
        PreHirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().any(expr_prefers_stable_materialization)
        }
        PreHirExpr::Unary { .. }
        | PreHirExpr::Load { .. }
        | PreHirExpr::PtrOffset { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::AggregateCopy { .. }
        | PreHirExpr::FieldAccess { .. }
        | PreHirExpr::Call { .. } => true,
        PreHirExpr::Binary { op, .. } => matches!(
            op,
            PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Div
                | PreHirBinaryOp::Mod
                | PreHirBinaryOp::And
                | PreHirBinaryOp::Or
                | PreHirBinaryOp::Xor
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar
                | PreHirBinaryOp::Eq
                | PreHirBinaryOp::Ne
                | PreHirBinaryOp::Lt
                | PreHirBinaryOp::Le
                | PreHirBinaryOp::SLt
                | PreHirBinaryOp::SLe
        ),
    }
}

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

/// Hoist pure `v = param_N` copies above their first use when a prior pass
/// left a use-before-def (observed after cmov/flag recovery on x86-32 clamp).
///
/// Only applies to top-level straight-line bodies: a single dominating
/// definition of `v` that is a param alias, with an earlier pure use.
pub fn hoist_param_alias_copies_before_first_use(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0usize;
    while i < stmts.len() {
        let (name, param) = match &stmts[i] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Var(param),
            } if param.starts_with("param_") => (name.clone(), param.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        // Only hoist when this is the sole top-level def of `name`.
        let def_count = stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(n),
                        ..
                    } if n == &name
                )
            })
            .count();
        if def_count != 1 {
            i += 1;
            continue;
        }
        if let Some(first_use) = first_top_level_use_index_of_var(stmts, &name) {
            if first_use < i {
                let stmt = stmts.remove(i);
                stmts.insert(first_use, stmt);
                changed = true;
                // Restart so chained hoists are ordered correctly.
                i = 0;
                continue;
            }
        }
        let _ = param;
        i += 1;
    }
    // Recurse into structured bodies.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                );
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        hoist_param_alias_copies_before_first_use(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        );
                }
                changed |= hoist_param_alias_copies_before_first_use(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                );
            }
            _ => {}
        }
    }
    changed
}

fn first_top_level_use_index_of_var(stmts: &[PreHirStmt], name: &str) -> Option<usize> {
    for (idx, stmt) in stmts.iter().enumerate() {
        if stmt_uses_var(stmt, name) {
            return Some(idx);
        }
    }
    None
}

fn stmt_uses_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var(lhs, name) || expr_mentions_var(rhs, name)
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::Return(Some(expr))
        | PreHirStmt::VaStart { va_list: expr, .. } => expr_mentions_var(expr, name),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_mentions_var(cond, name)
                || then_body.iter().any(|s| stmt_uses_var(s, name))
                || else_body.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => body.iter().any(|s| stmt_uses_var(s, name)),
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || cond.as_ref().is_some_and(|c| expr_mentions_var(c, name))
                || update.as_ref().is_some_and(|s| stmt_uses_var(s, name))
                || body.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_mentions_var(expr, name)
                || cases
                    .iter()
                    .any(|c| c.body.iter().any(|s| stmt_uses_var(s, name)))
                || default.iter().any(|s| stmt_uses_var(s, name))
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn lvalue_uses_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false, // definition, not a use
        PreHirLValue::Deref { ptr, .. } => expr_mentions_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_mentions_var(base, name),
    }
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

pub fn prune_unused_temp_bindings(func: &mut PreHirFunction) -> bool {
    // Built once for the whole function: this used to call
    // `count_uses_in_stmt_list` (a fresh full-body walk) per local binding,
    // making this O(locals * body size) on every one of the ~30 pipeline
    // stages that call this pass. A single whole-body use-count map makes
    // each binding's check O(1) instead.
    let use_map = DefUseMap::build(&func.body);
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = use_map
            .use_count
            .get(binding.name.as_str())
            .copied()
            .unwrap_or(0)
            > 0;
        let written = use_map
            .def_count
            .get(binding.name.as_str())
            .copied()
            .unwrap_or(0)
            > 0;
        let initializer_has_side_effects = binding
            .initializer
            .as_ref()
            .is_some_and(expr_has_side_effects);
        // A binding the body neither reads nor writes has no effect the
        // emitted C can express, so its declaration is noise. The rule below
        // cannot reach these: it turns on `use_count` alone -- reads only --
        // and so must leave write-only stack homes declared for the rescue
        // pass to find. A home that is written appears in `def_count`, so
        // this cannot collide with them.
        //
        // **Stack-derived bindings are excluded, and that exclusion is the
        // whole subtlety.** This pass runs at some thirty pipeline stages,
        // including ones before slot addresses have been rewritten into slot
        // names. A stack local referenced only as `Load { ptr: <slot addr> }`
        // appears nowhere by name and is still live; pruning it there deletes
        // the binding a later stage was going to rewrite into. Nothing in the
        // emitted text breaks -- the name was absent either way -- so a
        // corpus sweep cannot see it. `preview_type_hints_apply_stack_local_
        // type_to_surfaced_slot_alias` can, and did.
        //
        // What is left reachable: temporaries and scaffolding, which are
        // name-referenced by construction. Measured on the 250-function
        // sample-set, 224 of 6,774 declared locals appear nowhere at all;
        // 123 of those are stack slots this deliberately does not touch.
        let address_referable = binding.origin.is_some_and(|origin| {
            matches!(
                origin,
                NirBindingOrigin::StackOffset(_)
                    | NirBindingOrigin::HomeSlot(_)
                    | NirBindingOrigin::OutgoingArgSlot(_)
                    | NirBindingOrigin::DerivedFromStackOffset(_)
                    | NirBindingOrigin::VaRegion
            )
        });
        if !used && !written && !initializer_has_side_effects && !address_referable {
            changed = true;
            return false;
        }
        let assigned_side_effect =
            stmt_list_assigns_var_from_side_effecting_expr(&func.body, &binding.name);
        let keep = should_keep_unused_temp_binding(
            is_prunable_unused_temp_binding(binding),
            used || assigned_side_effect,
            initializer_has_side_effects,
        );
        changed |= !keep;
        keep
    });
    changed
}

fn is_prunable_unused_temp_binding(binding: &PreHirBinding) -> bool {
    is_trivial_temp_name(&binding.name) || binding.is_temp_like()
}

fn stmt_list_assigns_var_from_side_effecting_expr(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
}

fn stmt_assigns_var_from_side_effecting_expr(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } => lhs_name == name && expr_has_side_effects(rhs),
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => {
            stmt_list_assigns_var_from_side_effecting_expr(stmts, name)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmt_list_assigns_var_from_side_effecting_expr(then_body, name)
                || stmt_list_assigns_var_from_side_effecting_expr(else_body, name)
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || stmt_list_assigns_var_from_side_effecting_expr(body, name)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmt_list_assigns_var_from_side_effecting_expr(&case.body, name))
                || stmt_list_assigns_var_from_side_effecting_expr(default, name)
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn prune_unused_dead_local_bindings(func: &mut PreHirFunction) -> bool {
    let param_names = func
        .params
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<HashSet<_>>();
    // Built once for the whole function rather than per-binding: same
    // rationale as `prune_unused_temp_bindings` above.
    let use_map = DefUseMap::build(&func.body);
    let mut changed = false;
    func.locals.retain(|binding| {
        // LHS assigns do not count as "uses" in `use_map`, so a
        // write-only stack home (`local_18 = param_3`) would previously drop its
        // binding while leaving the assign — undeclared identifier / compile_error
        // (`matrix_multiply`-class). Keep the binding whenever the name is still
        // defined in the body.
        let keep = !is_dead_local_clobber_name(&binding.name)
            || param_names.contains(binding.name.as_str())
            || binding.name.starts_with("slot_")
            || matches!(binding.ty, NirType::Aggregate { .. })
            || use_map
                .use_count
                .get(binding.name.as_str())
                .copied()
                .unwrap_or(0)
                > 0
            || binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects);
        changed |= !keep;
        keep
    });
    changed
}

/// True when `name` appears as a plain variable assignment target anywhere in
/// `stmts` (definition site, not a use).
fn stmt_list_defines_var(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| stmt_defines_var(stmt, name))
}

fn stmt_defines_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => stmt_list_defines_var(body, name),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => stmt_list_defines_var(then_body, name) || stmt_list_defines_var(else_body, name),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().is_some_and(|s| stmt_defines_var(s, name))
                || update.as_ref().is_some_and(|s| stmt_defines_var(s, name))
                || stmt_list_defines_var(body, name)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmt_list_defines_var(&c.body, name))
                || stmt_list_defines_var(default, name)
        }
        _ => false,
    }
}

fn is_rescue_candidate_name(name: &str) -> bool {
    if name.starts_with("iVar")
        || name.starts_with("uVar")
        || name.starts_with("bVar")
        || name.starts_with("xVar")
    {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
    } else if name.starts_with("tmp_") {
        let suffix = &name[4..];
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if let Some(suffix) = name.strip_prefix("local_") {
        // Stack-home surface names (`local_0`, `local_4`, `local_1c`) from
        // materialize. Used but undeclared → compile_error in semantic harness.
        !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
    } else if matches!(name, "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_") {
        // Named EFLAGS bits (SLA 0x200 layout). Prefer dead-flag cleanup; if a
        // live use remains, declare as Bool so the C harness compiles.
        true
    } else if name.starts_with('r') || name.starts_with('e') {
        name != "reg" && name != "rsp" && name != "rbp" && name != "esp" && name != "ebp"
    } else {
        false
    }
}

pub fn rescue_undeclared_bindings(func: &mut PreHirFunction) -> bool {
    use fission_midend_prehir::util::expr_type;

    let mut declared: HashSet<String> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|b| b.name.clone())
        .collect();

    // Collect every variable name that appears anywhere in the body.
    let mut body_names: HashSet<String> = HashSet::default();
    collect_all_body_names_stmts(&func.body, &mut body_names);

    // Find undeclared names and try to infer their type from the first
    // assignment RHS in the body.
    let mut changed = false;
    for name in &body_names {
        if declared.contains(name.as_str()) {
            continue;
        }
        if !is_rescue_candidate_name(name.as_str()) {
            continue;
        }
        let inferred_ty = if matches!(
            name.as_str(),
            "cf" | "pf" | "af" | "zf" | "sf" | "of" | "df" | "if_"
        ) {
            NirType::Bool
        } else {
            infer_type_from_first_assign(&func.body, name)
        };
        func.locals.push(PreHirBinding {
            name: name.clone(),
            ty: inferred_ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        declared.insert(name.clone());
        changed = true;
    }
    changed
}

fn collect_all_body_names_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
        PreHirExpr::Unary { expr, .. } | PreHirExpr::Cast { expr, .. } => {
            collect_all_body_names_expr(expr, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_all_body_names_expr(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        PreHirExpr::Call { target, args, .. } => {
            // target is a function name String, not PreHirExpr.
            for arg in args {
                collect_all_body_names_expr(arg, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_expr(then_expr, out);
            collect_all_body_names_expr(else_expr, out);
        }
        PreHirExpr::Load { ptr, .. } => {
            collect_all_body_names_expr(ptr, out);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        PreHirExpr::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            collect_all_body_names_expr(src, out);
        }
    }
}

fn collect_all_body_names_lvalue(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(name) => {
            out.insert(name.clone());
        }
        PreHirLValue::Deref { ptr, .. } => collect_all_body_names_expr(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
    }
}

fn collect_all_body_names_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_all_body_names_lvalue(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } | PreHirStmt::Expr(va_list) => {
            collect_all_body_names_expr(va_list, out);
        }
        PreHirStmt::Return(Some(expr)) => collect_all_body_names_expr(expr, out),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
            collect_all_body_names_stmts(body, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_all_body_names_stmts(body, out);
            collect_all_body_names_expr(cond, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_all_body_names_stmt(init, out);
            }
            if let Some(cond) = cond {
                collect_all_body_names_expr(cond, out);
            }
            if let Some(update) = update {
                collect_all_body_names_stmt(update, out);
            }
            collect_all_body_names_stmts(body, out);
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_stmts(then_body, out);
            collect_all_body_names_stmts(else_body, out);
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_all_body_names_expr(expr, out);
            for case in cases {
                collect_all_body_names_stmts(&case.body, out);
            }
            collect_all_body_names_stmts(default, out);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

fn collect_all_body_names_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_all_body_names_stmt(stmt, out);
    }
}

/// Try to infer the type of a variable from its first assignment RHS in the body.
fn infer_type_from_first_assign(stmts: &[PreHirStmt], name: &str) -> NirType {
    use fission_midend_prehir::util::expr_type;
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return ty;
        }
    }
    NirType::Unknown
}

fn infer_type_from_stmt(stmt: &PreHirStmt, name: &str) -> Option<NirType> {
    use fission_midend_prehir::util::expr_type;
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } if lhs_name == name => {
            let ty = expr_type(rhs);
            Some(if ty == NirType::Unknown {
                NirType::Int {
                    bits: 32,
                    signed: true,
                }
            } else {
                ty
            })
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } => {
            infer_type_from_first_assign_stmts(body, name)
        }
        PreHirStmt::DoWhile { body, .. } => infer_type_from_first_assign_stmts(body, name),
        PreHirStmt::For { body, .. } => infer_type_from_first_assign_stmts(body, name),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => infer_type_from_first_assign_stmts(then_body, name)
            .or_else(|| infer_type_from_first_assign_stmts(else_body, name)),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                if let Some(ty) = infer_type_from_first_assign_stmts(&case.body, name) {
                    return Some(ty);
                }
            }
            infer_type_from_first_assign_stmts(default, name)
        }
        _ => None,
    }
}

fn infer_type_from_first_assign_stmts(stmts: &[PreHirStmt], name: &str) -> Option<NirType> {
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return Some(ty);
        }
    }
    None
}

pub fn elide_unused_popcount_assigns(func: &mut PreHirFunction) -> bool {
    if !func.body.iter().any(has_popcount) {
        return false;
    }
    let use_map = DefUseMap::build(&func.body);

    let mut changed = false;
    for _ in 0..8 {
        let round_changed = elide_popcount_round(func, &use_map);
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

fn elide_popcount_round(func: &mut PreHirFunction, use_map: &DefUseMap) -> bool {
    let mut changed = false;
    elide_popcount_in_stmts(&mut func.body, use_map, &mut changed);
    if changed {
        let remaining_names: HashSet<String> =
            func.body.iter().flat_map(collect_assigned_names).collect();
        func.locals.retain(|b| {
            remaining_names.contains(&b.name)
                || use_map.use_count.get(&b.name).copied().unwrap_or(0) > 0
        });
    }
    changed
}

fn collect_assigned_names(stmt: &PreHirStmt) -> Vec<String> {
    let mut names = Vec::new();
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            names.push(name.clone());
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter().chain(else_body.iter()) {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::For { body, .. } => {
            for s in body.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    names.extend(collect_assigned_names(s));
                }
            }
            for s in default.iter() {
                names.extend(collect_assigned_names(s));
            }
        }
        _ => {}
    }
    names
}

fn elide_popcount_in_stmts(stmts: &mut Vec<PreHirStmt>, use_map: &DefUseMap, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body) => elide_popcount_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                use_map,
                changed,
            ),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    use_map,
                    changed,
                );
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::For { body, .. } => {
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    use_map,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    elide_popcount_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        use_map,
                        changed,
                    );
                }
                elide_popcount_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    use_map,
                    changed,
                );
            }
            _ => {}
        }
    }
    stmts.retain(|stmt| {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = stmt
        {
            let uses = use_map.use_count.get(name.as_str()).copied().unwrap_or(0);
            if uses == 0 && rhs_contains_popcount(rhs) && !expr_has_side_effects(rhs) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn rhs_contains_popcount(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { target, .. } if target == "__popcount" => true,
        PreHirExpr::Cast { expr: inner, .. } | PreHirExpr::Unary { expr: inner, .. } => {
            rhs_contains_popcount(inner)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rhs_contains_popcount(lhs) || rhs_contains_popcount(rhs)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(rhs_contains_popcount),
        _ => false,
    }
}

fn has_popcount(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { rhs, .. } => rhs_contains_popcount(rhs),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => rhs_contains_popcount(expr),
        PreHirStmt::Block(body) => body.iter().any(has_popcount),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rhs_contains_popcount(cond)
                || then_body.iter().any(has_popcount)
                || else_body.iter().any(has_popcount)
        }
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
            rhs_contains_popcount(cond) || body.iter().any(has_popcount)
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(has_popcount)
                || cond.as_ref().is_some_and(rhs_contains_popcount)
                || update.as_deref().is_some_and(has_popcount)
                || body.iter().any(has_popcount)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            rhs_contains_popcount(expr)
                || cases.iter().any(|c| c.body.iter().any(has_popcount))
                || default.iter().any(has_popcount)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Coerce pointer-typed variables used in integer-only bit operations
// ---------------------------------------------------------------------------

/// Collect variable names that appear as the LHS of an assignment where the RHS
/// is a bitwise-integer-only binary operation (And, Or, Xor, Shl, Shr, Sar).
/// These variables must have an integer (not pointer) type to compile as valid C.
fn collect_bitop_lhs_vars_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_bitop_lhs_vars_stmt(stmt, out);
    }
}

fn collect_bitop_lhs_vars_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            if rhs_is_integer_bitop(rhs) {
                out.insert(name.clone());
            }
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            collect_bitop_lhs_vars_stmts(body, out);
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_bitop_lhs_vars_stmts(then_body, out);
            collect_bitop_lhs_vars_stmts(else_body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_bitop_lhs_vars_stmts(&case.body, out);
            }
            collect_bitop_lhs_vars_stmts(default, out);
        }
        _ => {}
    }
}

fn rhs_is_integer_bitop(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Binary { op, .. } => matches!(
            op,
            PreHirBinaryOp::And
                | PreHirBinaryOp::Or
                | PreHirBinaryOp::Xor
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar
        ),
        PreHirExpr::Cast { expr: inner, .. } => rhs_is_integer_bitop(inner),
        _ => false,
    }
}

/// Safety-net pass: if a local binding has `NirType::Ptr(_)` but is used as the
/// destination of a bitwise-integer-only operation, coerce its type to `ulonglong`
/// so that the generated C compiles cleanly.
///
/// This handles x86-64 idioms where a pointer difference is computed, stored in
/// a pointer-typed slot, and then bit-masked (e.g. `ptr_diff &= 4`).
pub fn coerce_ptr_typed_bitop_vars(func: &mut PreHirFunction) -> bool {
    // Collect all LHS names that receive a bitwise-integer RHS.
    let mut bitop_lhs: HashSet<String> = HashSet::default();
    collect_bitop_lhs_vars_stmts(&func.body, &mut bitop_lhs);
    if bitop_lhs.is_empty() {
        return false;
    }

    let int64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };

    let mut changed = false;
    for binding in &mut func.locals {
        if bitop_lhs.contains(&binding.name) && matches!(binding.ty, NirType::Ptr(_)) {
            binding.ty = int64_ty.clone();
            // Drop any pointer initializer so it doesn't conflict with the new integer type.
            if binding.initializer.is_some() {
                binding.initializer = None;
            }
            changed = true;
        }
    }
    changed
}
