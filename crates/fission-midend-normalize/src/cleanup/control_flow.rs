use super::utils::{collect_referenced_labels, *};
use crate::pipeline::PROTECTED_LSDA_LABELS;
use crate::prelude::{
    HashMap, HashSet, PreHirBinaryOp, PreHirExpr, PreHirStmt, fold_logical_chain, negate_expr,
    simplify_logical_expr,
};
use fission_midend_prehir::util::label_cleanup::cleanup_redundant_labels;

/// `global_refs`, when given, is the whole-function referenced-label set
/// (as opposed to `stmts`-local `collect_referenced_labels`, which only sees
/// `Goto`s inside this nested body). A label whose sole reference is a
/// `Goto` from a *different* scope -- e.g. the target of a `switch` case
/// elsewhere in the function -- is not dead just because it looks
/// unreferenced from here; without `global_refs` this pruned a label that
/// was still a live cross-scope jump target, leaving the other scope's
/// `Goto` dangling with no matching label anywhere in the rendered output.
pub fn prune_unreachable_after_terminal(
    stmts: &mut Vec<PreHirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    let mut changed = false;
    let mut referenced_labels = collect_referenced_labels(stmts);
    if let Some(refs) = global_refs {
        referenced_labels.extend(refs.iter().cloned());
    }
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

fn is_unconditional_terminal(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue
    )
}

fn stmt_contains_referenced_label(stmt: &PreHirStmt, referenced_labels: &HashSet<String>) -> bool {
    match stmt {
        PreHirStmt::Label(label) => referenced_labels.contains(label),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => body
            .iter()
            .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels)),
        PreHirStmt::If {
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
        PreHirStmt::Switch { cases, default, .. } => {
            default
                .iter()
                .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                || cases.iter().any(|case| {
                    case.body
                        .iter()
                        .any(|stmt| stmt_contains_referenced_label(stmt, referenced_labels))
                })
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn simplify_empty_and_constant_ifs(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for stmt in stmts.drain(..) {
        match stmt {
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let constant = match cond {
                    PreHirExpr::Const(value, _) => Some(value != 0),
                    _ => None,
                };

                if let Some(trueish) = constant {
                    changed = true;
                    rewritten.extend(std::rc::Rc::unwrap_or_clone(if trueish {
                        then_body
                    } else {
                        else_body
                    }));
                    continue;
                }

                if then_body.is_empty() && else_body.is_empty() {
                    changed = true;
                    if expr_has_side_effects(&cond) {
                        rewritten.push(PreHirStmt::Expr(cond));
                    }
                    continue;
                }

                if then_body.is_empty() && !else_body.is_empty() {
                    changed = true;
                    rewritten.push(PreHirStmt::If {
                        cond: negate_expr(cond),
                        then_body: else_body,
                        else_body: Vec::new().into(),
                    });
                    continue;
                }

                rewritten.push(PreHirStmt::If {
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

pub fn simplify_empty_and_constant_ifs_recursive(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => {
                changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_mut()
                    && let PreHirStmt::Block(body) = init.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
                if let Some(update) = update.as_mut()
                    && let PreHirStmt::Block(body) = update.as_mut()
                {
                    changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                }
                changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
                changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
                }
                changed |= simplify_empty_and_constant_ifs_recursive(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    changed |= simplify_empty_and_constant_ifs(stmts);
    let before_len = stmts.len();
    stmts.retain(|stmt| !matches!(stmt, PreHirStmt::Block(body) if body.is_empty()));
    changed | (stmts.len() != before_len)
}

pub fn simplify_fallthrough_edges(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut rewritten = Vec::with_capacity(stmts.len());

    for idx in 0..stmts.len() {
        let stmt = stmts[idx].clone();
        let next_label = next_adjacent_label_name(stmts, idx + 1);
        match stmt {
            PreHirStmt::Goto(label) if next_label.as_deref() == Some(label.as_str()) => {
                changed = true;
            }
            PreHirStmt::If {
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
                    rewritten.push(PreHirStmt::Expr(cond));
                }
            }
            PreHirStmt::If {
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
                    rewritten.push(PreHirStmt::Expr(cond));
                }
            }
            PreHirStmt::If {
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
                            rewritten.push(PreHirStmt::Expr(cond));
                        }
                    }
                    (Some(_next), Some(then_target), Some(else_target))
                        if then_target == else_target =>
                    {
                        changed = true;
                        if expr_has_side_effects(&cond) {
                            rewritten.push(PreHirStmt::Expr(cond));
                        }
                        rewritten.push(PreHirStmt::Goto(then_target.to_string()));
                    }
                    (Some(next), Some(then_target), Some(else_target)) if then_target == next => {
                        changed = true;
                        rewritten.push(PreHirStmt::If {
                            cond: negate_expr(cond),
                            then_body: vec![PreHirStmt::Goto(else_target.to_string())].into(),
                            else_body: Vec::new().into(),
                        });
                    }
                    (Some(next), Some(then_target), Some(else_target)) if else_target == next => {
                        changed = true;
                        rewritten.push(PreHirStmt::If {
                            cond,
                            then_body: vec![PreHirStmt::Goto(then_target.to_string())].into(),
                            else_body: Vec::new().into(),
                        });
                    }
                    _ => rewritten.push(PreHirStmt::If {
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

fn next_adjacent_label_name(stmts: &[PreHirStmt], start_idx: usize) -> Option<String> {
    for stmt in stmts.iter().skip(start_idx) {
        match stmt {
            PreHirStmt::Label(label) => return Some(label.clone()),
            _ => return None,
        }
    }
    None
}

fn next_label_index_and_name(stmts: &[PreHirStmt], start_idx: usize) -> Option<(usize, String)> {
    for (idx, stmt) in stmts.iter().enumerate().skip(start_idx) {
        if let PreHirStmt::Label(label) = stmt {
            return Some((idx, label.clone()));
        }
    }
    None
}

fn matches_single_goto(body: &[PreHirStmt], label: &str) -> bool {
    matches!(body, [PreHirStmt::Goto(target)] if target == label)
}

pub fn fuse_single_predecessor_boundaries(stmts: &mut Vec<PreHirStmt>) -> bool {
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
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } if matches_single_goto(then_body, &label_name) && else_body.is_empty() => {
                Some(PreHirStmt::If {
                    cond: negate_expr(cond.clone()),
                    then_body: fused_segment.clone().into(),
                    else_body: Vec::new().into(),
                })
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } if then_body.is_empty() && matches_single_goto(else_body, &label_name) => {
                Some(PreHirStmt::If {
                    cond: cond.clone(),
                    then_body: fused_segment.clone().into(),
                    else_body: Vec::new().into(),
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

fn stmts_are_fuseable_linear_segment(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().all(stmt_is_fuseable_linear)
}

fn stmt_is_fuseable_linear(stmt: &PreHirStmt) -> bool {
    match stmt {
        // Return is linear for if-goto inversion: the statements between
        // `if (c) goto L;` and `L:` may include early returns (saturating_add).
        PreHirStmt::Assign { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Return(_) => true,
        PreHirStmt::Block(body) => stmts_are_fuseable_linear_segment(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmts_are_fuseable_linear_segment(then_body)
                && stmts_are_fuseable_linear_segment(else_body)
        }
        PreHirStmt::Switch { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn promote_guarded_jump_target_tail(stmts: &mut Vec<PreHirStmt>) -> bool {
    let referenced = collect_referenced_label_counts(stmts);
    let mut changed = false;
    let mut idx = 0usize;
    while idx + 3 < stmts.len() {
        let (
            PreHirStmt::If {
                cond: first_cond,
                then_body: first_then,
                else_body: first_else,
            },
            PreHirStmt::If {
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
        if !matches!(stmts.get(idx + 2), Some(PreHirStmt::Label(label)) if label == &body_label) {
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
            PreHirBinaryOp::LogicalOr,
        );
        stmts[idx] = PreHirStmt::If {
            cond: combined_cond,
            then_body: body_segment.into(),
            else_body: Vec::new().into(),
        };
        stmts.drain(idx + 1..=join_idx);
        changed = true;
        idx += 1;
    }
    changed
}

fn single_goto_target(body: &[PreHirStmt]) -> Option<&str> {
    match body {
        [PreHirStmt::Goto(target)] => Some(target.as_str()),
        _ => None,
    }
}

pub fn collapse_common_exit_guard_chain(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0usize;

    while idx < stmts.len() {
        let Some((exit_label, guard_count, conds)) = common_exit_guard_chain(stmts, idx) else {
            idx += 1;
            continue;
        };
        let Some(exit_idx) = stmts.iter().enumerate().skip(idx + guard_count).find_map(
            |(label_idx, stmt)| match stmt {
                PreHirStmt::Label(label) if label == &exit_label => Some(label_idx),
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
        let exit_cond = simplify_logical_expr(fold_logical_chain(conds, PreHirBinaryOp::LogicalOr));
        stmts[idx] = PreHirStmt::If {
            cond: negate_expr(exit_cond),
            then_body: guarded_body.into(),
            else_body: Vec::new().into(),
        };
        stmts.drain(idx + 1..exit_idx);
        changed = true;
        idx += 1;
    }

    changed
}

fn common_exit_guard_chain(
    stmts: &[PreHirStmt],
    start_idx: usize,
) -> Option<(String, usize, Vec<PreHirExpr>)> {
    let mut guard_count = 0usize;
    let mut exit_label: Option<String> = None;
    let mut conds = Vec::new();

    for stmt in stmts.iter().skip(start_idx) {
        let PreHirStmt::If {
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
    stmts: &mut Vec<PreHirStmt>,
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
    stmts: &mut Vec<PreHirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> bool {
    let local_refs = if global_refs.is_none() {
        Some(collect_referenced_labels(stmts))
    } else {
        None
    };
    let referenced = global_refs.unwrap_or_else(|| local_refs.as_ref().unwrap());
    let mut changed = false;
    while let Some(PreHirStmt::Label(label)) = stmts.first() {
        if !referenced.contains(label) && !should_preserve_unreferenced_leading_labels(stmts) {
            stmts.remove(0);
            changed = true;
        } else {
            break;
        }
    }
    changed
}

fn should_preserve_unreferenced_leading_labels(stmts: &[PreHirStmt]) -> bool {
    let first_non_label = stmts
        .iter()
        .position(|stmt| !matches!(stmt, PreHirStmt::Label(_)));
    match first_non_label {
        None => true,
        Some(idx) => matches!(stmts.get(idx..), Some([PreHirStmt::Return(_)])),
    }
}

pub fn single_pred_label_inline(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= single_pred_label_inline_in_stmt(stmt);
    }
    changed |= single_pred_label_inline_flat(stmts);
    changed
}

fn single_pred_label_inline_in_stmt(stmt: &mut PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Block(body) => single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body)),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let a = single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
            let b = single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
            a || b
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body))
        }
        PreHirStmt::For { body, .. } => single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body)),
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases.iter_mut() {
                changed |= single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            changed |= single_pred_label_inline(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            changed
        }
        _ => false,
    }
}

fn single_pred_label_inline_flat(stmts: &mut Vec<PreHirStmt>) -> bool {
    let mut changed = false;
    for _ in 0..512 {
        let ref_counts = collect_referenced_label_counts(stmts);

        let mut did_inline = false;
        let mut i = 0;
        while i < stmts.len() {
            let goto_label = match &stmts[i] {
                PreHirStmt::Goto(label) => label.clone(),
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
                .position(|s| matches!(s, PreHirStmt::Label(l) if l == &goto_label))
                .map(|offset| offset + i + 1);

            let Some(j) = label_pos else {
                if hoist_goto_target_from_guarded_infinite_loop(stmts, i, &goto_label, &ref_counts)
                {
                    did_inline = true;
                    changed = true;
                    continue;
                }
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
            //
            // `collect_defined_labels` walks into nested compound bodies
            // (a label inside the `While`/`For`/`If` this segment contains
            // is just as real a target as one at this level) -- a shallow
            // `segment.iter()` scan for direct `Label` elements only sees
            // top-level labels and silently drains ones nested one level
            // down that a surviving `Goto` outside the segment still
            // targets, leaving that `Goto` dangling.
            let external_ref_found = collect_defined_labels(segment).iter().any(|l| {
                if PROTECTED_LSDA_LABELS.with(|protected| protected.borrow().contains(l)) {
                    return true;
                }
                let total_refs = ref_counts.get(l).copied().unwrap_or(0);
                let internal_refs = segment_label_refs.get(l).copied().unwrap_or(0);
                total_refs > internal_refs
            });

            if external_ref_found {
                i += 1;
                continue;
            }

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

/// `goto L` whose only target is a `Label(L)` buried as the first statement
/// inside a solitary `while (1) { L: ... }` guarded by a redundant `if
/// (cond) { ... }` precheck -- GCC -O1+ loop-rotation output routinely
/// hoists a do-while's own back-edge condition and reuses it as a bogus
/// entry guard, even when the real entry is this unconditional goto landing
/// straight inside the loop body. `single_pred_label_inline_flat`'s flat
/// `Label` scan above only finds a target at this segment's own top level;
/// here the label is nested two levels down (`If.then_body -> While.body`),
/// so it falls through to this fallback instead. The same "everything
/// between the goto and its target is unreachable" argument that justifies
/// draining the segment above applies to the guard's own condition and dead
/// `else` arm too: the only way execution reaches *any* part of this `If`
/// is by jumping straight past its condition into the loop body, so the
/// condition is never evaluated and the other arm never taken.
fn hoist_goto_target_from_guarded_infinite_loop(
    stmts: &mut Vec<PreHirStmt>,
    goto_idx: usize,
    goto_label: &str,
    ref_counts: &HashMap<String, usize>,
) -> bool {
    let search_start = goto_idx + 1;
    let Some(if_pos) = stmts[search_start..]
        .iter()
        .position(|s| guarded_infinite_loop_entry_label(s) == Some(goto_label))
        .map(|offset| offset + search_start)
    else {
        return false;
    };

    let dead_zone = &stmts[search_start..if_pos];
    let dead_zone_refs = collect_referenced_label_counts(dead_zone);
    let external_ref_found = collect_defined_labels(dead_zone).iter().any(|l| {
        if PROTECTED_LSDA_LABELS.with(|protected| protected.borrow().contains(l)) {
            return true;
        }
        let total_refs = ref_counts.get(l).copied().unwrap_or(0);
        let internal_refs = dead_zone_refs.get(l).copied().unwrap_or(0);
        total_refs > internal_refs
    });
    if external_ref_found {
        return false;
    }

    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmts.remove(if_pos)
    else {
        unreachable!("position matched by guarded_infinite_loop_entry_label above");
    };
    let while_stmt = std::rc::Rc::unwrap_or_clone(then_body)
        .into_iter()
        .next()
        .filter(|s| matches!(s, PreHirStmt::While { .. }))
        .or_else(|| {
            std::rc::Rc::unwrap_or_clone(else_body)
                .into_iter()
                .next()
                .filter(|s| matches!(s, PreHirStmt::While { .. }))
        })
        .expect("shape re-checked by guarded_infinite_loop_entry_label above");
    stmts.drain(search_start..if_pos);
    stmts[goto_idx] = while_stmt;
    true
}

/// If `stmt` is `If { then_body: [While { cond: nonzero const, body }],
/// else_body: [] }` (or the same with the arms swapped), and `body`'s first
/// statement is a `Label`, returns that label. `None` for anything else.
fn guarded_infinite_loop_entry_label(stmt: &PreHirStmt) -> Option<&str> {
    let PreHirStmt::If {
        then_body,
        else_body,
        ..
    } = stmt
    else {
        return None;
    };
    let solitary_while = match (then_body.as_slice(), else_body.as_slice()) {
        ([w @ PreHirStmt::While { .. }], []) | ([], [w @ PreHirStmt::While { .. }]) => w,
        _ => return None,
    };
    let PreHirStmt::While {
        cond: PreHirExpr::Const(value, _),
        body,
    } = solitary_while
    else {
        return None;
    };
    if *value == 0 {
        return None;
    }
    match body.first() {
        Some(PreHirStmt::Label(l)) => Some(l.as_str()),
        _ => None,
    }
}
