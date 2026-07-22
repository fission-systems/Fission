//! Pure HIR label cleanup shared by normalize and structuring.

use crate::ir::{DirStmt, DirSwitchCase};
use std::collections::{HashMap, HashSet};

pub fn cleanup_redundant_labels(
    body: Vec<DirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> Vec<DirStmt> {
    let aliases = adjacent_label_aliases(&body);
    let body = rewrite_stmt_labels(body, &aliases);
    let local_refs = if global_refs.is_none() {
        Some(collect_referenced_labels(&body))
    } else {
        None
    };
    let referenced = global_refs.unwrap_or_else(|| local_refs.as_ref().unwrap());
    let mut cleaned = Vec::with_capacity(body.len());
    let mut seen_labels = HashSet::new();

    for stmt in body {
        match stmt {
            DirStmt::Label(label) => {
                if !seen_labels.insert(label.clone()) {
                    continue;
                }
                if cleaned.is_empty() || referenced.contains(&label) {
                    cleaned.push(DirStmt::Label(label));
                }
            }
            other => cleaned.push(other),
        }
    }

    cleaned
}

fn adjacent_label_aliases(body: &[DirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let DirStmt::Label(_) = &body[idx] else {
            idx += 1;
            continue;
        };
        let start = idx;
        while idx + 1 < body.len() && matches!(body[idx + 1], DirStmt::Label(_)) {
            idx += 1;
        }
        if idx > start {
            let DirStmt::Label(canonical) = &body[idx] else {
                unreachable!();
            };
            for alias_idx in start..idx {
                let DirStmt::Label(alias) = &body[alias_idx] else {
                    unreachable!();
                };
                aliases.insert(alias.clone(), canonical.clone());
            }
        }
        idx += 1;
    }
    aliases
}

fn canonicalize_label(label: &str, aliases: &HashMap<String, String>) -> String {
    let mut current = label.to_string();
    let mut seen = HashSet::new();
    while let Some(next) = aliases.get(&current) {
        if !seen.insert(current.clone()) {
            break;
        }
        current = next.clone();
    }
    current
}

fn rewrite_stmt_labels(body: Vec<DirStmt>, aliases: &HashMap<String, String>) -> Vec<DirStmt> {
    body.into_iter()
        .map(|stmt| rewrite_stmt_label(stmt, aliases))
        .collect()
}

fn rewrite_stmt_label(stmt: DirStmt, aliases: &HashMap<String, String>) -> DirStmt {
    match stmt {
        DirStmt::Block(body) => DirStmt::Block(rewrite_stmt_labels(body, aliases)),
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => DirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|case| DirSwitchCase {
                    values: case.values,
                    body: rewrite_stmt_labels(case.body, aliases),
                })
                .collect(),
            default: rewrite_stmt_labels(default, aliases),
        },
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => DirStmt::If {
            cond,
            then_body: rewrite_stmt_labels(then_body, aliases),
            else_body: rewrite_stmt_labels(else_body, aliases),
        },
        DirStmt::While { cond, body } => DirStmt::While {
            cond,
            body: rewrite_stmt_labels(body, aliases),
        },
        DirStmt::DoWhile { body, cond } => DirStmt::DoWhile {
            body: rewrite_stmt_labels(body, aliases),
            cond,
        },
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => DirStmt::For {
            init: init.map(|s| {
                Box::new(
                    rewrite_stmt_labels(vec![*s], aliases)
                        .into_iter()
                        .next()
                        .unwrap(),
                )
            }),
            cond,
            update: update.map(|s| {
                Box::new(
                    rewrite_stmt_labels(vec![*s], aliases)
                        .into_iter()
                        .next()
                        .unwrap(),
                )
            }),
            body: rewrite_stmt_labels(body, aliases),
        },
        DirStmt::Label(label) => DirStmt::Label(canonicalize_label(&label, aliases)),
        DirStmt::Goto(label) => DirStmt::Goto(canonicalize_label(&label, aliases)),
        other => other,
    }
}

pub fn collect_referenced_labels(body: &[DirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

fn collect_stmt_referenced_labels(stmt: &DirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        DirStmt::If {
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
        DirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}


pub fn collect_referenced_label_counts(body: &[DirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

fn collect_stmt_referenced_label_counts(stmt: &DirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for stmt in body {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in &case.body {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        DirStmt::If {
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
        DirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        _ => {}
    }
}
