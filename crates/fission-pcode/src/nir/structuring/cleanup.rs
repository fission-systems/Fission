use super::*;

pub(crate) fn cleanup_redundant_labels(body: Vec<HirStmt>) -> Vec<HirStmt> {
    let aliases = adjacent_label_aliases(&body);
    let body = rewrite_stmt_labels(body, &aliases);
    let referenced = collect_referenced_labels(&body);
    let mut cleaned = Vec::with_capacity(body.len());
    let mut seen_labels = HashSet::new();

    for stmt in body {
        match stmt {
            HirStmt::Label(label) => {
                if !seen_labels.insert(label.clone()) {
                    continue;
                }
                if cleaned.is_empty() || referenced.contains(&label) {
                    cleaned.push(HirStmt::Label(label));
                }
            }
            other => cleaned.push(other),
        }
    }

    cleaned
}

fn adjacent_label_aliases(body: &[HirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let HirStmt::Label(_) = &body[idx] else {
            idx += 1;
            continue;
        };
        let start = idx;
        while idx + 1 < body.len() && matches!(body[idx + 1], HirStmt::Label(_)) {
            idx += 1;
        }
        if idx > start {
            let HirStmt::Label(canonical) = &body[idx] else {
                unreachable!();
            };
            for alias_idx in start..idx {
                let HirStmt::Label(alias) = &body[alias_idx] else {
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

fn rewrite_stmt_labels(body: Vec<HirStmt>, aliases: &HashMap<String, String>) -> Vec<HirStmt> {
    body.into_iter()
        .map(|stmt| rewrite_stmt_label(stmt, aliases))
        .collect()
}

fn rewrite_stmt_label(stmt: HirStmt, aliases: &HashMap<String, String>) -> HirStmt {
    match stmt {
        HirStmt::Block(body) => HirStmt::Block(rewrite_stmt_labels(body, aliases)),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => HirStmt::Switch {
            expr,
            cases: cases
                .into_iter()
                .map(|case| HirSwitchCase {
                    values: case.values,
                    body: rewrite_stmt_labels(case.body, aliases),
                })
                .collect(),
            default: rewrite_stmt_labels(default, aliases),
        },
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => HirStmt::If {
            cond,
            then_body: rewrite_stmt_labels(then_body, aliases),
            else_body: rewrite_stmt_labels(else_body, aliases),
        },
        HirStmt::While { cond, body } => HirStmt::While {
            cond,
            body: rewrite_stmt_labels(body, aliases),
        },
        HirStmt::DoWhile { body, cond } => HirStmt::DoWhile {
            body: rewrite_stmt_labels(body, aliases),
            cond,
        },
        HirStmt::Label(label) => HirStmt::Label(canonicalize_label(&label, aliases)),
        HirStmt::Goto(label) => HirStmt::Goto(canonicalize_label(&label, aliases)),
        other => other,
    }
}

fn collect_referenced_labels(body: &[HirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub(super) fn collect_referenced_label_counts(body: &[HirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
}

fn collect_stmt_referenced_labels(stmt: &HirStmt, referenced: &mut HashSet<String>) {
    match stmt {
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
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
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
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

pub(super) fn single_goto_target(body: &[HirStmt]) -> Option<&str> {
    match body {
        [HirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

pub(super) fn has_top_level_label(body: &[HirStmt]) -> bool {
    body.iter().any(|stmt| matches!(stmt, HirStmt::Label(_)))
}

pub(super) fn is_ignorable_discovery_stmt(stmt: &HirStmt) -> bool {
    matches!(stmt, HirStmt::Label(_)) || matches!(stmt, HirStmt::Block(body) if body.is_empty())
}

pub(super) fn trim_ignorable_stmt_bounds(body: &[HirStmt]) -> Option<(usize, usize)> {
    let start = body
        .iter()
        .position(|stmt| !is_ignorable_discovery_stmt(stmt))?;
    let end = body
        .iter()
        .rposition(|stmt| !is_ignorable_discovery_stmt(stmt))
        .unwrap_or(start);
    Some((start, end + 1))
}

pub(super) fn has_non_ignorable_payload(body: &[HirStmt]) -> bool {
    trim_ignorable_stmt_bounds(body).is_some()
}
