use super::utils::{collect_referenced_labels, *};
use crate::pipeline::PROTECTED_LSDA_LABELS;
use crate::prelude::{
    HashMap, HashSet, DirBinaryOp, DirExpr, DirStmt, fold_logical_chain, negate_expr,
    simplify_logical_expr,
};
use fission_midend_dir::util::label_cleanup::cleanup_redundant_labels;

pub fn prune_unreachable_after_terminal(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut referenced_labels = collect_referenced_labels(stmts);
    PROTECTED_LSDA_LABELS.with(|protected| {
        referenced_labels.extend(protected.borrow().iter().cloned());
    });
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

fn is_unconditional_terminal(stmt: &DirStmt) -> bool {
    matches!(
        stmt,
        DirStmt::Return(_) | DirStmt::Goto(_) | DirStmt::Break | DirStmt::Continue
    )
}

fn stmt_contains_referenced_label(stmt: &DirStmt, referenced_labels: &HashSet<String>) -> bool {
    match stmt {
        DirStmt::Label(label) => referenced_labels.contains(label),
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => body
            .iter()
            .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels)),
        DirStmt::If {
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
        DirStmt::Switch { cases, default, .. } => {
            default
                .iter()
                .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                || cases.iter().any(|case| {
                    case.body
                        .iter()
                        .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                })
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Return(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

pub fn simplify_empty_and_constant_ifs(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for stmt in stmts.drain(..) {
        match stmt {
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let constant = match cond {
                    DirExpr::Const(value, _) => Some(value != 0),
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
                        rewritten.push(DirStmt::Expr(cond));
                    }
                    continue;
                }

                if then_body.is_empty() && !else_body.is_empty() {
                    changed = true;
                    rewritten.push(DirStmt::If {
                        cond: negate_expr(cond),
                        then_body: else_body,
                        else_body: Vec::new(),
                    });
                    continue;
                }

                rewritten.push(DirStmt::If {
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

pub fn simplify_empty_and_constant_ifs_recursive(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= simplify_empty_and_constant_ifs_recursive(body);
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_mut()
                    && let DirStmt::Block(body) = init.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(body);
                }
                if let Some(update) = update.as_mut()
                    && let DirStmt::Block(body) = update.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(body);
                }
                changed |= simplify_empty_and_constant_ifs_recursive(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= simplify_empty_and_constant_ifs_recursive(then_body);
                changed |= simplify_empty_and_constant_ifs_recursive(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= simplify_empty_and_constant_ifs_recursive(&mut case.body);
                }
                changed |= simplify_empty_and_constant_ifs_recursive(default);
            }
            DirStmt::Assign { .. }
            | DirStmt::VaStart { .. }
            | DirStmt::Expr(_)
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => {}
        }
    }
    changed |= simplify_empty_and_constant_ifs(stmts);
    let before_len = stmts.len();
    stmts.retain(|stmt| !matches!(stmt, DirStmt::Block(body) if body.is_empty()));
    changed | (stmts.len() != before_len)
}

pub fn simplify_fallthrough_edges(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for idx in 0..stmts.len() {
        let stmt = stmts[idx].clone();
        let next_label = next_adjacent_label_name(stmts, idx + 1);
        match stmt {
            DirStmt::Goto(label) if next_label.as_deref() == Some(label.as_str()) => {
                changed = true;
            }
            DirStmt::If {
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
                    rewritten.push(DirStmt::Expr(cond));
                }
            }
            DirStmt::If {
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
                    rewritten.push(DirStmt::Expr(cond));
                }
            }
            DirStmt::If {
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
                            rewritten.push(DirStmt::Expr(cond));
                        }
                    }
                    (Some(_next), Some(then_target), Some(else_target))
                        if then_target == else_target =>
                    {
                        changed = true;
                        if expr_has_side_effects(&cond) {
                            rewritten.push(DirStmt::Expr(cond));
                        }
                        rewritten.push(DirStmt::Goto(then_target.to_string()));
                    }
                    (Some(next), Some(then_target), Some(else_target)) if then_target == next => {
                        changed = true;
                        rewritten.push(DirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![DirStmt::Goto(else_target.to_string())],
                            else_body: Vec::new(),
                        });
                    }
                    (Some(next), Some(then_target), Some(else_target)) if else_target == next => {
                        changed = true;
                        rewritten.push(DirStmt::If {
                            cond,
                            then_body: vec![DirStmt::Goto(then_target.to_string())],
                            else_body: Vec::new(),
                        });
                    }
                    _ => rewritten.push(DirStmt::If {
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

fn next_adjacent_label_name(stmts: &[DirStmt], start_idx: usize) -> Option<String> {
    for stmt in stmts.iter().skip(start_idx) {
        match stmt {
            DirStmt::Label(label) => return Some(label.clone()),
            _ => return None,
        }
    }
    None
}

fn next_label_index_and_name(stmts: &[DirStmt], start_idx: usize) -> Option<(usize, String)> {
    for (idx, stmt) in stmts.iter().enumerate().skip(start_idx) {
        if let DirStmt::Label(label) = stmt {
            return Some((idx, label.clone()));
        }
    }
    None
}

fn matches_single_goto(body: &[DirStmt], label: &str) -> bool {
    matches!(body, [DirStmt::Goto(target)] if target == label)
}

pub fn fuse_single_predecessor_boundaries(stmts: &mut Vec<DirStmt>) -> bool {
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
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } if matches_single_goto(then_body, &label_name) && else_body.is_empty() => {
                Some(DirStmt::If {
                    cond: negate_expr(cond.clone()),
                    then_body: fused_segment.clone(),
                    else_body: Vec::new(),
                })
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } if then_body.is_empty() && matches_single_goto(else_body, &label_name) => {
                Some(DirStmt::If {
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

fn stmts_are_fuseable_linear_segment(stmts: &[DirStmt]) -> bool {
    stmts.iter().all(stmt_is_fuseable_linear)
}

fn stmt_is_fuseable_linear(stmt: &DirStmt) -> bool {
    match stmt {
        // Return is linear for if-goto inversion: the statements between
        // `if (c) goto L;` and `L:` may include early returns (saturating_add).
        DirStmt::Assign { .. }
        | DirStmt::Expr(_)
        | DirStmt::VaStart { .. }
        | DirStmt::Return(_) => true,
        DirStmt::Block(body) => stmts_are_fuseable_linear_segment(body),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmts_are_fuseable_linear_segment(then_body)
                && stmts_are_fuseable_linear_segment(else_body)
        }
        DirStmt::Switch { .. }
        | DirStmt::While { .. }
        | DirStmt::DoWhile { .. }
        | DirStmt::For { .. }
        | DirStmt::Label(_)
        | DirStmt::Goto(_)
        | DirStmt::Break
        | DirStmt::Continue => false,
    }
}

pub fn promote_guarded_jump_target_tail(stmts: &mut Vec<DirStmt>) -> bool {
    let referenced = collect_referenced_label_counts(stmts);
    let mut changed = false;
    let mut idx = 0usize;
    while idx + 3 < stmts.len() {
        let (
            DirStmt::If {
                cond: first_cond,
                then_body: first_then,
                else_body: first_else,
            },
            DirStmt::If {
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
        if !matches!(stmts.get(idx + 2), Some(DirStmt::Label(label)) if label == &body_label) {
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
            DirBinaryOp::LogicalOr,
        );
        stmts[idx] = DirStmt::If {
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

fn single_goto_target(body: &[DirStmt]) -> Option<&str> {
    match body {
        [DirStmt::Goto(target)] => Some(target.as_str()),
        _ => None,
    }
}

pub fn collapse_common_exit_guard_chain(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some((exit_label, guard_count, conds)) = common_exit_guard_chain(stmts, idx) else {
            idx += 1;
            continue;
        };
        let Some(exit_idx) = stmts.iter().enumerate().skip(idx + guard_count).find_map(
            |(label_idx, stmt)| match stmt {
                DirStmt::Label(label) if label == &exit_label => Some(label_idx),
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
        let exit_cond = simplify_logical_expr(fold_logical_chain(conds, DirBinaryOp::LogicalOr));
        stmts[idx] = DirStmt::If {
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
    stmts: &[DirStmt],
    start_idx: usize,
) -> Option<(String, usize, Vec<DirExpr>)> {
    let mut guard_count = 0usize;
    let mut exit_label: Option<String> = None;
    let mut conds = Vec::new();

    for stmt in stmts.iter().skip(start_idx) {
        let DirStmt::If {
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

pub fn cleanup_redundant_boundary_labels(
    stmts: &mut Vec<DirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    // `cleanup_redundant_labels` lives in fission-midend-core and takes
    // std's RandomState-hashed HashSet -- a third crate boundary this
    // pipeline's FxBuildHasher alias doesn't reach. Converting only at
    // this narrow call site (rather than propagating std::collections::
    // HashSet through global_refs/active_refs everywhere) keeps the rest
    // of this crate's own label bookkeeping on the deterministic hasher.
    let std_refs: Option<std::collections::HashSet<String>> =
        global_refs.map(|refs| refs.iter().cloned().collect());
    let original = stmts.clone();
    let cleaned = cleanup_redundant_labels(std::mem::take(stmts), std_refs.as_ref());
    let changed = cleaned != original;
    *stmts = cleaned;
    changed
}

pub fn remove_unreferenced_leading_labels(
    stmts: &mut Vec<DirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    let local_refs = if global_refs.is_none() {
        Some(collect_referenced_labels(stmts))
    } else {
        None
    };
    let referenced = global_refs.unwrap_or_else(|| local_refs.as_ref().unwrap());
    let mut changed = false;
    while let Some(DirStmt::Label(label)) = stmts.first() {
        if !referenced.contains(label) && !should_preserve_unreferenced_leading_labels(stmts) {
            stmts.remove(0);
            changed = true;
        } else {
            break;
        }
    }
    changed
}

fn should_preserve_unreferenced_leading_labels(stmts: &[DirStmt]) -> bool {
    let first_non_label = stmts
        .iter()
        .position(|stmt| !matches!(stmt, DirStmt::Label(_)));
    match first_non_label {
        None => true,
        Some(idx) => matches!(stmts.get(idx..), Some([DirStmt::Return(_)])),
    }
}

pub fn single_pred_label_inline(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= single_pred_label_inline_in_stmt(stmt);
    }
    changed |= single_pred_label_inline_flat(stmts);
    changed
}

fn single_pred_label_inline_in_stmt(stmt: &mut DirStmt) -> bool {
    match stmt {
        DirStmt::Block(body) => single_pred_label_inline(body),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let a = single_pred_label_inline(then_body);
            let b = single_pred_label_inline(else_body);
            a || b
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            single_pred_label_inline(body)
        }
        DirStmt::For { body, .. } => single_pred_label_inline(body),
        DirStmt::Switch { cases, default, .. } => {
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

fn single_pred_label_inline_flat(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    for _ in 0..512 {
        let ref_counts = collect_referenced_label_counts(stmts);

        let mut did_inline = false;
        let mut i = 0;
        while i < stmts.len() {
            let goto_label = match &stmts[i] {
                DirStmt::Goto(label) => label.clone(),
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
                .position(|s| matches!(s, DirStmt::Label(l) if l == &goto_label))
                .map(|offset| offset + i + 1);

            let Some(j) = label_pos else {
                i += 1;
                continue;
            };

            let segment = &stmts[i + 1..j];
            let segment_label_refs = collect_referenced_label_counts(segment);
            // A label with zero textual Goto references anywhere still must
            // not be silently drained here if it's an LSDA landing pad (see
            // PROTECTED_LSDA_LABELS) -- it's a real entry point the
            // personality routine unwinds into at runtime, just one
            // `ref_counts`/`collect_referenced_label_counts` (both purely
            // Goto-based) have no way to see.
            let external_ref_found = segment.iter().any(|s| {
                if let DirStmt::Label(l) = s {
                    if PROTECTED_LSDA_LABELS.with(|protected| protected.borrow().contains(l)) {
                        return true;
                    }
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
