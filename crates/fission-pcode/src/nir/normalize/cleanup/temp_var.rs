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
    eliminate_dead_local_clobber_assigns_in_stmts(&mut func.body, &func.params, &func.locals)
}

fn eliminate_dead_local_clobber_assigns_in_stmts(
    stmts: &mut Vec<HirStmt>,
    params: &[NirBinding],
    locals: &[NirBinding],
) -> bool {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                eliminate_dead_local_clobber_assigns_in_stmts(body, params, locals);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                eliminate_dead_local_clobber_assigns_in_stmts(then_body, params, locals);
                eliminate_dead_local_clobber_assigns_in_stmts(else_body, params, locals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    eliminate_dead_local_clobber_assigns_in_stmts(&mut case.body, params, locals);
                }
                eliminate_dead_local_clobber_assigns_in_stmts(default, params, locals);
            }
            _ => {}
        }
    }

    let local_types = locals
        .iter()
        .map(|binding| (binding.name.as_str(), &binding.ty))
        .collect::<HashMap<_, _>>();
    let param_names = params
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<HashSet<_>>();

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
        if matches!(
            local_types.get(name).copied(),
            Some(NirType::Aggregate { .. } | NirType::Ptr(_))
        ) {
            continue;
        }
        if count_uses_in_stmt_list(stmts, name) == 0 {
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

fn elide_popcount_round(
    func: &mut HirFunction,
    use_map: &DefUseMap,
) -> bool {
    let mut changed = false;
    elide_popcount_in_stmts(&mut func.body, use_map, &mut changed);
    if changed {
        let remaining_names: HashSet<String> = func
            .body
            .iter()
            .flat_map(collect_assigned_names)
            .collect();
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

fn elide_popcount_in_stmts(
    stmts: &mut Vec<HirStmt>,
    use_map: &DefUseMap,
    changed: &mut bool,
) {
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
        HirStmt::If { cond, then_body, else_body } => {
            rhs_contains_popcount(cond)
                || then_body.iter().any(has_popcount)
                || else_body.iter().any(has_popcount)
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rhs_contains_popcount(cond) || body.iter().any(has_popcount)
        }
        HirStmt::For { init, cond, update, body } => {
            init.as_deref().is_some_and(has_popcount)
                || cond.as_ref().is_some_and(rhs_contains_popcount)
                || update.as_deref().is_some_and(has_popcount)
                || body.iter().any(has_popcount)
        }
        HirStmt::Switch { expr, cases, default } => {
            rhs_contains_popcount(expr)
                || cases.iter().any(|c| c.body.iter().any(has_popcount))
                || default.iter().any(has_popcount)
        }
        _ => false,
    }
}
