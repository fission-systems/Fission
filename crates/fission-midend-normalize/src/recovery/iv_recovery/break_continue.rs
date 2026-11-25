use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Within a loop body (already extracted from the parent statement list),
/// replace `If { then_body: [Goto(exit_label)], else_body: [] }` with
/// `If { then_body: [Break], ... }` when `exit_label` is an "after-loop" label.
///
/// `after_labels` = set of labels defined *after* the loop in the same flat
/// statement list.  `head_labels` = labels defined immediately before the loop.
/// `goto_counts` = number of times each label is reached by a Goto in the
/// *entire function* (so we know if the label becomes dead after replacement).
fn recover_break_continue_in_body(
    body: &mut Vec<PreHirStmt>,
    after_labels: &HashSet<String>,
    head_labels: &HashSet<String>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    // Recurse into nested loops first (inner loops are handled before outer).
    for stmt in body.iter_mut() {
        match stmt {
            PreHirStmt::While { body: inner, .. }
            | PreHirStmt::DoWhile { body: inner, .. }
            | PreHirStmt::For { body: inner, .. } => {
                // For nested loops we collect their own after/head labels.
                // We skip nested loops here — the outer pass will recurse on
                // the whole function again if changed.
                let _ = inner; // recurse handled at top level
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < body.len() {
        let do_break = if let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &body[i]
        {
            if else_body.is_empty() {
                if let [PreHirStmt::Goto(lbl)] = then_body.as_slice() {
                    after_labels.contains(lbl)
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        let do_continue = if !do_break {
            if let PreHirStmt::If {
                then_body,
                else_body,
                ..
            } = &body[i]
            {
                if else_body.is_empty() {
                    if let [PreHirStmt::Goto(lbl)] = then_body.as_slice() {
                        head_labels.contains(lbl)
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if do_break {
            if let PreHirStmt::If { then_body, .. } = &mut body[i] {
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body).clear();
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body).push(PreHirStmt::Break);
                changed = true;
            }
        } else if do_continue {
            if let PreHirStmt::If { then_body, .. } = &mut body[i] {
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body).clear();
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body).push(PreHirStmt::Continue);
                changed = true;
            }
        }
        i += 1;
    }
    changed
}

/// Top-level break/continue recovery for a flat statement list.
/// Scans every While/DoWhile/For loop and replaces eligible Goto patterns
/// inside their bodies.
pub(super) fn apply_break_continue_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    goto_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let n = stmts.len();
    for loop_idx in 0..n {
        // Exit labels: only the labels directly after the loop are where a
        // `break` lands. A label further on (or nested inside a later
        // statement) is reached past the code in between, so a jump there
        // is not a break.
        let after_labels: HashSet<String> = stmts[loop_idx + 1..]
            .iter()
            .map_while(|stmt| match stmt {
                PreHirStmt::Label(label) => Some(label.clone()),
                _ => None,
            })
            .collect();
        // Head labels: only the labels directly before a `while` loop are
        // where a `continue` lands. A do-while's `continue` evaluates the
        // condition first and a for's runs the update, so a jump back to the
        // label in front of either is not a continue.
        let head_labels: HashSet<String> = if matches!(stmts[loop_idx], PreHirStmt::While { .. }) {
            stmts[..loop_idx]
                .iter()
                .rev()
                .map_while(|stmt| match stmt {
                    PreHirStmt::Label(label) => Some(label.clone()),
                    _ => None,
                })
                .collect()
        } else {
            HashSet::default()
        };

        let is_loop = matches!(
            &stmts[loop_idx],
            PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. }
        );
        if !is_loop {
            continue;
        }

        let body = match &mut stmts[loop_idx] {
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => body,
            _ => unreachable!(),
        };
        changed |= recover_break_continue_in_body(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            &after_labels,
            &head_labels,
            goto_counts,
        );
    }

    // Recurse into If/Block/Switch to catch loops nested there.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= apply_break_continue_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    goto_counts,
                );
                changed |= apply_break_continue_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    goto_counts,
                );
            }
            PreHirStmt::Block(body) => {
                changed |= apply_break_continue_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    goto_counts,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= apply_break_continue_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        goto_counts,
                    );
                }
                changed |= apply_break_continue_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    goto_counts,
                );
            }
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                // Recurse for nested loops.
                changed |= apply_break_continue_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    goto_counts,
                );
            }
            _ => {}
        }
    }

    changed
}
