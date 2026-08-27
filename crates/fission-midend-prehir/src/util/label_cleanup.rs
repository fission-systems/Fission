//! Pure HIR label cleanup shared by normalize and structuring.

use crate::ir::{PreHirStmt, PreHirSwitchCase};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub fn cleanup_redundant_labels(
    body: Vec<PreHirStmt>,
    global_refs: Option<&HashSet<String>>,
) -> Vec<PreHirStmt> {
    let mut aliases = adjacent_label_aliases(&body);
    if let Some(referenced) = global_refs {
        // Recursive callers clean one nested statement list at a time.  They
        // cannot rewrite gotos in sibling or parent lists, so a label that is
        // referenced elsewhere in the function must not be folded into an
        // adjacent local alias here.
        aliases.retain(|alias, _| !referenced.contains(alias));
    }
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
            PreHirStmt::Label(label) => {
                if !seen_labels.insert(label.clone()) {
                    continue;
                }
                if cleaned.is_empty() || referenced.contains(&label) {
                    cleaned.push(PreHirStmt::Label(label));
                }
            }
            other => cleaned.push(other),
        }
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_preserves_adjacent_alias_referenced_outside_current_body() {
        let body = vec![
            PreHirStmt::Return(None),
            PreHirStmt::Label("dream_L9".to_string()),
            PreHirStmt::Label("block_401000".to_string()),
            PreHirStmt::Return(None),
        ];
        let global_refs = HashSet::from(["dream_L9".to_string()]);

        let cleaned = cleanup_redundant_labels(body, Some(&global_refs));

        assert!(cleaned.contains(&PreHirStmt::Label("dream_L9".to_string())));
        assert!(!cleaned.contains(&PreHirStmt::Label("block_401000".to_string())));
    }
}

fn adjacent_label_aliases(body: &[PreHirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let PreHirStmt::Label(_) = &body[idx] else {
            idx += 1;
            continue;
        };
        let start = idx;
        while idx + 1 < body.len() && matches!(body[idx + 1], PreHirStmt::Label(_)) {
            idx += 1;
        }
        if idx > start {
            let PreHirStmt::Label(canonical) = &body[idx] else {
                unreachable!();
            };
            for alias_idx in start..idx {
                let PreHirStmt::Label(alias) = &body[alias_idx] else {
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

fn rewrite_stmt_labels(
    body: Vec<PreHirStmt>,
    aliases: &HashMap<String, String>,
) -> Vec<PreHirStmt> {
    body.into_iter()
        .map(|stmt| rewrite_stmt_label(stmt, aliases))
        .collect()
}

fn rewrite_stmt_label(stmt: PreHirStmt, aliases: &HashMap<String, String>) -> PreHirStmt {
    match stmt {
        PreHirStmt::Block(body) => PreHirStmt::Block(Rc::new(rewrite_stmt_labels(
            Rc::unwrap_or_clone(body),
            aliases,
        ))),
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => PreHirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|case| PreHirSwitchCase {
                    values: case.values,
                    body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(case.body), aliases)),
                })
                .collect(),
            default: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(default), aliases)),
        },
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond,
            then_body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(then_body), aliases)),
            else_body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(else_body), aliases)),
        },
        PreHirStmt::While { cond, body } => PreHirStmt::While {
            cond,
            body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(body), aliases)),
        },
        PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(body), aliases)),
            cond,
        },
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => PreHirStmt::For {
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
            body: Rc::new(rewrite_stmt_labels(Rc::unwrap_or_clone(body), aliases)),
        },
        PreHirStmt::Label(label) => PreHirStmt::Label(canonicalize_label(&label, aliases)),
        PreHirStmt::Goto(label) => PreHirStmt::Goto(canonicalize_label(&label, aliases)),
        other => other,
    }
}

pub fn collect_referenced_labels(body: &[PreHirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

fn collect_stmt_referenced_labels(stmt: &PreHirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for stmt in body.iter() {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in case.body.iter() {
                    collect_stmt_referenced_labels(stmt, referenced);
                }
            }
            for stmt in default.iter() {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body.iter() {
                collect_stmt_referenced_labels(stmt, referenced);
            }
            for stmt in else_body.iter() {
                collect_stmt_referenced_labels(stmt, referenced);
            }
        }
        PreHirStmt::Goto(label) => {
            referenced.insert(label.clone());
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub fn collect_referenced_label_counts(body: &[PreHirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

fn collect_stmt_referenced_label_counts(stmt: &PreHirStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for stmt in body.iter() {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for stmt in case.body.iter() {
                    collect_stmt_referenced_label_counts(stmt, counts);
                }
            }
            for stmt in default.iter() {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for stmt in then_body.iter() {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
            for stmt in else_body.iter() {
                collect_stmt_referenced_label_counts(stmt, counts);
            }
        }
        PreHirStmt::Goto(label) => {
            *counts.entry(label.clone()).or_insert(0) += 1;
        }
        _ => {}
    }
}
