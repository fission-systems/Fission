use super::super::*;
use super::utils::*;
use crate::nir::structuring::cleanup_redundant_labels;
use std::collections::{HashMap, HashSet};

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

fn single_goto_target(body: &[HirStmt]) -> Option<&str> {
    match body {
        [HirStmt::Goto(target)] => Some(target.as_str()),
        _ => None,
    }
}

pub(crate) fn collapse_common_exit_guard_chain(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some((exit_label, guard_count, conds)) = common_exit_guard_chain(stmts, idx) else {
            idx += 1;
            continue;
        };
        let Some(exit_idx) = stmts.iter().enumerate().skip(idx + guard_count).find_map(
            |(label_idx, stmt)| match stmt {
                HirStmt::Label(label) if label == &exit_label => Some(label_idx),
                _ => None,
            },
        ) else {
            idx += 1;
            continue;
        };
        if exit_idx == idx + guard_count {
            idx += 1;
            continue;
        }

        let guarded_body = stmts[idx + guard_count..exit_idx].to_vec();
        let exit_cond = simplify_logical_expr(fold_logical_chain(conds, HirBinaryOp::LogicalOr));
        stmts[idx] = HirStmt::If {
            cond: negate_expr(exit_cond),
            then_body: guarded_body,
            else_body: Vec::new(),
        };
        stmts.drain(idx + 1..exit_idx);
        changed = true;
        idx += 1;
    }

    changed
}

fn common_exit_guard_chain(
    stmts: &[HirStmt],
    start_idx: usize,
) -> Option<(String, usize, Vec<HirExpr>)> {
    let mut guard_count = 0usize;
    let mut exit_label: Option<String> = None;
    let mut conds = Vec::new();

    for stmt in stmts.iter().skip(start_idx) {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            break;
        };
        if !else_body.is_empty() {
            break;
        }
        let Some(target) = single_goto_target(then_body) else {
            break;
        };
        match exit_label.as_deref() {
            Some(label) if label != target => break,
            None => exit_label = Some(target.to_string()),
            _ => {}
        }
        guard_count += 1;
        conds.push(cond.clone());
    }

    Some((exit_label?, guard_count, conds))
        .filter(|(_, count, conds)| *count > 1 && !conds.is_empty())
}

pub(crate) fn cleanup_redundant_boundary_labels(
    stmts: &mut Vec<HirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    let original = stmts.clone();
    let cleaned = cleanup_redundant_labels(std::mem::take(stmts), global_refs);
    let changed = cleaned != original;
    *stmts = cleaned;
    changed
}

pub(crate) fn remove_unreferenced_leading_labels(
    stmts: &mut Vec<HirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    let local_refs = if global_refs.is_none() {
        Some(collect_referenced_labels(stmts))
    } else {
        None
    };
    let referenced = global_refs.unwrap_or_else(|| local_refs.as_ref().unwrap());
    let mut changed = false;
    while let Some(HirStmt::Label(label)) = stmts.first() {
        if !referenced.contains(label) && !should_preserve_unreferenced_leading_labels(stmts) {
            stmts.remove(0);
            changed = true;
        } else {
            break;
        }
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

pub(crate) fn single_pred_label_inline(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= single_pred_label_inline_in_stmt(stmt);
    }
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

fn single_pred_label_inline_flat(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for _ in 0..512 {
        let ref_counts = collect_referenced_label_counts(stmts);

        let mut did_inline = false;
        let mut i = 0;
        while i < stmts.len() {
            let goto_label = match &stmts[i] {
                HirStmt::Goto(label) => label.clone(),
                _ => {
                    i += 1;
                    continue;
                }
            };

            if ref_counts.get(&goto_label).copied().unwrap_or(0) != 1 {
                i += 1;
                continue;
            }

            let label_pos = stmts[i + 1..]
                .iter()
                .position(|s| matches!(s, HirStmt::Label(l) if l == &goto_label))
                .map(|offset| offset + i + 1);

            let Some(j) = label_pos else {
                i += 1;
                continue;
            };

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

            eprintln!(
                "[DEBUG-INLINE] inlining label={} at i={}, j={}, drained_count={}",
                goto_label,
                i,
                j,
                j - (i + 1)
            );
            stmts.remove(j);
            if j > i + 1 {
                stmts.drain(i + 1..j);
            }
            stmts.remove(i);
            did_inline = true;
            changed = true;
        }

        if !did_inline {
            break;
        }
    }
    changed
}
