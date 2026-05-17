use super::super::analysis::defuse::DefUseMap;
use super::super::analysis::preservation::{
    should_block_trivial_return_collapse, should_keep_unused_temp_binding,
    should_skip_inline_for_preserved_temp,
};
use super::super::wave_stats;
use super::super::*;
use crate::nir::structuring::cleanup_redundant_labels;
use std::collections::{HashMap, HashSet};

pub(crate) fn collapse_trivial_assign_returns(
    stmts: &mut Vec<HirStmt>,
    preserved_temps: &HashSet<String>,
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
    preserved_temps: &HashSet<String>,
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
        let total_uses = count_uses_in_stmt_list(stmts, &name);
        if total_uses != target_uses {
            idx += 1;
            continue;
        }
        let predicate_sensitive = stmt_uses_var_in_predicate_position(&stmts[target_idx], &name);
        let low_cost_inline = expr_is_low_cost_inline_candidate(&rhs);
        if predicate_sensitive && prefers_stable_materialization {
            idx += 1;
            continue;
        }
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
    changed
}

pub(crate) fn collapse_loop_exit_alias_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx + 1 < stmts.len() {
        let Some(alias) = return_var_name(&stmts[idx + 1]).map(str::to_string) else {
            idx += 1;
            continue;
        };
        if count_uses_in_stmt_list(stmts, &alias) != 1 {
            idx += 1;
            continue;
        }
        if !loop_executes_before_exit_return(stmts, idx) {
            idx += 1;
            continue;
        }
        let Some(source) = loop_exit_alias_source(&stmts[idx], &alias) else {
            idx += 1;
            continue;
        };
        let source_expr = HirExpr::Var(source.clone());
        if remove_loop_exit_alias_assignment(&mut stmts[idx], &alias, &source) {
            stmts[idx + 1] = HirStmt::Return(Some(source_expr));
            changed = true;
        }
        idx += 1;
    }

    changed
}

fn return_var_name(stmt: &HirStmt) -> Option<&str> {
    match stmt {
        HirStmt::Return(Some(HirExpr::Var(name))) => Some(name.as_str()),
        _ => None,
    }
}

fn loop_executes_before_exit_return(stmts: &[HirStmt], loop_idx: usize) -> bool {
    match stmts.get(loop_idx) {
        Some(HirStmt::DoWhile { .. }) => true,
        Some(HirStmt::For { init, cond, .. }) => {
            for_loop_guard_proves_first_iteration(stmts, loop_idx, init.as_deref(), cond.as_ref())
        }
        _ => false,
    }
}

fn loop_exit_alias_source(stmt: &HirStmt, alias: &str) -> Option<String> {
    match stmt {
        HirStmt::DoWhile { body, cond } => loop_body_exit_alias_source(body, alias)
            .filter(|source| !expr_mentions_var(cond, alias) && !expr_mentions_var(cond, source)),
        HirStmt::For {
            update, body, cond, ..
        } => loop_body_exit_alias_source(body, alias).filter(|source| {
            cond.as_ref()
                .is_none_or(|cond| !expr_mentions_var(cond, alias))
                && update.as_deref().is_none_or(|update| {
                    !stmt_mentions_var(update, alias) && !stmt_assigns_var(update, source)
                })
        }),
        _ => None,
    }
}

fn loop_body_exit_alias_source(body: &[HirStmt], alias: &str) -> Option<String> {
    let mut match_idx = None;
    let mut match_source = None;

    for (idx, stmt) in body.iter().enumerate() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            rhs: HirExpr::Var(source),
        } = stmt
        {
            if lhs == alias && source != alias {
                if match_idx.is_some() {
                    return None;
                }
                match_idx = Some(idx);
                match_source = Some(source.clone());
            }
        } else if stmt_assigns_var(stmt, alias) {
            return None;
        }
    }

    let idx = match_idx?;
    let source = match_source?;
    if body[idx + 1..]
        .iter()
        .any(|stmt| stmt_assigns_var(stmt, &source) || stmt_mentions_var(stmt, alias))
    {
        return None;
    }
    Some(source)
}

fn remove_loop_exit_alias_assignment(stmt: &mut HirStmt, alias: &str, source: &str) -> bool {
    let body = match stmt {
        HirStmt::DoWhile { body, .. } | HirStmt::For { body, .. } => body,
        _ => return false,
    };
    let Some(idx) = body.iter().position(|stmt| {
        matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(lhs),
                rhs: HirExpr::Var(rhs),
            } if lhs == alias && rhs == source
        )
    }) else {
        return false;
    };
    body.remove(idx);
    true
}

fn for_loop_guard_proves_first_iteration(
    stmts: &[HirStmt],
    loop_idx: usize,
    init: Option<&HirStmt>,
    cond: Option<&HirExpr>,
) -> bool {
    let Some(exit_label) = stmts.get(loop_idx + 2).and_then(|stmt| match stmt {
        HirStmt::Label(label) => Some(label.as_str()),
        _ => None,
    }) else {
        return false;
    };
    let Some((_iv, bound)) = zero_based_less_than_bound(init, cond) else {
        return false;
    };

    stmts[..loop_idx].iter().any(|stmt| {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return false;
        };
        else_body.is_empty()
            && matches_single_goto(then_body, exit_label)
            && guard_excludes_zero_iteration(cond, &bound)
    })
}

fn zero_based_less_than_bound(
    init: Option<&HirStmt>,
    cond: Option<&HirExpr>,
) -> Option<(String, String)> {
    let HirStmt::Assign {
        lhs: HirLValue::Var(init_var),
        rhs,
    } = init?
    else {
        return None;
    };
    if expr_as_const_ignoring_casts(rhs) != Some(0) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Lt | HirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = cond?
    else {
        return None;
    };
    let cond_var = expr_as_var_ignoring_casts(lhs)?;
    if cond_var != init_var {
        return None;
    }
    let bound = expr_as_var_ignoring_casts(rhs)?;
    Some((init_var.clone(), bound.to_string()))
}

fn guard_excludes_zero_iteration(cond: &HirExpr, bound: &str) -> bool {
    let HirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return false;
    };
    let lhs_var = expr_as_var_ignoring_casts(lhs);
    let rhs_var = expr_as_var_ignoring_casts(rhs);
    let lhs_const = expr_as_const_ignoring_casts(lhs);
    let rhs_const = expr_as_const_ignoring_casts(rhs);

    matches!(
        (op, lhs_var, rhs_const),
        (HirBinaryOp::Le | HirBinaryOp::SLe, Some(var), Some(0)) if var == bound
    ) || matches!(
        (op, lhs_const, rhs_var),
        (HirBinaryOp::Ge | HirBinaryOp::SGe, Some(0), Some(var)) if var == bound
    )
}

fn expr_as_var_ignoring_casts(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => expr_as_var_ignoring_casts(expr),
        _ => None,
    }
}

fn expr_as_const_ignoring_casts(expr: &HirExpr) -> Option<i64> {
    match expr {
        HirExpr::Const(value, _) => Some(*value),
        HirExpr::Cast { expr, .. } => expr_as_const_ignoring_casts(expr),
        _ => None,
    }
}

fn stmt_mentions_var(stmt: &HirStmt, name: &str) -> bool {
    count_var_uses_in_stmt(stmt, name) > 0
}

fn stmt_assigns_var(stmt: &HirStmt, name: &str) -> bool {
    var_is_assigned_in_stmt(stmt, name)
}

fn stmt_may_bypass_following_stmts(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Return(_) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => true,
        HirStmt::Block(body) => body.iter().any(stmt_may_bypass_following_stmts),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_may_bypass_following_stmts)
                || else_body.iter().any(stmt_may_bypass_following_stmts)
        }
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| case.body.iter().any(stmt_may_bypass_following_stmts))
                || default.iter().any(stmt_may_bypass_following_stmts)
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } | HirStmt::For { body, .. } => {
            body.iter().any(stmt_may_bypass_following_stmts)
        }
        HirStmt::Assign { .. } | HirStmt::Expr(_) | HirStmt::VaStart { .. } | HirStmt::Label(_) => {
            false
        }
    }
}

fn stmt_assigns_any_expr_var(stmt: &HirStmt, expr: &HirExpr) -> bool {
    match stmt {
        HirStmt::Assign { lhs, .. } => lvalue_assigns_any_expr_var(lhs, expr),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => body
            .iter()
            .any(|stmt| stmt_assigns_any_expr_var(stmt, expr)),
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_any_expr_var(stmt, expr))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_any_expr_var(stmt, expr))
                || body
                    .iter()
                    .any(|stmt| stmt_assigns_any_expr_var(stmt, expr))
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_assigns_any_expr_var(stmt, expr))
                || else_body
                    .iter()
                    .any(|stmt| stmt_assigns_any_expr_var(stmt, expr))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| {
                case.body
                    .iter()
                    .any(|stmt| stmt_assigns_any_expr_var(stmt, expr))
            }) || default
                .iter()
                .any(|stmt| stmt_assigns_any_expr_var(stmt, expr))
        }
        HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::VaStart { .. }
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn lvalue_assigns_any_expr_var(lhs: &HirLValue, expr: &HirExpr) -> bool {
    match lhs {
        HirLValue::Var(name) => expr_contains_var(expr, name),
        HirLValue::Deref { .. } | HirLValue::Index { .. } => false,
    }
}

pub(crate) fn prune_unreachable_after_terminal(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let referenced_labels = collect_referenced_labels(stmts);
    let mut idx = 0usize;
    while idx < stmts.len() {
        if !is_unconditional_terminal(&stmts[idx]) {
            idx += 1;
            continue;
        }

        let mut end = idx + 1;
        while end < stmts.len() && !stmt_contains_referenced_label(&stmts[end], &referenced_labels)
        {
            end += 1;
        }
        if end > idx + 1 {
            stmts.drain(idx + 1..end);
            changed = true;
        }
        idx += 1;
    }
    changed
}

fn is_unconditional_terminal(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Return(_) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue
    )
}

fn stmt_contains_referenced_label(stmt: &HirStmt, referenced_labels: &HashSet<String>) -> bool {
    match stmt {
        HirStmt::Label(label) => referenced_labels.contains(label),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => body
            .iter()
            .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels)),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body
                .iter()
                .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                || else_body
                    .iter()
                    .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
        }
        HirStmt::Switch { cases, default, .. } => {
            default
                .iter()
                .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                || cases.iter().any(|case| {
                    case.body
                        .iter()
                        .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                })
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

pub(crate) fn eliminate_dead_temp_assigns(
    stmts: &mut Vec<HirStmt>,
    _preserved_temps: &HashSet<String>,
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

        let uses = count_uses_in_stmt_list(stmts, name);
        let side_effects = expr_has_side_effects(rhs);
        if uses == 0 && !side_effects {
            to_remove[idx] = true;
            changed = true;
        }
    }

    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
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

pub(crate) fn collapse_trivial_pointer_alias_bindings(func: &mut HirFunction) -> bool {
    let mut aliases = HashMap::<String, HirExpr>::new();
    for binding in &func.locals {
        if !matches!(binding.ty, NirType::Ptr(_)) {
            continue;
        }
        if binding.name.starts_with("slot_") && should_preserve_slot_alias_binding(func, binding) {
            continue;
        }
        let Some(initializer) = binding.initializer.as_ref() else {
            continue;
        };
        let Some(replacement) = pointer_alias_replacement(initializer) else {
            continue;
        };
        if expr_mentions_var(&replacement, &binding.name)
            || expr_has_side_effects(&replacement)
            || var_is_assigned_in_stmts(&func.body, &binding.name)
        {
            continue;
        }
        let use_count = count_uses_in_stmt_list(&func.body, &binding.name)
            + count_uses_in_bindings(&func.locals, &binding.name);
        if use_count > 0 {
            aliases.insert(binding.name.clone(), replacement);
        }
    }
    if aliases.is_empty() {
        return false;
    }

    for (name, replacement) in &aliases {
        for stmt in &mut func.body {
            replace_var_in_stmt(stmt, name, replacement);
        }
        for binding in &mut func.locals {
            if binding.name != *name
                && let Some(initializer) = &mut binding.initializer
            {
                replace_var_in_expr(initializer, name, replacement);
            }
        }
    }

    let before = func.locals.len();
    func.locals
        .retain(|binding| !aliases.contains_key(&binding.name));
    before != func.locals.len()
}

fn should_preserve_slot_alias_binding(func: &HirFunction, binding: &NirBinding) -> bool {
    binding.surface_type_name.is_some()
        || matches!(
            binding.origin,
            Some(NirBindingOrigin::StackOffset(_))
                | Some(NirBindingOrigin::DerivedFromStackOffset(_))
        )
        || binding
            .initializer
            .as_ref()
            .and_then(ptr_offset_const)
            .is_some_and(|offset| offset != 0)
        || stmt_list_uses_var_as_index_base(&func.body, &binding.name)
}

fn ptr_offset_const(expr: &HirExpr) -> Option<i64> {
    match expr {
        HirExpr::PtrOffset { offset, .. } => Some(*offset),
        HirExpr::Cast { expr, .. } => ptr_offset_const(expr),
        _ => Some(0),
    }
}

fn stmt_list_uses_var_as_index_base(stmts: &[HirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_uses_var_as_index_base(stmt, name))
}

fn stmt_uses_var_as_index_base(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            lvalue_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        HirStmt::Expr(expr)
        | HirStmt::Return(Some(expr))
        | HirStmt::VaStart { va_list: expr, .. } => expr_uses_var_as_index_base(expr, name),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => stmt_list_uses_var_as_index_base(body, name),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var_as_index_base(cond, name)
                || stmt_list_uses_var_as_index_base(then_body, name)
                || stmt_list_uses_var_as_index_base(else_body, name)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_var_as_index_base(expr, name)
                || cases
                    .iter()
                    .any(|case| stmt_list_uses_var_as_index_base(&case.body, name))
                || stmt_list_uses_var_as_index_base(default, name)
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn lvalue_uses_var_as_index_base(lhs: &HirLValue, name: &str) -> bool {
    match lhs {
        HirLValue::Index { base, index, .. } => {
            matches!(base.as_ref(), HirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        HirLValue::Deref { ptr, .. } => expr_uses_var_as_index_base(ptr, name),
        HirLValue::Var(_) => false,
    }
}

fn expr_uses_var_as_index_base(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Index { base, index, .. } => {
            matches!(base.as_ref(), HirExpr::Var(var) if var == name)
                || expr_uses_var_as_index_base(base, name)
                || expr_uses_var_as_index_base(index, name)
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_uses_var_as_index_base(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_var_as_index_base(lhs, name) || expr_uses_var_as_index_base(rhs, name)
        }
        HirExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_uses_var_as_index_base(arg, name)),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var_as_index_base(cond, name)
                || expr_uses_var_as_index_base(then_expr, name)
                || expr_uses_var_as_index_base(else_expr, name)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

pub(crate) fn inline_loop_condition_trailing_temps(
    func: &mut HirFunction,
    _preserved_temps: &HashSet<String>,
) -> bool {
    let mut changed = false;
    for _ in 0..8 {
        let use_count = DefUseMap::build(&func.body).use_count;
        if !inline_loop_condition_trailing_temps_in_stmts(&mut func.body, &use_count) {
            break;
        }
        changed = true;
    }
    changed
}

fn inline_loop_condition_trailing_temps_in_stmts(
    stmts: &mut Vec<HirStmt>,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            HirStmt::DoWhile { body, cond } => {
                changed |= inline_trailing_temps_into_condition(body, cond, read_counts);
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::While { body, .. } | HirStmt::Block(body) => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init
                    && let HirStmt::Block(body) = init.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
                }
                if let Some(update) = update
                    && let HirStmt::Block(body) = update.as_mut()
                {
                    changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(body, read_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_loop_condition_trailing_temps_in_stmts(then_body, read_counts);
                changed |= inline_loop_condition_trailing_temps_in_stmts(else_body, read_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        inline_loop_condition_trailing_temps_in_stmts(&mut case.body, read_counts);
                }
                changed |= inline_loop_condition_trailing_temps_in_stmts(default, read_counts);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
}

fn inline_trailing_temps_into_condition(
    body: &mut Vec<HirStmt>,
    cond: &mut HirExpr,
    read_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    loop {
        let Some(HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        }) = body.last()
        else {
            break;
        };
        if !is_trivial_temp_name(name)
            || expr_has_side_effects(rhs)
            || !expr_is_low_cost_inline_candidate(rhs)
            || expr_mentions_var(rhs, name)
        {
            break;
        }
        let cond_uses = count_var_uses(cond, name);
        if cond_uses == 0 || read_counts.get(name).copied().unwrap_or(0) != cond_uses {
            break;
        }
        let replacement = rhs.clone();
        replace_var_in_expr(cond, name, &replacement);
        body.pop();
        changed = true;
    }
    changed
}

pub(crate) fn simplify_empty_and_constant_ifs(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for stmt in stmts.drain(..) {
        match stmt {
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let constant = match cond {
                    HirExpr::Const(value, _) => Some(value != 0),
                    _ => None,
                };

                if let Some(trueish) = constant {
                    changed = true;
                    rewritten.extend(if trueish { then_body } else { else_body });
                    continue;
                }

                if then_body.is_empty() && else_body.is_empty() {
                    changed = true;
                    if expr_has_side_effects(&cond) {
                        rewritten.push(HirStmt::Expr(cond));
                    }
                    continue;
                }

                if then_body.is_empty() && !else_body.is_empty() {
                    changed = true;
                    rewritten.push(HirStmt::If {
                        cond: negate_expr(cond),
                        then_body: else_body,
                        else_body: Vec::new(),
                    });
                    continue;
                }

                rewritten.push(HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                });
            }
            other => rewritten.push(other),
        }
    }

    if changed {
        *stmts = rewritten;
    } else {
        *stmts = rewritten;
    }
    changed
}

pub(crate) fn simplify_empty_and_constant_ifs_recursive(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= simplify_empty_and_constant_ifs_recursive(body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_mut()
                    && let HirStmt::Block(body) = init.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(body);
                }
                if let Some(update) = update.as_mut()
                    && let HirStmt::Block(body) = update.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(body);
                }
                changed |= simplify_empty_and_constant_ifs_recursive(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= simplify_empty_and_constant_ifs_recursive(then_body);
                changed |= simplify_empty_and_constant_ifs_recursive(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= simplify_empty_and_constant_ifs_recursive(&mut case.body);
                }
                changed |= simplify_empty_and_constant_ifs_recursive(default);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed |= simplify_empty_and_constant_ifs(stmts);
    let before_len = stmts.len();
    stmts.retain(|stmt| !matches!(stmt, HirStmt::Block(body) if body.is_empty()));
    changed | (stmts.len() != before_len)
}

pub(crate) fn collapse_redundant_conditional_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some(HirStmt::If {
            cond,
            then_body,
            else_body,
        }) = stmts.get(idx)
        else {
            rewritten.push(stmts[idx].clone());
            idx += 1;
            continue;
        };

        let then_ret = single_return_stmt(then_body);
        let else_ret = single_return_stmt(else_body);

        // if (cond) return X; else return X;  ==>  [cond side effects]; return X;
        if let (Some(then_ret), Some(else_ret)) = (then_ret.clone(), else_ret.clone())
            && then_ret == else_ret
        {
            changed = true;
            if expr_has_side_effects(cond) {
                rewritten.push(HirStmt::Expr(cond.clone()));
            }
            rewritten.push(then_ret);
            idx += 1;
            continue;
        }

        // if (cond) return X; return X;  ==>  [cond side effects]; return X;
        // if (cond) {} else return X; return X;  ==>  [cond side effects]; return X;
        if let Some(next_ret) = stmts.get(idx + 1).and_then(as_return_stmt) {
            let then_matches_next =
                then_ret.as_ref().is_some_and(|ret| ret == next_ret) && else_body.is_empty();
            let else_matches_next =
                else_ret.as_ref().is_some_and(|ret| ret == next_ret) && then_body.is_empty();
            if then_matches_next || else_matches_next {
                changed = true;
                if expr_has_side_effects(cond) {
                    rewritten.push(HirStmt::Expr(cond.clone()));
                }
                idx += 1;
                continue;
            }
        }

        rewritten.push(stmts[idx].clone());
        idx += 1;
    }

    if changed {
        *stmts = rewritten;
    }
    changed
}

pub(crate) fn canonicalize_minmax_conditional_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx + 1 < stmts.len() {
        let Some((cond, then_body, else_body)) = if_parts(&stmts[idx]) else {
            idx += 1;
            continue;
        };
        if !else_body.is_empty() {
            idx += 1;
            continue;
        }
        let Some(then_expr) = single_return_expr(then_body) else {
            idx += 1;
            continue;
        };
        let Some(next_expr) = return_expr(&stmts[idx + 1]) else {
            idx += 1;
            continue;
        };
        let Some((op, lhs, rhs, ty)) = binary_comparison_parts(cond) else {
            idx += 1;
            continue;
        };
        if expr_has_side_effects(lhs) || expr_has_side_effects(rhs) {
            idx += 1;
            continue;
        }

        let Some(new_op) = minmax_branch_swap_op(op) else {
            idx += 1;
            continue;
        };
        if then_expr != rhs.as_ref() || next_expr != lhs.as_ref() {
            idx += 1;
            continue;
        }
        let lhs_expr = (**lhs).clone();
        let rhs_expr = (**rhs).clone();
        let cond_ty = ty.clone();

        stmts[idx] = HirStmt::If {
            cond: HirExpr::Binary {
                op: new_op,
                lhs: Box::new(lhs_expr.clone()),
                rhs: Box::new(rhs_expr.clone()),
                ty: cond_ty,
            },
            then_body: vec![HirStmt::Return(Some(lhs_expr))],
            else_body: Vec::new(),
        };
        stmts[idx + 1] = HirStmt::Return(Some(rhs_expr));
        changed = true;
        idx += 2;
    }

    changed
}

pub(crate) fn simplify_fallthrough_edges(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for idx in 0..stmts.len() {
        let stmt = stmts[idx].clone();
        let next_label = next_adjacent_label_name(stmts, idx + 1);
        match stmt {
            HirStmt::Goto(label) if next_label.as_deref() == Some(label.as_str()) => {
                changed = true;
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } if next_label
                .as_deref()
                .is_some_and(|label| matches_single_goto(&then_body, label))
                && else_body.is_empty() =>
            {
                changed = true;
                if expr_has_side_effects(&cond) {
                    rewritten.push(HirStmt::Expr(cond));
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } if next_label
                .as_deref()
                .is_some_and(|label| matches_single_goto(&else_body, label))
                && then_body.is_empty() =>
            {
                changed = true;
                if expr_has_side_effects(&cond) {
                    rewritten.push(HirStmt::Expr(cond));
                }
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let then_target = single_goto_target(&then_body);
                let else_target = single_goto_target(&else_body);

                match (next_label.as_deref(), then_target, else_target) {
                    (Some(next), Some(then_target), Some(else_target))
                        if then_target == else_target && then_target == next =>
                    {
                        changed = true;
                        if expr_has_side_effects(&cond) {
                            rewritten.push(HirStmt::Expr(cond));
                        }
                    }
                    (Some(_next), Some(then_target), Some(else_target))
                        if then_target == else_target =>
                    {
                        changed = true;
                        if expr_has_side_effects(&cond) {
                            rewritten.push(HirStmt::Expr(cond));
                        }
                        rewritten.push(HirStmt::Goto(then_target.to_string()));
                    }
                    (Some(next), Some(then_target), Some(else_target)) if then_target == next => {
                        changed = true;
                        rewritten.push(HirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![HirStmt::Goto(else_target.to_string())],
                            else_body: Vec::new(),
                        });
                    }
                    (Some(next), Some(then_target), Some(else_target)) if else_target == next => {
                        changed = true;
                        rewritten.push(HirStmt::If {
                            cond,
                            then_body: vec![HirStmt::Goto(then_target.to_string())],
                            else_body: Vec::new(),
                        });
                    }
                    _ => rewritten.push(HirStmt::If {
                        cond,
                        then_body,
                        else_body,
                    }),
                }
            }
            other => rewritten.push(other),
        }
    }

    *stmts = rewritten;
    changed
}

pub(crate) fn fuse_single_predecessor_boundaries(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        let Some((label_idx, label_name)) = next_label_index_and_name(stmts, idx + 1) else {
            idx += 1;
            continue;
        };
        let fused_segment = stmts[idx + 1..label_idx].to_vec();
        if fused_segment.is_empty() || !stmts_are_fuseable_linear_segment(&fused_segment) {
            idx += 1;
            continue;
        }

        let replacement = match &stmts[idx] {
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } if matches_single_goto(then_body, &label_name) && else_body.is_empty() => {
                Some(HirStmt::If {
                    cond: negate_expr(cond.clone()),
                    then_body: fused_segment.clone(),
                    else_body: Vec::new(),
                })
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } if then_body.is_empty() && matches_single_goto(else_body, &label_name) => {
                Some(HirStmt::If {
                    cond: cond.clone(),
                    then_body: fused_segment.clone(),
                    else_body: Vec::new(),
                })
            }
            _ => None,
        };

        let Some(replacement) = replacement else {
            idx += 1;
            continue;
        };

        stmts[idx] = replacement;
        stmts.drain(idx + 1..label_idx);
        changed = true;
        idx += 1;
    }
    changed
}

pub(crate) fn promote_guarded_jump_target_tail(stmts: &mut Vec<HirStmt>) -> bool {
    let referenced = collect_referenced_label_counts(stmts);
    let mut changed = false;
    let mut idx = 0usize;
    while idx + 3 < stmts.len() {
        let (
            HirStmt::If {
                cond: first_cond,
                then_body: first_then,
                else_body: first_else,
            },
            HirStmt::If {
                cond: second_cond,
                then_body: second_then,
                else_body: second_else,
            },
        ) = (&stmts[idx], &stmts[idx + 1])
        else {
            idx += 1;
            continue;
        };

        if !first_else.is_empty() || !second_else.is_empty() {
            idx += 1;
            continue;
        }
        let Some(body_label) = single_goto_target(first_then).map(str::to_string) else {
            idx += 1;
            continue;
        };
        let Some(join_label) = single_goto_target(second_then).map(str::to_string) else {
            idx += 1;
            continue;
        };
        if body_label == join_label {
            idx += 1;
            continue;
        }
        if !matches!(stmts.get(idx + 2), Some(HirStmt::Label(label)) if label == &body_label) {
            idx += 1;
            continue;
        }
        let Some((join_idx, _)) =
            next_label_index_and_name(stmts, idx + 3).filter(|(_, label)| label == &join_label)
        else {
            idx += 1;
            continue;
        };
        let body_segment = stmts[idx + 3..join_idx].to_vec();
        if body_segment.is_empty() || !stmts_are_fuseable_linear_segment(&body_segment) {
            idx += 1;
            continue;
        }
        if referenced.get(&body_label).copied().unwrap_or(0) > 1
            || referenced.get(&join_label).copied().unwrap_or(0) > 1
        {
            idx += 1;
            continue;
        }

        let combined_cond = fold_logical_chain(
            vec![first_cond.clone(), negate_expr(second_cond.clone())],
            HirBinaryOp::LogicalOr,
        );
        stmts[idx] = HirStmt::If {
            cond: combined_cond,
            then_body: body_segment,
            else_body: Vec::new(),
        };
        stmts.drain(idx + 1..=join_idx);
        changed = true;
        idx += 1;
    }
    changed
}

pub(crate) fn cleanup_redundant_boundary_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let original = stmts.clone();
    let cleaned = cleanup_redundant_labels(std::mem::take(stmts));
    let changed = cleaned != original;
    *stmts = cleaned;
    changed
}

pub(crate) fn remove_unreferenced_leading_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let referenced = collect_referenced_labels(stmts);
    let mut changed = false;
    while matches!(stmts.first(), Some(HirStmt::Label(label)) if !referenced.contains(label))
        && !should_preserve_unreferenced_leading_labels(stmts)
    {
        stmts.remove(0);
        changed = true;
    }
    changed
}

fn should_preserve_unreferenced_leading_labels(stmts: &[HirStmt]) -> bool {
    let first_non_label = stmts
        .iter()
        .position(|stmt| !matches!(stmt, HirStmt::Label(_)));
    match first_non_label {
        None => true,
        Some(idx) => matches!(stmts.get(idx..), Some([HirStmt::Return(_)])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn preserved_temp_binding(name: &str, bits: u32) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: int(bits),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        }
    }

    #[test]
    fn recursive_empty_if_cleanup_prunes_nested_pure_empty_guard() {
        let mut stmts = vec![HirStmt::Block(vec![HirStmt::If {
            cond: HirExpr::Var("xVar12".to_string()),
            then_body: Vec::new(),
            else_body: Vec::new(),
        }])];

        assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts));
        assert!(stmts.is_empty());
    }

    #[test]
    fn recursive_empty_if_cleanup_preserves_side_effectful_empty_guard() {
        let mut stmts = vec![HirStmt::If {
            cond: HirExpr::Call {
                target: "unknown_predicate".to_string(),
                args: Vec::new(),
                ty: NirType::Bool,
            },
            then_body: Vec::new(),
            else_body: Vec::new(),
        }];

        assert!(simplify_empty_and_constant_ifs_recursive(&mut stmts));
        assert!(matches!(
            &stmts[..],
            [HirStmt::Expr(HirExpr::Call { target, .. })] if target == "unknown_predicate"
        ));
    }

    #[test]
    fn collapse_trivial_assign_returns_skips_preserved_temp() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar0".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(HirExpr::Var("eax".to_string())),
                    rhs: Box::new(HirExpr::Var("ecx".to_string())),
                    ty: int(32),
                },
            },
            HirStmt::Return(Some(HirExpr::Var("uVar0".to_string()))),
        ];

        assert!(!collapse_trivial_assign_returns(
            &mut stmts,
            &HashSet::from([String::from("uVar0")]),
        ));
        assert!(matches!(stmts[0], HirStmt::Assign { .. }));
        assert!(matches!(stmts[1], HirStmt::Return(Some(HirExpr::Var(_)))));
    }

    #[test]
    fn collapse_loop_exit_alias_return_rewrites_do_while_exit_copy() {
        let mut stmts = vec![
            HirStmt::DoWhile {
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("sum".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("sum".to_string())),
                            rhs: Box::new(HirExpr::Var("value".to_string())),
                            ty: int(32),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("exit_sum".to_string()),
                        rhs: HirExpr::Var("sum".to_string()),
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("ptr".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("ptr".to_string())),
                            rhs: Box::new(HirExpr::Const(1, int(64))),
                            ty: int(64),
                        },
                    },
                ],
                cond: HirExpr::Var("keep_going".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
        ];

        assert!(collapse_loop_exit_alias_returns(&mut stmts));
        let HirStmt::DoWhile { body, .. } = &stmts[0] else {
            panic!("expected do/while");
        };
        assert!(!body.iter().any(|stmt| matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if name == "exit_sum"
        )));
        assert!(matches!(
            &stmts[1],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "sum"
        ));
    }

    #[test]
    fn collapse_loop_exit_alias_return_rejects_rhs_mutated_after_copy() {
        let mut stmts = vec![
            HirStmt::DoWhile {
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("exit_sum".to_string()),
                        rhs: HirExpr::Var("sum".to_string()),
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("sum".to_string()),
                        rhs: HirExpr::Const(0, int(32)),
                    },
                ],
                cond: HirExpr::Var("keep_going".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
        ];

        assert!(!collapse_loop_exit_alias_returns(&mut stmts));
    }

    #[test]
    fn eliminate_redundant_var_assigns_removes_adjacent_duplicate_assign() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar84".to_string()),
                rhs: HirExpr::Const(0, int(64)),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar84".to_string()),
                rhs: HirExpr::Const(0, int(64)),
            },
            HirStmt::Return(Some(HirExpr::Var("uVar84".to_string()))),
        ];

        assert!(eliminate_redundant_var_assigns(&mut stmts));
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            &stmts[0],
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Const(0, _),
            } if name == "uVar84"
        ));
    }

    #[test]
    fn eliminate_redundant_var_assigns_keeps_self_dependent_duplicate() {
        let rhs = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("sum".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("sum".to_string()),
                rhs: rhs.clone(),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("sum".to_string()),
                rhs,
            },
        ];

        assert!(!eliminate_redundant_var_assigns(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn eliminate_redundant_var_assigns_removes_exact_self_assign() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar29".to_string()),
                rhs: HirExpr::Var("xVar29".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("xVar29".to_string()))),
        ];

        assert!(eliminate_redundant_var_assigns(&mut stmts));
        assert_eq!(
            stmts,
            vec![HirStmt::Return(Some(HirExpr::Var("xVar29".to_string())))]
        );
    }

    #[test]
    fn cast_elision_rewrites_self_widening_assignment_to_self_assign() {
        let mut func = HirFunction {
            name: "test_self_widening_cast".to_string(),
            locals: vec![NirBinding {
                name: "uVar84".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar84".to_string()),
                    rhs: HirExpr::Cast {
                        ty: int(64),
                        expr: Box::new(HirExpr::Var("uVar84".to_string())),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("uVar84".to_string()))),
            ],
            ..Default::default()
        };

        assert!(cast_elision_pass(&mut func));
        assert_eq!(
            func.body[0],
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar84".to_string()),
                rhs: HirExpr::Var("uVar84".to_string()),
            }
        );
    }

    #[test]
    fn cast_elision_keeps_self_narrowing_assignment_to_wide_binding() {
        let mut func = HirFunction {
            name: "test_self_narrowing_cast".to_string(),
            locals: vec![NirBinding {
                name: "xVar29".to_string(),
                ty: int(64),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("xVar29".to_string()),
                rhs: HirExpr::Cast {
                    ty: int(32),
                    expr: Box::new(HirExpr::Var("xVar29".to_string())),
                },
            }],
            ..Default::default()
        };

        assert!(!cast_elision_pass(&mut func));
    }

    #[test]
    fn collapse_loop_exit_alias_return_rewrites_guarded_for_exit_copy() {
        let mut stmts = vec![
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLe,
                    lhs: Box::new(HirExpr::Var("len".to_string())),
                    rhs: Box::new(HirExpr::Const(0, int(32))),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Goto("exit_zero".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("sum".to_string()),
                rhs: HirExpr::Const(0, int(32)),
            },
            HirStmt::For {
                init: Some(Box::new(HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: HirExpr::Const(0, int(64)),
                })),
                cond: Some(HirExpr::Binary {
                    op: HirBinaryOp::SLt,
                    lhs: Box::new(HirExpr::Var("i".to_string())),
                    rhs: Box::new(HirExpr::Cast {
                        ty: int(64),
                        expr: Box::new(HirExpr::Var("len".to_string())),
                    }),
                    ty: NirType::Bool,
                }),
                update: Some(Box::new(HirStmt::Assign {
                    lhs: HirLValue::Var("i".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("i".to_string())),
                        rhs: Box::new(HirExpr::Const(1, int(64))),
                        ty: int(64),
                    },
                })),
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("sum".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("sum".to_string())),
                            rhs: Box::new(HirExpr::Var("value".to_string())),
                            ty: int(32),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("exit_sum".to_string()),
                        rhs: HirExpr::Var("sum".to_string()),
                    },
                ],
            },
            HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
            HirStmt::Label("exit_zero".to_string()),
            HirStmt::Return(Some(HirExpr::Const(0, int(32)))),
        ];

        assert!(collapse_loop_exit_alias_returns(&mut stmts));
        let HirStmt::For { body, .. } = &stmts[2] else {
            panic!("expected for loop");
        };
        assert!(!body.iter().any(|stmt| matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if name == "exit_sum"
        )));
        assert!(matches!(
            &stmts[3],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "sum"
        ));
    }

    #[test]
    fn collapse_loop_exit_alias_return_rejects_non_alias_expression() {
        let mut stmts = vec![
            HirStmt::DoWhile {
                body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("exit_sum".to_string()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("sum".to_string())),
                        rhs: Box::new(HirExpr::Var("value".to_string())),
                        ty: int(32),
                    },
                }],
                cond: HirExpr::Var("keep_going".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("exit_sum".to_string()))),
        ];

        assert!(!collapse_loop_exit_alias_returns(&mut stmts));
    }

    #[test]
    fn eliminate_dead_temp_assigns_removes_dead_preserved_temp() {
        let mut stmts = vec![HirStmt::Assign {
            lhs: HirLValue::Var("uVar0".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("eax".to_string())),
                rhs: Box::new(HirExpr::Const(1, int(32))),
                ty: int(32),
            },
        }];

        assert!(eliminate_dead_temp_assigns(
            &mut stmts,
            &HashSet::from([String::from("uVar0")]),
        ));
        assert!(stmts.is_empty());
    }

    #[test]
    fn prune_unreachable_after_return_stops_at_label_boundary() {
        let mut stmts = vec![
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            HirStmt::Assign {
                lhs: HirLValue::Var("dead".to_string()),
                rhs: HirExpr::Const(1, int(32)),
            },
            HirStmt::Goto("kept".to_string()),
            HirStmt::Label("kept".to_string()),
            HirStmt::Return(None),
        ];

        assert!(prune_unreachable_after_terminal(&mut stmts));
        assert_eq!(
            stmts,
            vec![
                HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
                HirStmt::Label("kept".to_string()),
                HirStmt::Return(None),
            ]
        );
    }

    #[test]
    fn prune_unused_temp_bindings_removes_dead_preserved_temp() {
        let mut func = HirFunction {
            name: "test_preserved_prune".to_string(),
            params: vec![],
            locals: vec![preserved_temp_binding("uVar0", 32)],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32))))],
            ..Default::default()
        };

        assert!(prune_unused_temp_bindings(&mut func));
        assert!(func.locals.is_empty());
    }

    #[test]
    fn prune_unused_temp_bindings_keeps_side_effect_assignment_target() {
        let mut func = HirFunction {
            name: "test_side_effect_lhs_preserved".to_string(),
            params: vec![],
            locals: vec![preserved_temp_binding("xVar30", 64)],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Var("xVar30".to_string()),
                rhs: HirExpr::Call {
                    target: "__pcodeop_294".to_string(),
                    args: vec![],
                    ty: int(64),
                },
            }],
            ..Default::default()
        };

        assert!(!prune_unused_temp_bindings(&mut func));
        assert_eq!(func.locals.len(), 1);
        assert_eq!(func.locals[0].name, "xVar30");
    }

    #[test]
    fn inline_single_use_temps_does_not_cross_label_boundary() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar0".to_string()),
                rhs: HirExpr::Const(0, int(32)),
            },
            HirStmt::Label("loop_head".to_string()),
            HirStmt::If {
                cond: HirExpr::Var("xVar0".to_string()),
                then_body: vec![HirStmt::Goto("loop_head".to_string())],
                else_body: Vec::new(),
            },
        ];

        assert!(!inline_single_use_temps(&mut stmts, &HashSet::new()));
        assert!(matches!(
            &stmts[2],
            HirStmt::If {
                cond: HirExpr::Var(name),
                ..
            } if name == "xVar0"
        ));
    }

    #[test]
    fn inline_single_use_temps_keeps_same_linear_segment_inline() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar0".to_string()),
                rhs: HirExpr::Const(1, int(32)),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar1".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("xVar0".to_string())),
                    rhs: Box::new(HirExpr::Const(2, int(32))),
                    ty: int(32),
                },
            },
        ];

        assert!(inline_single_use_temps(&mut stmts, &HashSet::new()));
        assert_eq!(stmts.len(), 1);
        let HirStmt::Assign { rhs, .. } = &stmts[0] else {
            panic!("expected assignment");
        };
        assert!(!expr_contains_var(rhs, "xVar0"));
    }

    #[test]
    fn inline_single_use_temps_inlines_flag_intrinsic_into_predicate() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar0".to_string()),
                rhs: HirExpr::Call {
                    target: "__sborrow".to_string(),
                    args: vec![
                        HirExpr::Var("param_1".to_string()),
                        HirExpr::Const(1, int(32)),
                    ],
                    ty: NirType::Bool,
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("xVar0".to_string()),
                then_body: Vec::new(),
                else_body: Vec::new(),
            },
        ];

        assert!(inline_single_use_temps(&mut stmts, &HashSet::new()));
        assert_eq!(stmts.len(), 1);
        let HirStmt::If { cond, .. } = &stmts[0] else {
            panic!("expected if");
        };
        assert!(matches!(cond, HirExpr::Call { target, .. } if target == "__sborrow"));
    }

    #[test]
    fn inline_loop_condition_trailing_temps_substitutes_condition_chain() {
        let mut func = HirFunction {
            name: "test_loop_cond_inline".to_string(),
            params: vec![],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::DoWhile {
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("sum".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("sum".to_string())),
                            rhs: Box::new(HirExpr::Const(1, int(32))),
                            ty: int(32),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("xVar38".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Sub,
                            lhs: Box::new(HirExpr::Var("ptr".to_string())),
                            rhs: Box::new(HirExpr::Var("end".to_string())),
                            ty: int(64),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("xVar39".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Eq,
                            lhs: Box::new(HirExpr::Var("xVar38".to_string())),
                            rhs: Box::new(HirExpr::Const(0, int(64))),
                            ty: NirType::Bool,
                        },
                    },
                ],
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("xVar39".to_string())),
                    ty: NirType::Bool,
                },
            }],
            ..Default::default()
        };

        assert!(inline_loop_condition_trailing_temps(
            &mut func,
            &HashSet::new(),
        ));
        let HirStmt::DoWhile { body, cond } = &func.body[0] else {
            panic!("expected do-while");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(
            cond,
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr,
                ..
            } if matches!(
                expr.as_ref(),
                HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs,
                    ..
                } if matches!(
                    lhs.as_ref(),
                    HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        ..
                    }
                )
            )
        ));
    }

    #[test]
    fn inline_single_use_temps_keeps_unknown_call_out_of_predicate() {
        let mut stmts = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar0".to_string()),
                rhs: HirExpr::Call {
                    target: "unknown_helper".to_string(),
                    args: vec![HirExpr::Var("param_1".to_string())],
                    ty: int(32),
                },
            },
            HirStmt::If {
                cond: HirExpr::Var("xVar0".to_string()),
                then_body: Vec::new(),
                else_body: Vec::new(),
            },
        ];

        assert!(!inline_single_use_temps(&mut stmts, &HashSet::new()));
        assert_eq!(stmts.len(), 2);
    }
}

fn next_adjacent_label_name(stmts: &[HirStmt], start_idx: usize) -> Option<String> {
    for stmt in stmts.iter().skip(start_idx) {
        match stmt {
            HirStmt::Label(label) => return Some(label.clone()),
            _ => return None,
        }
    }
    None
}

fn next_label_index_and_name(stmts: &[HirStmt], start_idx: usize) -> Option<(usize, String)> {
    for (idx, stmt) in stmts.iter().enumerate().skip(start_idx) {
        if let HirStmt::Label(label) = stmt {
            return Some((idx, label.clone()));
        }
    }
    None
}

fn matches_single_goto(body: &[HirStmt], label: &str) -> bool {
    matches!(body, [HirStmt::Goto(target)] if target == label)
}

fn as_return_stmt(stmt: &HirStmt) -> Option<&HirStmt> {
    matches!(stmt, HirStmt::Return(_)).then_some(stmt)
}

fn return_expr(stmt: &HirStmt) -> Option<&HirExpr> {
    match stmt {
        HirStmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

fn single_return_stmt(body: &[HirStmt]) -> Option<HirStmt> {
    match body {
        [HirStmt::Return(expr)] => Some(HirStmt::Return(expr.clone())),
        _ => None,
    }
}

fn single_return_expr(body: &[HirStmt]) -> Option<&HirExpr> {
    match body {
        [HirStmt::Return(Some(expr))] => Some(expr),
        _ => None,
    }
}

fn if_parts(stmt: &HirStmt) -> Option<(&HirExpr, &[HirStmt], &[HirStmt])> {
    match stmt {
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => Some((cond, then_body, else_body)),
        _ => None,
    }
}

fn binary_comparison_parts(
    expr: &HirExpr,
) -> Option<(HirBinaryOp, &Box<HirExpr>, &Box<HirExpr>, &NirType)> {
    match expr {
        HirExpr::Binary {
            op:
                op @ (HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::Gt
                | HirBinaryOp::Ge
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe
                | HirBinaryOp::SGt
                | HirBinaryOp::SGe),
            lhs,
            rhs,
            ty,
        } => Some((*op, lhs, rhs, ty)),
        _ => None,
    }
}

fn minmax_branch_swap_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    match op {
        HirBinaryOp::Lt | HirBinaryOp::Le => Some(HirBinaryOp::Gt),
        HirBinaryOp::Gt | HirBinaryOp::Ge => Some(HirBinaryOp::Lt),
        HirBinaryOp::SLt | HirBinaryOp::SLe => Some(HirBinaryOp::SGt),
        HirBinaryOp::SGt | HirBinaryOp::SGe => Some(HirBinaryOp::SLt),
        _ => None,
    }
}

fn single_goto_target(body: &[HirStmt]) -> Option<&str> {
    match body {
        [HirStmt::Goto(target)] => Some(target.as_str()),
        _ => None,
    }
}

fn stmts_are_fuseable_linear_segment(stmts: &[HirStmt]) -> bool {
    stmts.iter().all(stmt_is_fuseable_linear)
}

fn stmt_is_fuseable_linear(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { .. } | HirStmt::Expr(_) | HirStmt::VaStart { .. } => true,
        HirStmt::Block(body) => stmts_are_fuseable_linear_segment(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmts_are_fuseable_linear_segment(then_body)
                && stmts_are_fuseable_linear_segment(else_body)
        }
        HirStmt::Switch { .. }
        | HirStmt::While { .. }
        | HirStmt::DoWhile { .. }
        | HirStmt::For { .. }
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn collect_referenced_labels(stmts: &[HirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in stmts {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

fn collect_referenced_label_counts(stmts: &[HirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in stmts {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

fn collect_stmt_referenced_labels(stmt: &HirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
            for stmt in else_body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        HirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn collect_stmt_referenced_label_counts(stmt: &HirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
            for stmt in else_body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        HirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

pub(crate) fn eliminate_dead_local_clobber_assigns(func: &mut HirFunction) -> bool {
    eliminate_dead_local_clobber_assigns_in_stmts(&mut func.body, &func.params, &func.locals)
}

pub(crate) fn prune_unused_temp_bindings(func: &mut HirFunction) -> bool {
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = count_uses_in_stmt_list(&func.body, &binding.name) > 0;
        let assigned_side_effect =
            stmt_list_assigns_var_from_side_effecting_expr(&func.body, &binding.name);
        let keep = should_keep_unused_temp_binding(
            is_trivial_temp_name(&binding.name),
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

fn retain_unmarked_stmts(stmts: &mut Vec<HirStmt>, to_remove: &[bool]) {
    let mut idx = 0usize;
    stmts.retain(|_| {
        let keep = !to_remove.get(idx).copied().unwrap_or(false);
        idx += 1;
        keep
    });
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
        // If the variable is not mentioned at all in this statement (neither
        // read nor redefined), we can skip past it — even if it is a loop,
        // switch, or block that would otherwise stop the scan.
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
        // uses > 0 but we cannot inline here (e.g., nested loop body).
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

fn is_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
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

fn is_low_cost_flag_intrinsic(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow")
}

fn is_dead_local_clobber_name(name: &str) -> bool {
    if name.starts_with("param_ffff")
        || name.starts_with("param_fff")
        || name.starts_with("param_ff")
    {
        return true;
    }
    let Some(hex) = name.strip_prefix("local_") else {
        return false;
    };
    u64::from_str_radix(hex, 16)
        .map(|offset| offset <= 0x0c)
        .unwrap_or(false)
}

fn count_var_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name)
        }
        HirStmt::Expr(expr) => count_var_uses(expr, name),
        HirStmt::Block(stmts) => stmts
            .iter()
            .map(|stmt| count_var_uses_in_stmt(stmt, name))
            .sum(),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses(expr, name)
                + cases
                    .iter()
                    .map(|case| {
                        case.body
                            .iter()
                            .map(|stmt| count_var_uses_in_stmt(stmt, name))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses(cond, name)
                + then_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::While { cond, body } => {
            count_var_uses(cond, name)
                + body
                    .iter()
                    .map(|stmt| count_var_uses_in_stmt(stmt, name))
                    .sum::<usize>()
        }
        HirStmt::DoWhile { body, cond } => {
            body.iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>()
                + count_var_uses(cond, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let mut total = 0;
            if let Some(i) = init {
                total += count_var_uses_in_stmt(i, name);
            }
            if let Some(c) = cond {
                total += count_var_uses(c, name);
            }
            if let Some(u) = update {
                total += count_var_uses_in_stmt(u, name);
            }
            total += body
                .iter()
                .map(|stmt| count_var_uses_in_stmt(stmt, name))
                .sum::<usize>();
            total
        }
        HirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        HirStmt::VaStart { va_list, .. } => count_var_uses(va_list, name),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => 0,
    }
}

fn expr_contains_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => var == name,
        HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, name) || expr_contains_var(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, name)),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, name) || expr_contains_var(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_var(cond, name)
                || expr_contains_var(then_expr, name)
                || expr_contains_var(else_expr, name)
        }
    }
}

fn count_uses_in_stmt_list(stmts: &[HirStmt], name: &str) -> usize {
    stmts
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, name))
        .sum()
}

fn count_uses_in_bindings(bindings: &[NirBinding], name: &str) -> usize {
    bindings
        .iter()
        .filter(|binding| binding.name != name)
        .filter_map(|binding| binding.initializer.as_ref())
        .map(|expr| count_var_uses(expr, name))
        .sum()
}

fn pointer_alias_replacement(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => Some(expr.clone()),
        HirExpr::Cast {
            ty: NirType::Ptr(_),
            expr,
        } => match expr.as_ref() {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => Some((**expr).clone()),
            _ => None,
        },
        _ => None,
    }
}

fn expr_mentions_var(expr: &HirExpr, name: &str) -> bool {
    count_var_uses(expr, name) > 0
}

fn var_is_assigned_in_stmts(stmts: &[HirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| var_is_assigned_in_stmt(stmt, name))
}

fn var_is_assigned_in_stmt(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => var_is_assigned_in_stmts(body, name),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => var_is_assigned_in_stmts(then_body, name) || var_is_assigned_in_stmts(else_body, name),
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| var_is_assigned_in_stmts(&case.body, name))
                || var_is_assigned_in_stmts(default, name)
        }
    }
}

fn count_var_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(_) => 0,
        HirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
    }
}

fn count_var_uses(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(var) | HirExpr::AddressOfGlobal(var) => usize::from(var == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. } => count_var_uses(expr, name),
        HirExpr::Unary { expr, .. } => count_var_uses(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        HirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        HirExpr::Load { ptr, .. } => count_var_uses(ptr, name),
        HirExpr::PtrOffset { base, .. } => count_var_uses(base, name),
        HirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        HirExpr::AggregateCopy { src, .. } => count_var_uses(src, name),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_uses(cond, name)
                + count_var_uses(then_expr, name)
                + count_var_uses(else_expr, name)
        }
    }
}

pub(crate) fn expr_has_side_effects(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_has_side_effects(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        HirExpr::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_side_effects(cond)
                || expr_has_side_effects(then_expr)
                || expr_has_side_effects(else_expr)
        }
        HirExpr::Call { target, args, .. } => {
            if is_pure_intrinsic_call(target) {
                args.iter().any(expr_has_side_effects)
            } else {
                true
            }
        }
    }
}

fn is_pure_intrinsic_call(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirStmt::VaStart { va_list, .. } => replace_var_in_expr(va_list, name, replacement),
        HirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for stmt in &mut case.body {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            for stmt in default {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in then_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            for stmt in else_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init_stmt) = init {
                replace_var_in_stmt(init_stmt, name, replacement);
            }
            if let Some(c) = cond {
                replace_var_in_expr(c, name, replacement);
            }
            if let Some(upd_stmt) = update {
                replace_var_in_stmt(upd_stmt, name, replacement);
            }
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
    }
}

fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
    match expr {
        HirExpr::Var(var) if var == name => *expr = replacement.clone(),
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Unary { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        HirExpr::Load { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirExpr::PtrOffset { base, .. } => replace_var_in_expr(base, name, replacement),
        HirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        HirExpr::AggregateCopy { src, .. } => replace_var_in_expr(src, name, replacement),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            replace_var_in_expr(cond, name, replacement);
            replace_var_in_expr(then_expr, name, replacement);
            replace_var_in_expr(else_expr, name, replacement);
        }
    }
}

// ── Cast elision pass ──────────────────────────────────────────────────────

/// Remove casts in assignment context that are redundant given the binding
/// type already established by type inference.
///
/// Two cases are handled:
///
/// 1. **Assignment-context cast**: `x = (T)expr` where `x.ty == T` and both
///    are known scalar types.  The binding declaration already carries the
///    type, so the explicit cast adds no information to the output.
///
/// 2. **Identity cast in expr context**: handled by `canonicalize_cast_expr`
///    in `arith.rs` (`expr_type(inner) == ty → inner`); we rely on that
///    existing rule and do not duplicate it here.
///
/// This pass is Ghidra's `option_hide_exts` / `CastStrategy::isExtensionCastImplied`
/// equivalent: it drops casts where the surrounding context already implies the
/// desired type.  It is purely syntactic — no semantic changes.
///
/// Returns `true` if any cast was removed.
pub(crate) fn cast_elision_pass(func: &mut HirFunction) -> bool {
    // Build a map of known binding types (locals + params).
    // We only operate on bindings with resolved, non-pointer, non-aggregate types
    // to avoid accidentally stripping semantically significant casts.
    let binding_types: std::collections::HashMap<String, NirType> = func
        .locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| is_scalar_non_unknown(&b.ty))
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect();

    let return_type = is_scalar_non_unknown(&func.return_type).then(|| func.return_type.clone());

    if binding_types.is_empty() && return_type.is_none() {
        return false;
    }

    let mut changed = false;
    elide_casts_in_stmts(
        &mut func.body,
        &binding_types,
        return_type.as_ref(),
        &mut changed,
    );
    changed
}

fn is_scalar_non_unknown(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

fn scalar_bit_width(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn redundant_self_cast_assignment(name: &str, rhs: &HirExpr, binding_ty: &NirType) -> bool {
    let HirExpr::Cast { ty: cast_ty, expr } = rhs else {
        return false;
    };
    let HirExpr::Var(var) = expr.as_ref() else {
        return false;
    };
    if var != name {
        return false;
    }
    let Some(binding_bits) = scalar_bit_width(binding_ty) else {
        return false;
    };
    let Some(cast_bits) = scalar_bit_width(cast_ty) else {
        return false;
    };
    cast_bits >= binding_bits
}

fn elide_casts_in_stmts(
    stmts: &mut Vec<HirStmt>,
    binding_types: &std::collections::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        elide_casts_in_stmt(stmt, binding_types, return_type, changed);
    }
}

fn elide_casts_in_stmt(
    stmt: &mut HirStmt,
    binding_types: &std::collections::HashMap<String, NirType>,
    return_type: Option<&NirType>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            // If the binding has a known scalar type, try to strip a redundant
            // outer cast whose target type matches the binding.
            if let Some(binding_ty) = binding_types.get(name.as_str()) {
                if redundant_self_cast_assignment(name, rhs, binding_ty) {
                    *rhs = HirExpr::Var(name.clone());
                    *changed = true;
                } else if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        HirStmt::Return(Some(expr)) => {
            if let Some(return_type) = return_type
                && let Some(stripped) = try_strip_return_outer_cast(expr, return_type)
            {
                *expr = stripped;
                *changed = true;
            }
        }
        HirStmt::Block(stmts) => elide_casts_in_stmts(stmts, binding_types, return_type, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            elide_casts_in_stmts(then_body, binding_types, return_type, changed);
            elide_casts_in_stmts(else_body, binding_types, return_type, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            elide_casts_in_stmts(body, binding_types, return_type, changed)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                elide_casts_in_stmt(i, binding_types, return_type, changed);
            }
            if let Some(u) = update {
                elide_casts_in_stmt(u, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(body, binding_types, return_type, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                elide_casts_in_stmts(&mut case.body, binding_types, return_type, changed);
            }
            elide_casts_in_stmts(default, binding_types, return_type, changed);
        }
        // Expr, Label, Goto, Break, Continue — not an implied cast context.
        _ => {}
    }
}

/// If `expr` is a `Cast { ty: cast_ty, inner }` where `cast_ty == binding_ty`,
/// return `*inner`.  Otherwise return `None`.
///
/// We only strip *direct* outer casts; nested casts like `(T)(U)x` where the
/// outer cast matches are NOT stripped because the inner cast may still be
/// needed.
fn try_strip_outer_cast(expr: &HirExpr, binding_ty: &NirType) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    if cast_ty != binding_ty {
        return None;
    }
    // Only strip when the inner expression's own type is compatible (same bit
    // width or narrower).  We do NOT strip a cast that widens the inner type
    // into a type that could lose information on the next read — but since we're
    // trusting the binding's declared type, this is safe as long as the inner
    // type is the same width or narrower than `binding_ty`.
    let inner_ty = expr_type(inner);
    let compatible = match (&inner_ty, binding_ty) {
        // Unknown inner type: safe to strip (the binding type is authoritative).
        (NirType::Unknown, _) => true,
        // Same type: identity cast — always safe.
        (a, b) if a == b => true,
        // Bool → any int: safe, Bool is stored as 0/1.
        (NirType::Bool, NirType::Int { .. }) => true,
        // Int → Int: safe when inner bits <= outer bits (widening or same).
        (
            NirType::Int {
                bits: inner_bits, ..
            },
            NirType::Int {
                bits: outer_bits, ..
            },
        ) => inner_bits <= outer_bits,
        _ => false,
    };
    if compatible {
        Some((**inner).clone())
    } else {
        None
    }
}

/// If `return_type` already declares the scalar conversion performed by a
/// top-level return cast, the cast is implied by C return semantics.
fn try_strip_return_outer_cast(expr: &HirExpr, return_type: &NirType) -> Option<HirExpr> {
    let HirExpr::Cast {
        ty: cast_ty,
        expr: inner,
    } = expr
    else {
        return None;
    };
    if cast_ty == return_type && is_scalar_non_unknown(cast_ty) {
        Some((**inner).clone())
    } else {
        None
    }
}

// ── Parity / popcount dead elimination ───────────────────────────────────────

/// Eliminate assignments whose RHS (transitively) only involves `__popcount`
/// intrinsic calls and whose assigned variable has zero rvalue uses in the
/// entire function body.
///
/// This is a fast-path complement to `defuse_dead_assignment_pass`: the regular
/// defuse pass only removes Temp-origin bindings, whereas this pass also removes
/// named-register bindings (like the `pf` parity flag variable and any
/// intermediate variables derived from it) when the RHS is provably pure and the
/// result is unused.
///
/// Returns `true` if any statement was removed.
pub(crate) fn elide_unused_popcount_assigns(func: &mut HirFunction) -> bool {
    use super::super::analysis::defuse::DefUseMap;

    // Build a whole-body use count map.
    let use_map = DefUseMap::build(&func.body);

    // Collect the names of all variables with a popcount-based RHS so we
    // can cascade: if `b = __popcount(x)` is dead, then `a = f(b)` may
    // also become dead in a subsequent iteration.
    let mut changed = false;
    // Iterate to convergence (at most a small number of rounds for cascades).
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
    use_map: &super::super::analysis::defuse::DefUseMap,
) -> bool {
    let mut changed = false;
    elide_popcount_in_stmts(&mut func.body, use_map, &mut changed);
    if changed {
        // Remove bindings for eliminated variables.
        let remaining_names: HashSet<String> = func
            .body
            .iter()
            .flat_map(|s| collect_assigned_names(s))
            .collect();
        func.locals.retain(|b| {
            // Keep bindings that still have assignments OR are used elsewhere.
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
    use_map: &super::super::analysis::defuse::DefUseMap,
    changed: &mut bool,
) {
    // Recurse first.
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
    // Remove flat-level dead popcount assignments.
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

/// Returns `true` if `expr` contains a `__popcount` call anywhere.
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

// ── Single-predecessor label inlining (goto reduction) ───────────────────────

/// Inline labels that are targeted by exactly one unconditional forward `goto`.
///
/// ## What this eliminates
///
/// After CFG structuring many residual goto+label pairs remain in the flat HIR
/// for edges the structurer could not fold into `if`/`while`/`for` constructs.
/// A common pattern is:
///
/// ```text
/// goto block_X;        ← unconditional forward jump
/// <unreachable stmts>  ← dead code between goto and label
/// block_X:             ← single-reference label
/// <body stmts>         ← the actual continuation
/// ```
///
/// Because `Goto` is unconditional, everything between the goto and the
/// single-reference label is *unreachable* — provided no other `goto` or
/// fall-through path into that dead segment exists.
///
/// This transformation removes the `Goto`, the unreachable segment, and the
/// `Label`, leaving the body stmts in-place as natural fall-through.
///
/// ## Safety invariants
///
/// 1. The label must have **exactly one** incoming `Goto` reference in the
///    entire function body (single-predecessor constraint).
/// 2. The `Label` must appear **after** the `Goto` in the linear order
///    (forward edge only — back-edges are loop headers and must not be removed).
/// 3. The unreachable segment between goto and label must contain **no labels**
///    that are referenced from outside that segment (to avoid removing code
///    that is otherwise reachable).
///
/// The pass operates on the *top-level* statement list.  Recursion into nested
/// `if`/`while`/`for` bodies is performed after the top-level pass.
pub(crate) fn single_pred_label_inline(stmts: &mut Vec<HirStmt>) -> bool {
    // First recurse into nested scopes so their gotos are cleaned up before
    // we look at the flat list.
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= single_pred_label_inline_in_stmt(stmt);
    }

    // Now process the top-level flat sequence.
    changed |= single_pred_label_inline_flat(stmts);
    changed
}

fn single_pred_label_inline_in_stmt(stmt: &mut HirStmt) -> bool {
    match stmt {
        HirStmt::Block(body) => single_pred_label_inline(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let a = single_pred_label_inline(then_body);
            let b = single_pred_label_inline(else_body);
            a || b
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            single_pred_label_inline(body)
        }
        HirStmt::For { body, .. } => single_pred_label_inline(body),
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                changed |= single_pred_label_inline(&mut case.body);
            }
            changed |= single_pred_label_inline(default);
            changed
        }
        _ => false,
    }
}

/// Core flat-list transformation: inline single-predecessor forward labels.
fn single_pred_label_inline_flat(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    // Each round removes at least one goto+label pair, so the loop terminates
    // in at most O(label_count) iterations.  We cap at 512 as a safety guard.
    for _ in 0..512 {
        // Build the global reference count for all labels.
        let ref_counts = collect_referenced_label_counts(stmts);

        let mut did_inline = false;
        let mut i = 0;
        while i < stmts.len() {
            // We need a top-level unconditional Goto.
            let goto_label = match &stmts[i] {
                HirStmt::Goto(label) => label.clone(),
                _ => {
                    i += 1;
                    continue;
                }
            };

            // Only process single-reference labels.
            if ref_counts.get(&goto_label).copied().unwrap_or(0) != 1 {
                i += 1;
                continue;
            }

            // Find Label("goto_label") somewhere AFTER position i (forward edge).
            let label_pos = stmts[i + 1..]
                .iter()
                .position(|s| matches!(s, HirStmt::Label(l) if l == &goto_label))
                .map(|offset| offset + i + 1);

            let Some(j) = label_pos else {
                // Label is before goto (back-edge) or not found — leave.
                i += 1;
                continue;
            };

            // Verify the segment [i+1..j) contains no labels that are
            // referenced from OUTSIDE that segment (otherwise those paths
            // remain reachable and removing the segment would be wrong).
            let segment = &stmts[i + 1..j];
            let segment_label_refs = collect_referenced_label_counts(segment);
            let external_ref_found = segment.iter().any(|s| {
                if let HirStmt::Label(l) = s {
                    let total_refs = ref_counts.get(l).copied().unwrap_or(0);
                    let internal_refs = segment_label_refs.get(l).copied().unwrap_or(0);
                    total_refs > internal_refs
                } else {
                    false
                }
            });

            if external_ref_found {
                i += 1;
                continue;
            }

            // Perform the inlining:
            // 1. Remove Label at position j.
            stmts.remove(j);
            // 2. Remove unreachable segment [i+1..j).
            if j > i + 1 {
                stmts.drain(i + 1..j);
            }
            // 3. Remove Goto at position i.
            stmts.remove(i);
            // Positions shifted — restart from i (don't increment).
            did_inline = true;
            changed = true;
        }

        if !did_inline {
            break;
        }
    }
    changed
}
