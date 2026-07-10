use super::super::analysis::defuse::DefUseMap;
use super::super::analysis::preservation::{
    should_block_trivial_return_collapse, should_keep_unused_temp_binding,
    should_skip_inline_for_preserved_temp,
};
use super::super::wave_stats;
use super::super::*;
use super::utils::*;
use std::collections::{HashMap, HashSet};

pub(crate) fn collapse_trivial_assign_returns(
    stmts: &mut Vec<HirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    let mut changed = false;
    let mut blocked = 0usize;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let replacement = match (&stmts[idx], &stmts[idx + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    rhs,
                },
                HirStmt::Return(Some(HirExpr::Var(ret_name))),
            ) if name == ret_name && is_trivial_temp_name(name) => {
                if should_block_trivial_return_collapse(name, preserved_temps) {
                    blocked += 1;
                    None
                } else {
                    Some(rhs.clone())
                }
            }
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[idx + 1] = HirStmt::Return(Some(expr));
            to_remove[idx] = true;
            changed = true;
        }
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }
    wave_stats::add_preserved_temp_prune_blocked(blocked);
    changed
}

pub(crate) fn inline_single_use_temps(
    stmts: &mut Vec<HirStmt>,
    preserved_temps: &HashSet<&str>,
) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    inline_single_use_temps_recursive(stmts, preserved_temps, &use_counts)
}

fn inline_single_use_temps_recursive(
    stmts: &mut Vec<HirStmt>,
    preserved_temps: &HashSet<&str>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let (name, rhs) = match &stmts[idx] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
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
        if total_uses != target_uses {
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
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= inline_single_use_temps_recursive(body, preserved_temps, use_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = &mut **i {
                        changed |=
                            inline_single_use_temps_recursive(b, preserved_temps, use_counts);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = &mut **u {
                        changed |=
                            inline_single_use_temps_recursive(b, preserved_temps, use_counts);
                    }
                }
                changed |= inline_single_use_temps_recursive(body, preserved_temps, use_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    inline_single_use_temps_recursive(then_body, preserved_temps, use_counts);
                changed |=
                    inline_single_use_temps_recursive(else_body, preserved_temps, use_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_single_use_temps_recursive(
                        &mut case.body,
                        preserved_temps,
                        use_counts,
                    );
                }
                changed |= inline_single_use_temps_recursive(default, preserved_temps, use_counts);
            }
            _ => {}
        }
    }

    changed
}

fn find_inline_forward_target(
    stmts: &[HirStmt],
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

fn stmt_blocks_linear_inline_scan(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, HirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        HirStmt::Expr(expr) => expr_has_side_effects(expr),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::VaStart { .. }
        | HirStmt::Block(_)
        | HirStmt::Switch { .. }
        | HirStmt::If { .. }
        | HirStmt::While { .. }
        | HirStmt::DoWhile { .. }
        | HirStmt::For { .. }
        | HirStmt::Break
        | HirStmt::Continue => true,
    }
}

fn stmt_blocks_stable_inline_scan(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, HirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        HirStmt::Label(_) => false,
        HirStmt::Return(None)
        | HirStmt::VaStart { .. }
        | HirStmt::Block(_)
        | HirStmt::Switch { .. }
        | HirStmt::If { .. }
        | HirStmt::While { .. }
        | HirStmt::DoWhile { .. }
        | HirStmt::For { .. }
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => true,
    }
}

fn stmt_allows_forward_scan(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(_),
            rhs,
        } => !expr_has_side_effects(rhs),
        HirStmt::Return(Some(expr)) => !expr_has_side_effects(expr),
        HirStmt::If { cond, .. } => !expr_has_side_effects(cond),
        HirStmt::Expr(expr) => !expr_has_side_effects(expr),
        _ => false,
    }
}

fn stmt_allows_inline_target(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign { .. } | HirStmt::Expr(_) | HirStmt::Return(_) | HirStmt::If { .. }
    )
}

fn stmt_redefines_temp(stmt: &HirStmt, name: &str) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            ..
        } if lhs_name == name
    )
}

fn stmt_uses_var_in_predicate_position(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::If { cond, .. } => expr_contains_var(cond, name),
        HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => {
            expr_contains_var(cond, name)
        }
        HirStmt::For {
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
        HirStmt::Switch { expr, .. } => expr_contains_var(expr, name),
        HirStmt::Block(stmts) => stmts
            .iter()
            .any(|inner| stmt_uses_var_in_predicate_position(inner, name)),
        _ => false,
    }
}

fn expr_is_low_cost_inline_candidate(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
        HirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            expr_is_low_cost_inline_candidate(expr)
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                HirBinaryOp::Eq
                    | HirBinaryOp::Ne
                    | HirBinaryOp::Lt
                    | HirBinaryOp::Le
                    | HirBinaryOp::SLt
                    | HirBinaryOp::SLe
                    | HirBinaryOp::And
                    | HirBinaryOp::Or
                    | HirBinaryOp::Xor
                    | HirBinaryOp::Add
                    | HirBinaryOp::Sub
                    | HirBinaryOp::Shl
                    | HirBinaryOp::Shr
                    | HirBinaryOp::Sar
                    | HirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

fn expr_prefers_stable_materialization(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. } => expr_prefers_stable_materialization(expr),
        HirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().any(expr_prefers_stable_materialization)
        }
        HirExpr::Unary { .. }
        | HirExpr::Load { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::Select { .. }
        | HirExpr::AggregateCopy { .. }
        | HirExpr::FieldAccess { .. }
        | HirExpr::Call { .. } => true,
        HirExpr::Binary { op, .. } => matches!(
            op,
            HirBinaryOp::Add
                | HirBinaryOp::Sub
                | HirBinaryOp::Mul
                | HirBinaryOp::Div
                | HirBinaryOp::Mod
                | HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar
                | HirBinaryOp::Eq
                | HirBinaryOp::Ne
                | HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe
        ),
    }
}

pub(crate) fn eliminate_dead_temp_assigns(
    stmts: &mut Vec<HirStmt>,
    _preserved_temps: &HashSet<&str>,
) -> bool {
    let use_counts = DefUseMap::build(stmts).use_count;
    eliminate_dead_temp_assigns_recursive(stmts, &use_counts)
}

fn eliminate_dead_temp_assigns_recursive(
    stmts: &mut Vec<HirStmt>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for (idx, stmt) in stmts.iter().enumerate() {
        let (name, rhs) = match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
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
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= eliminate_dead_temp_assigns_recursive(body, use_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = &mut **i {
                        changed |= eliminate_dead_temp_assigns_recursive(b, use_counts);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = &mut **u {
                        changed |= eliminate_dead_temp_assigns_recursive(b, use_counts);
                    }
                }
                changed |= eliminate_dead_temp_assigns_recursive(body, use_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_dead_temp_assigns_recursive(then_body, use_counts);
                changed |= eliminate_dead_temp_assigns_recursive(else_body, use_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_dead_temp_assigns_recursive(&mut case.body, use_counts);
                }
                changed |= eliminate_dead_temp_assigns_recursive(default, use_counts);
            }
            _ => {}
        }
    }

    changed
}

pub(crate) fn eliminate_redundant_var_assigns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];

    for idx in 0..stmts.len() {
        let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = &stmts[idx]
        else {
            continue;
        };

        if matches!(rhs, HirExpr::Var(rhs_name) if rhs_name == name) {
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

        let HirStmt::Assign {
            lhs: HirLValue::Var(prev_name),
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
    changed
}

fn redundant_assign_rhs_equal(lhs: &HirExpr, rhs: &HirExpr) -> bool {
    lhs == rhs
        || matches!(
            (lhs, rhs),
            (HirExpr::Const(lhs_value, _), HirExpr::Const(rhs_value, _)) if lhs_value == rhs_value
        )
}

pub(crate) fn eliminate_dead_local_clobber_assigns(func: &mut HirFunction) -> bool {
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
    stmts: &mut Vec<HirStmt>,
    param_names: &HashSet<&str>,
    local_types: &HashMap<&str, &NirType>,
    stack_backed_names: &HashSet<&str>,
    use_map: &DefUseMap,
) -> bool {
    // Recurse into nested bodies first (the use_map is already whole-function).
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                eliminate_dead_local_clobber_assigns_in_stmts(
                    then_body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
                eliminate_dead_local_clobber_assigns_in_stmts(
                    else_body,
                    param_names,
                    local_types,
                    stack_backed_names,
                    use_map,
                );
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    eliminate_dead_local_clobber_assigns_in_stmts(
                        &mut case.body,
                        param_names,
                        local_types,
                        stack_backed_names,
                        use_map,
                    );
                }
                eliminate_dead_local_clobber_assigns_in_stmts(
                    default,
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
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
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

pub(crate) fn prune_unused_temp_bindings(func: &mut HirFunction) -> bool {
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = count_uses_in_stmt_list(&func.body, &binding.name) > 0;
        let assigned_side_effect =
            stmt_list_assigns_var_from_side_effecting_expr(&func.body, &binding.name);
        let keep = should_keep_unused_temp_binding(
            is_prunable_unused_temp_binding(binding),
            used || assigned_side_effect,
            binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects),
        );
        changed |= !keep;
        keep
    });
    changed
}

fn is_prunable_unused_temp_binding(binding: &NirBinding) -> bool {
    is_trivial_temp_name(&binding.name) || binding.is_temp_like()
}

fn stmt_list_assigns_var_from_side_effecting_expr(stmts: &[HirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
}

fn stmt_assigns_var_from_side_effecting_expr(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            rhs,
        } => lhs_name == name && expr_has_side_effects(rhs),
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => {
            stmt_list_assigns_var_from_side_effecting_expr(stmts, name)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmt_list_assigns_var_from_side_effecting_expr(then_body, name)
                || stmt_list_assigns_var_from_side_effecting_expr(else_body, name)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || stmt_list_assigns_var_from_side_effecting_expr(body, name)
        }
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmt_list_assigns_var_from_side_effecting_expr(&case.body, name))
                || stmt_list_assigns_var_from_side_effecting_expr(default, name)
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

pub(crate) fn prune_unused_dead_local_bindings(func: &mut HirFunction) -> bool {
    let param_names = func
        .params
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<HashSet<_>>();
    let mut changed = false;
    func.locals.retain(|binding| {
        let keep = !is_dead_local_clobber_name(&binding.name)
            || param_names.contains(binding.name.as_str())
            || binding.name.starts_with("slot_")
            || matches!(binding.ty, NirType::Aggregate { .. })
            || count_uses_in_stmt_list(&func.body, &binding.name) > 0
            || binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects);
        changed |= !keep;
        keep
    });
    changed
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

pub(crate) fn rescue_undeclared_bindings(func: &mut HirFunction) -> bool {
    use crate::nir::support::expr_type;

    let mut declared: HashSet<String> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|b| b.name.clone())
        .collect();

    // Collect every variable name that appears anywhere in the body.
    let mut body_names: HashSet<String> = HashSet::new();
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
        func.locals.push(NirBinding {
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

fn collect_all_body_names_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            out.insert(name.clone());
        }
        HirExpr::Const(_, _) | HirExpr::AddressOfGlobal(_) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            collect_all_body_names_expr(expr, out);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_all_body_names_expr(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        HirExpr::Call { target, args, .. } => {
            // target is a function name String, not HirExpr.
            for arg in args {
                collect_all_body_names_expr(arg, out);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_expr(then_expr, out);
            collect_all_body_names_expr(else_expr, out);
        }
        HirExpr::Load { ptr, .. } => {
            collect_all_body_names_expr(ptr, out);
        }
        HirExpr::PtrOffset { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        HirExpr::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        HirExpr::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
        HirExpr::AggregateCopy { src, .. } => {
            collect_all_body_names_expr(src, out);
        }
    }
}

fn collect_all_body_names_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        HirLValue::Var(name) => {
            out.insert(name.clone());
        }
        HirLValue::Deref { ptr, .. } => collect_all_body_names_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_all_body_names_expr(base, out);
            collect_all_body_names_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => {
            collect_all_body_names_expr(base, out);
        }
    }
}

fn collect_all_body_names_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_all_body_names_lvalue(lhs, out);
            collect_all_body_names_expr(rhs, out);
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => {
            collect_all_body_names_expr(va_list, out);
        }
        HirStmt::Return(Some(expr)) => collect_all_body_names_expr(expr, out),
        HirStmt::Block(body) | HirStmt::While { body, .. } => {
            collect_all_body_names_stmts(body, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_all_body_names_stmts(body, out);
            collect_all_body_names_expr(cond, out);
        }
        HirStmt::For {
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
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_all_body_names_expr(cond, out);
            collect_all_body_names_stmts(then_body, out);
            collect_all_body_names_stmts(else_body, out);
        }
        HirStmt::Switch {
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
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_all_body_names_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_all_body_names_stmt(stmt, out);
    }
}

/// Try to infer the type of a variable from its first assignment RHS in the body.
fn infer_type_from_first_assign(stmts: &[HirStmt], name: &str) -> NirType {
    use crate::nir::support::expr_type;
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return ty;
        }
    }
    NirType::Unknown
}

fn infer_type_from_stmt(stmt: &HirStmt, name: &str) -> Option<NirType> {
    use crate::nir::support::expr_type;
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
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
        HirStmt::Block(body) | HirStmt::While { body, .. } => {
            infer_type_from_first_assign_stmts(body, name)
        }
        HirStmt::DoWhile { body, .. } => infer_type_from_first_assign_stmts(body, name),
        HirStmt::For { body, .. } => infer_type_from_first_assign_stmts(body, name),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => infer_type_from_first_assign_stmts(then_body, name)
            .or_else(|| infer_type_from_first_assign_stmts(else_body, name)),
        HirStmt::Switch { cases, default, .. } => {
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

fn infer_type_from_first_assign_stmts(stmts: &[HirStmt], name: &str) -> Option<NirType> {
    for stmt in stmts {
        if let Some(ty) = infer_type_from_stmt(stmt, name) {
            return Some(ty);
        }
    }
    None
}

pub(crate) fn elide_unused_popcount_assigns(func: &mut HirFunction) -> bool {
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

fn elide_popcount_round(func: &mut HirFunction, use_map: &DefUseMap) -> bool {
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

fn collect_assigned_names(stmt: &HirStmt) -> Vec<String> {
    let mut names = Vec::new();
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            names.push(name.clone());
        }
        HirStmt::Block(body) => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter().chain(else_body.iter()) {
                names.extend(collect_assigned_names(s));
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        HirStmt::For { body, .. } => {
            for s in body {
                names.extend(collect_assigned_names(s));
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    names.extend(collect_assigned_names(s));
                }
            }
            for s in default {
                names.extend(collect_assigned_names(s));
            }
        }
        _ => {}
    }
    names
}

fn elide_popcount_in_stmts(stmts: &mut Vec<HirStmt>, use_map: &DefUseMap, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) => elide_popcount_in_stmts(body, use_map, changed),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                elide_popcount_in_stmts(then_body, use_map, changed);
                elide_popcount_in_stmts(else_body, use_map, changed);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                elide_popcount_in_stmts(body, use_map, changed);
            }
            HirStmt::For { body, .. } => {
                elide_popcount_in_stmts(body, use_map, changed);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    elide_popcount_in_stmts(&mut case.body, use_map, changed);
                }
                elide_popcount_in_stmts(default, use_map, changed);
            }
            _ => {}
        }
    }
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
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

fn rhs_contains_popcount(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { target, .. } if target == "__popcount" => true,
        HirExpr::Cast { expr: inner, .. } | HirExpr::Unary { expr: inner, .. } => {
            rhs_contains_popcount(inner)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            rhs_contains_popcount(lhs) || rhs_contains_popcount(rhs)
        }
        HirExpr::Call { args, .. } => args.iter().any(rhs_contains_popcount),
        _ => false,
    }
}

fn has_popcount(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { rhs, .. } => rhs_contains_popcount(rhs),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => rhs_contains_popcount(expr),
        HirStmt::Block(body) => body.iter().any(has_popcount),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rhs_contains_popcount(cond)
                || then_body.iter().any(has_popcount)
                || else_body.iter().any(has_popcount)
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rhs_contains_popcount(cond) || body.iter().any(has_popcount)
        }
        HirStmt::For {
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
        HirStmt::Switch {
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
fn collect_bitop_lhs_vars_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_bitop_lhs_vars_stmt(stmt, out);
    }
}

fn collect_bitop_lhs_vars_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if rhs_is_integer_bitop(rhs) {
                out.insert(name.clone());
            }
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            collect_bitop_lhs_vars_stmts(body, out);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_bitop_lhs_vars_stmts(then_body, out);
            collect_bitop_lhs_vars_stmts(else_body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_bitop_lhs_vars_stmts(&case.body, out);
            }
            collect_bitop_lhs_vars_stmts(default, out);
        }
        _ => {}
    }
}

fn rhs_is_integer_bitop(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary { op, .. } => matches!(
            op,
            HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Shl
                | HirBinaryOp::Shr
                | HirBinaryOp::Sar
        ),
        HirExpr::Cast { expr: inner, .. } => rhs_is_integer_bitop(inner),
        _ => false,
    }
}

/// Safety-net pass: if a local binding has `NirType::Ptr(_)` but is used as the
/// destination of a bitwise-integer-only operation, coerce its type to `ulonglong`
/// so that the generated C compiles cleanly.
///
/// This handles x86-64 idioms where a pointer difference is computed, stored in
/// a pointer-typed slot, and then bit-masked (e.g. `ptr_diff &= 4`).
pub(crate) fn coerce_ptr_typed_bitop_vars(func: &mut HirFunction) -> bool {
    // Collect all LHS names that receive a bitwise-integer RHS.
    let mut bitop_lhs: HashSet<String> = HashSet::new();
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
