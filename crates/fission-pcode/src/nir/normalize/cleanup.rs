use super::*;
use crate::nir::structuring::cleanup_redundant_labels;
use std::collections::{HashMap, HashSet};

pub(super) fn collapse_trivial_assign_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
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
            ) if name == ret_name && is_trivial_temp_name(name) => Some(rhs.clone()),
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
    changed
}

pub(super) fn inline_single_use_temps(stmts: &mut Vec<HirStmt>) -> bool {
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

        let Some(target_idx) = find_inline_forward_target(stmts, idx, &name) else {
            idx += 1;
            continue;
        };
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

pub(super) fn eliminate_dead_temp_assigns(stmts: &mut Vec<HirStmt>) -> bool {
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

pub(super) fn simplify_empty_and_constant_ifs(stmts: &mut Vec<HirStmt>) -> bool {
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

pub(super) fn simplify_fallthrough_edges(stmts: &mut Vec<HirStmt>) -> bool {
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

pub(super) fn fuse_single_predecessor_boundaries(stmts: &mut Vec<HirStmt>) -> bool {
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

pub(super) fn promote_guarded_jump_target_tail(stmts: &mut Vec<HirStmt>) -> bool {
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

pub(super) fn cleanup_redundant_boundary_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let original = stmts.clone();
    let cleaned = cleanup_redundant_labels(std::mem::take(stmts));
    let changed = cleaned != original;
    *stmts = cleaned;
    changed
}

pub(super) fn remove_unreferenced_leading_labels(stmts: &mut Vec<HirStmt>) -> bool {
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
        HirStmt::Assign { .. } | HirStmt::Expr(_) => true,
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
        | HirStmt::Expr(_)
        | HirStmt::Label(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

pub(super) fn eliminate_dead_local_clobber_assigns(func: &mut HirFunction) -> bool {
    eliminate_dead_local_clobber_assigns_in_stmts(&mut func.body, &func.params, &func.locals)
}

pub(super) fn prune_unused_temp_bindings(func: &mut HirFunction) -> bool {
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = count_uses_in_stmt_list(&func.body, &binding.name) > 0;
        let keep = !is_trivial_temp_name(&binding.name)
            || used
            || binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects);
        changed |= !keep;
        keep
    });
    changed
}

pub(super) fn prune_unused_dead_local_bindings(func: &mut HirFunction) -> bool {
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

fn find_inline_forward_target(stmts: &[HirStmt], def_idx: usize, name: &str) -> Option<usize> {
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

fn is_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
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
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue => 0,
    }
}

fn count_uses_in_stmt_list(stmts: &[HirStmt], name: &str) -> usize {
    stmts
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, name))
        .sum()
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
        HirExpr::Var(var) => usize::from(var == name),
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
    }
}

pub(super) fn expr_has_side_effects(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::Const(_, _) => false,
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
    matches!(target, "__carry" | "__scarry" | "__sborrow")
}

fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
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
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
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
pub(super) fn cast_elision_pass(func: &mut HirFunction) -> bool {
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

    if binding_types.is_empty() {
        return false;
    }

    let mut changed = false;
    elide_casts_in_stmts(&mut func.body, &binding_types, &mut changed);
    changed
}

fn is_scalar_non_unknown(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

fn elide_casts_in_stmts(
    stmts: &mut Vec<HirStmt>,
    binding_types: &std::collections::HashMap<String, NirType>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        elide_casts_in_stmt(stmt, binding_types, changed);
    }
}

fn elide_casts_in_stmt(
    stmt: &mut HirStmt,
    binding_types: &std::collections::HashMap<String, NirType>,
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
                if let Some(stripped) = try_strip_outer_cast(rhs, binding_ty) {
                    *rhs = stripped;
                    *changed = true;
                }
            }
        }
        HirStmt::Block(stmts) => elide_casts_in_stmts(stmts, binding_types, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            elide_casts_in_stmts(then_body, binding_types, changed);
            elide_casts_in_stmts(else_body, binding_types, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            elide_casts_in_stmts(body, binding_types, changed)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                elide_casts_in_stmt(i, binding_types, changed);
            }
            if let Some(u) = update {
                elide_casts_in_stmt(u, binding_types, changed);
            }
            elide_casts_in_stmts(body, binding_types, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                elide_casts_in_stmts(&mut case.body, binding_types, changed);
            }
            elide_casts_in_stmts(default, binding_types, changed);
        }
        // Return, Expr, Label, Goto, Break, Continue — not assignment context.
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
    let HirExpr::Cast { ty: cast_ty, expr: inner } = expr else {
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
            NirType::Int { bits: inner_bits, .. },
            NirType::Int { bits: outer_bits, .. },
        ) => inner_bits <= outer_bits,
        _ => false,
    };
    if compatible {
        Some((**inner).clone())
    } else {
        None
    }
}
