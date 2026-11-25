use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Count occurrences of each label name as a Goto target in a statement list.
pub(super) fn count_goto_targets(stmts: &[PreHirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_goto_targets_stmt(stmt, counts);
    }
}

fn count_goto_targets_stmt(stmt: &PreHirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        PreHirStmt::If {
            cond: _,
            then_body,
            else_body,
        } => {
            count_goto_targets(then_body, counts);
            count_goto_targets(else_body, counts);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            count_goto_targets(body, counts);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_goto_targets(&case.body, counts);
            }
            count_goto_targets(default, counts);
        }
        _ => {}
    }
}

/// Collect all label *definitions* in a statement list.
pub(super) fn collect_labels(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_labels_stmt(stmt, out);
    }
}

fn collect_labels_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Label(label) => {
            out.insert(label.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_labels(then_body, out);
            collect_labels(else_body, out);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            collect_labels(body, out);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_labels(&case.body, out);
            }
            collect_labels(default, out);
        }
        _ => {}
    }
}

pub(super) fn single_goto_target(stmts: &[PreHirStmt]) -> Option<&str> {
    match stmts {
        [PreHirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

pub(super) fn labels_after(stmts: &[PreHirStmt], idx: usize) -> HashSet<String> {
    let mut labels = HashSet::default();
    for stmt in stmts.iter().skip(idx + 1) {
        collect_labels_stmt(stmt, &mut labels);
    }
    labels
}
