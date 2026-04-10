use super::*;

// ---------------------------------------------------------------------------
// Goto elimination post-pass
// ---------------------------------------------------------------------------
//
// Three fixpoint rules applied in sequence until convergence:
//
//  1. Empty-jump removal:   `Goto(L)` immediately followed by `Label(L)` → remove the Goto.
//  2. Single-reference inline: `Label(L)` referenced exactly once as `Goto(L)` → remove the
//     Label and the Goto (they're already adjacent after rule 1 or after inlining).
//  3. Conditional goto inversion: `if (cond) { Goto(L) }` directly followed by `Label(L)`
//     and the rest of the code → replace with `if (!cond) { rest_code }`.
//
// Rules are applied at the TOP LEVEL only (not recursed into nested scopes) per iteration.
// After each pass that changes anything, the whole pass restarts to reach a fixpoint.

/// Apply all three goto-elimination rules at the top level of a statement list.
/// Returns `(cleaned, changed)` where `changed` indicates whether any rule fired.
fn goto_elim_pass(stmts: Vec<HirStmt>) -> (Vec<HirStmt>, bool) {
    let mut changed = false;
    let stmts = empty_jump_removal(stmts, &mut changed);
    let stmts = single_ref_label_inline(stmts, &mut changed);
    let stmts = cond_goto_inversion(stmts, &mut changed);
    (stmts, changed)
}

/// Rule 1: If a `Goto(L)` is immediately followed by `Label(L)`, remove the Goto.
fn empty_jump_removal(stmts: Vec<HirStmt>, changed: &mut bool) -> Vec<HirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        if let HirStmt::Goto(ref label) = stmt {
            if let Some(HirStmt::Label(next_label)) = iter.peek() {
                if label == next_label {
                    *changed = true;
                    continue; // drop the Goto; Label stays
                }
            }
        }
        out.push(stmt);
    }
    out
}

/// Rule 2: If a `Label(L)` is referenced exactly once (as a `Goto(L)`) in the same list,
/// and that Goto immediately precedes the Label (after rule 1), remove both.
fn single_ref_label_inline(stmts: Vec<HirStmt>, changed: &mut bool) -> Vec<HirStmt> {
    let ref_counts = collect_referenced_label_counts(&stmts);
    let singleton_labels: HashSet<&str> = ref_counts
        .iter()
        .filter(|&(_, &count)| count == 1)
        .map(|(label, _)| label.as_str())
        .collect();
    if singleton_labels.is_empty() {
        return stmts;
    }

    let mut out = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();
    while let Some(stmt) = iter.next() {
        // If we see `Goto(L)` where L has exactly one reference and the next stmt is
        // `Label(L)`, drop both (the label was already removed by rule 1 in the same
        // pass, or the Goto and Label are genuinely adjacent here).
        if let HirStmt::Goto(ref label) = stmt {
            if singleton_labels.contains(label.as_str()) {
                if let Some(HirStmt::Label(next_label)) = iter.peek() {
                    if label == next_label {
                        *changed = true;
                        let _ = iter.next(); // consume the Label
                        continue;
                    }
                }
            }
        }
        out.push(stmt);
    }
    out
}

/// Rule 3: `if (cond) { Goto(L) }` directly followed by `Label(L)` + rest →
/// `if (!cond) { rest }`.  This handles early-exit / guard patterns.
fn cond_goto_inversion(stmts: Vec<HirStmt>, changed: &mut bool) -> Vec<HirStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        // Pattern: If { cond, then=[Goto(L)], else=[] }  followed by  Label(L)  and rest
        if let HirStmt::If { cond, then_body, else_body } = &stmts[i] {
            if else_body.is_empty() {
                if let [HirStmt::Goto(goto_label)] = then_body.as_slice() {
                    // Find the immediately following Label(L) at the top level.
                    if i + 1 < stmts.len() {
                        if let HirStmt::Label(label) = &stmts[i + 1] {
                            if goto_label == label {
                                // Collect everything after the label as the inlined else body.
                                let inverted_cond = negate_expr(cond.clone());
                                let rest_body: Vec<HirStmt> =
                                    stmts[i + 2..].to_vec();
                                if !rest_body.is_empty() {
                                    *changed = true;
                                    out.push(HirStmt::If {
                                        cond: inverted_cond,
                                        then_body: rest_body,
                                        else_body: Vec::new(),
                                    });
                                    break; // rest_body is now inside the if, stop iteration
                                }
                            }
                        }
                    }
                }
            }
        }
        out.push(stmts[i].clone());
        i += 1;
    }
    out
}

/// Apply `goto_elim_pass` to fixpoint (convergence when no rule fires).
/// Only operates at the TOP LEVEL of `stmts`; nested scopes are not recursed.
/// Callers that need nested cleanup should call this recursively.
pub(crate) fn eliminate_redundant_gotos(mut stmts: Vec<HirStmt>) -> Vec<HirStmt> {
    const MAX_GOTO_ELIM_ITERS: usize = 32;
    for _ in 0..MAX_GOTO_ELIM_ITERS {
        let (new_stmts, changed) = goto_elim_pass(stmts);
        stmts = new_stmts;
        if !changed {
            break;
        }
    }
    stmts
}

// ---------------------------------------------------------------------------
// Existing label-cleanup utilities
// ---------------------------------------------------------------------------

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

pub(super) fn normalize_guarded_tail_layout(body: Vec<HirStmt>) -> (Vec<HirStmt>, usize) {
    let cleaned = cleanup_redundant_labels(body);
    let (canonicalized, rewritten_aliases) = canonicalize_top_level_forward_label_aliases(cleaned);
    let cleaned = cleanup_redundant_labels(canonicalized);
    (cleaned, rewritten_aliases)
}

pub(super) fn canonicalize_top_level_forward_label_aliases(
    body: Vec<HirStmt>,
) -> (Vec<HirStmt>, usize) {
    let (aliases, alias_ranges) = top_level_forward_label_aliases_with_ranges(&body);
    if aliases.is_empty() {
        return (body, 0);
    }

    let rewritten = rewrite_stmt_labels(body, &aliases);
    let mut out = Vec::with_capacity(rewritten.len());
    let mut idx = 0usize;
    let mut range_idx = 0usize;

    while idx < rewritten.len() {
        while range_idx < alias_ranges.len() && alias_ranges[range_idx].1 <= idx {
            range_idx += 1;
        }
        if range_idx < alias_ranges.len() {
            let (start, end) = alias_ranges[range_idx];
            if idx >= start && idx < end {
                idx = end;
                continue;
            }
        }
        out.push(rewritten[idx].clone());
        idx += 1;
    }

    (out, aliases.len())
}

fn top_level_forward_label_aliases_with_ranges(
    body: &[HirStmt],
) -> (HashMap<String, String>, Vec<(usize, usize)>) {
    let mut aliases = HashMap::new();
    let mut ranges = Vec::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let HirStmt::Label(alias_label) = &body[idx] else {
            idx += 1;
            continue;
        };
        let next_label_idx =
            (idx + 1..body.len()).find(|pos| matches!(body[*pos], HirStmt::Label(_)));
        let Some(next_label_idx) = next_label_idx else {
            idx += 1;
            continue;
        };
        let HirStmt::Label(next_label) = &body[next_label_idx] else {
            unreachable!();
        };
        if is_top_level_forward_alias_segment(&body[idx + 1..next_label_idx], next_label) {
            aliases.insert(alias_label.clone(), next_label.clone());
            ranges.push((idx, next_label_idx));
        }
        idx = next_label_idx;
    }
    (aliases, ranges)
}

fn is_top_level_forward_alias_segment(segment: &[HirStmt], next_label: &str) -> bool {
    let mut saw_forward_goto = false;
    for stmt in segment {
        if is_ignorable_discovery_stmt(stmt) {
            continue;
        }
        match stmt {
            HirStmt::Goto(label) if !saw_forward_goto && label == next_label => {
                saw_forward_goto = true;
            }
            _ => return false,
        }
    }
    saw_forward_goto
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
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => HirStmt::For {
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
        | HirStmt::VaStart { .. }
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
        | HirStmt::VaStart { .. }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goto_elim_removes_empty_jump_before_label() {
        let stmts = vec![
            HirStmt::Goto("exit".to_string()),
            HirStmt::Label("exit".to_string()),
            HirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![HirStmt::Label("exit".to_string()), HirStmt::Return(None)]
        );
    }

    #[test]
    fn goto_elim_removes_single_ref_label_and_goto_pair() {
        // Goto(L) immediately before Label(L) with a single reference → both removed.
        let stmts = vec![
            HirStmt::Goto("lbl".to_string()),
            HirStmt::Label("lbl".to_string()),
            HirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After empty-jump removal, Label(lbl) + Return remains.
        // Then single-ref inline removes both Goto and Label (they are adjacent).
        // The result should have no Goto and no Label.
        assert!(
            !result.iter().any(|s| matches!(s, HirStmt::Goto(_))),
            "goto should be eliminated: {result:?}"
        );
    }

    #[test]
    fn goto_elim_inverts_conditional_goto_followed_by_label() {
        // `if (cond) { Goto(L) }; Label(L); Return` →
        // `if (!cond) { Return }` (conditional inversion).
        let stmts = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("tail".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("tail".to_string()),
            HirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After inversion the Label should be gone and we should have a single If.
        assert_eq!(result.len(), 1, "expected single If after inversion: {result:?}");
        let HirStmt::If { else_body, then_body, .. } = &result[0] else {
            panic!("expected If: {result:?}");
        };
        assert!(else_body.is_empty(), "else should be empty: {result:?}");
        assert_eq!(then_body, &vec![HirStmt::Return(None)]);
    }

    #[test]
    fn normalize_guarded_tail_layout_collapses_adjacent_labels_before_alias_rewrite() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("block_alias_a".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("block_alias_a".to_string()),
            HirStmt::Label("block_alias_b".to_string()),
            HirStmt::Goto("block_tail".to_string()),
            HirStmt::Label("block_tail".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let (normalized, _) = normalize_guarded_tail_layout(body);
        assert_eq!(
            normalized,
            vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_string()),
                    then_body: vec![HirStmt::Goto("block_tail".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Label("block_tail".to_string()),
                HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_rewrites_and_prunes_alias_segment() {
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Label("block_alias".to_string()),
            HirStmt::Goto("block_tail".to_string()),
            HirStmt::Label("block_tail".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body);
        assert_eq!(rewritten, 1);
        assert_eq!(
            normalized,
            vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_string()),
                    then_body: vec![HirStmt::Goto("block_tail".to_string())],
                    else_body: Vec::new(),
                },
                HirStmt::Label("block_tail".to_string()),
                HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_preserves_nontrivial_alias_payload() {
        let body = vec![
            HirStmt::Label("block_alias".to_string()),
            HirStmt::Expr(HirExpr::Var("work".to_string())),
            HirStmt::Goto("block_tail".to_string()),
            HirStmt::Label("block_tail".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body.clone());
        assert_eq!(rewritten, 0);
        assert_eq!(normalized, body);
    }
}
