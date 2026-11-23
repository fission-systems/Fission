use super::super::for_loops::stmt_list_contains_continue_pub;
use super::labels::{collect_labels, count_goto_targets};
use super::loop_expressions::invert_condition;
use crate::prelude::*;
use crate::{HashMap, HashSet};

fn tail_goto_condition(stmt: &PreHirStmt, label: &str) -> Option<PreHirExpr> {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() {
        return None;
    }
    matches!(then_body.as_slice(), [PreHirStmt::Goto(target)] if target == label)
        .then(|| cond.clone())
}

fn collect_loop_body_labels(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut labels = HashSet::default();
    collect_labels(stmts, &mut labels);
    labels
}

fn collect_goto_targets_in_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_goto_targets_in_stmt(stmt, out);
    }
}

fn collect_goto_targets_in_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Goto(label) => {
            out.insert(label.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_goto_targets_in_stmts(then_body, out);
            collect_goto_targets_in_stmts(else_body, out);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => collect_goto_targets_in_stmts(body, out),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_goto_targets_in_stmts(&case.body, out);
            }
            collect_goto_targets_in_stmts(default, out);
        }
        _ => {}
    }
}

fn has_unscoped_break(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(has_unscoped_break_stmt)
}

fn has_unscoped_break_stmt(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Break => true,
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => has_unscoped_break(then_body) || has_unscoped_break(else_body),
        PreHirStmt::Block(body) => has_unscoped_break(body),
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| has_unscoped_break(&case.body)) || has_unscoped_break(default)
        }
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => false,
        _ => false,
    }
}

fn body_gotos_are_loop_local(body: &[PreHirStmt]) -> bool {
    let labels = collect_loop_body_labels(body);
    let mut gotos = HashSet::default();
    collect_goto_targets_in_stmts(body, &mut gotos);
    gotos.iter().all(|target| labels.contains(target))
}

fn try_tail_label_loop_to_for(
    stmts: &mut Vec<PreHirStmt>,
    label_idx: usize,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let PreHirStmt::Label(label) = &stmts[label_idx] else {
        return false;
    };
    let label = label.clone();
    if goto_counts.get(&label).copied().unwrap_or(0) != 1 {
        return false;
    }

    for tail_idx in label_idx + 1..stmts.len() {
        let Some(continue_cond) = tail_goto_condition(&stmts[tail_idx], &label) else {
            continue;
        };
        let body_slice = &stmts[label_idx + 1..tail_idx];
        if body_slice.is_empty()
            || stmt_list_contains_continue_pub(body_slice)
            || has_unscoped_break(body_slice)
            || !body_gotos_are_loop_local(body_slice)
        {
            return false;
        }

        // Side-entry check: Reject loop recovery if there are jumps from outside into the body.
        let body_labels = collect_loop_body_labels(body_slice);
        let mut internal_goto_counts = HashMap::default();
        count_goto_targets(body_slice, &mut internal_goto_counts);
        for label_in_body in &body_labels {
            let global_gotos = goto_counts.get(label_in_body).copied().unwrap_or(0);
            let internal_gotos = internal_goto_counts
                .get(label_in_body)
                .copied()
                .unwrap_or(0);
            if global_gotos > internal_gotos {
                return false;
            }
        }

        let mut body = body_slice.to_vec();
        body.push(PreHirStmt::If {
            cond: invert_condition(continue_cond),
            then_body: vec![PreHirStmt::Break].into(),
            else_body: Vec::new().into(),
        });
        let replacement = PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body: body.into(),
        };
        stmts.splice(label_idx..=tail_idx, [replacement]);
        return true;
    }

    false
}

pub(super) fn apply_tail_label_loop_recovery_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if matches!(&stmts[i], PreHirStmt::Label(_))
            && try_tail_label_loop_to_for(stmts, i, goto_counts)
        {
            changed = true;
            continue;
        }

        match &mut stmts[i] {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    goto_counts,
                );
                changed |= apply_tail_label_loop_recovery_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    goto_counts,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= apply_tail_label_loop_recovery_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    goto_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_tail_label_loop_recovery_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        goto_counts,
                    );
                }
                changed |= apply_tail_label_loop_recovery_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    goto_counts,
                );
            }
            _ => {}
        }
        i += 1;
    }
    changed
}
