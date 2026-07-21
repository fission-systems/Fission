use fission_midend_core::ir::{HirExpr, HirLValue, HirStmt, HirSwitchCase, NirType};
use fission_midend_core::util::label_cleanup as core_labels;
use crate::HashMap;
use crate::HashSet;


mod goto;
pub use goto::eliminate_redundant_gotos;

/// `protected` labels are never removed by the cleanup passes below even
/// when nothing in `body` textually references them via `Goto` -- see
/// [`cleanup_redundant_labels_protecting`]. Pass `host.lsda_landing_pad_labels()`
/// (empty for the overwhelming majority of functions, which have no C++
/// exception handling at all).
pub fn finalize_structured_body(protected: &HashSet<String>, mut body: Vec<HirStmt>) -> Vec<HirStmt> {
    body = eliminate_redundant_gotos(body);
    body = dedupe_structured_region_entry_labels(body);
    body = cleanup_redundant_labels_protecting(body, protected);
    let referenced = collect_referenced_labels(&body);
    while matches!(body.first(), Some(HirStmt::Label(label)) if !referenced.contains(label) && !protected.contains(label))
    {
        body.remove(0);
    }
    body
}

/// Like [`cleanup_redundant_labels`], but additionally protects every label
/// in `protected` from removal even when nothing in `body` textually
/// references it via `Goto` -- for labels reachable only via an edge with
/// no `HirStmt` representation at all (see
/// `StructuringHost::lsda_landing_pad_labels`). Ordinary label cleanup
/// (`referenced.contains`) is exactly right for real dead labels; it just
/// has no way to know a C++ exception landing pad's label is a live entry
/// point when nothing in the text ever does `Goto` to it.
pub fn cleanup_redundant_labels_protecting(
    body: Vec<HirStmt>,
    protected: &HashSet<String>,
) -> Vec<HirStmt> {
    if protected.is_empty() {
        return cleanup_redundant_labels(body, None);
    }
    let mut referenced = core_labels::collect_referenced_labels(&body);
    referenced.extend(protected.iter().cloned());
    cleanup_redundant_labels(body, Some(&referenced))
}

/// Remove duplicate block labels emitted both outside and inside a structured region
/// (e.g. `Label(L); while (1) { Label(L); ... }`). Keeps the inner declaration so
/// loop back-edges and continue lowering remain anchored on the region body.
pub fn dedupe_structured_region_entry_labels(mut body: Vec<HirStmt>) -> Vec<HirStmt> {
    dedupe_structured_region_entry_labels_in_place(&mut body);
    body
}

fn first_meaningful_label(stmts: &[HirStmt]) -> Option<&str> {
    stmts.iter().find_map(|stmt| {
        if let HirStmt::Label(label) = stmt {
            Some(label.as_str())
        } else {
            None
        }
    })
}

fn dedupe_structured_region_entry_labels_in_place(stmts: &mut Vec<HirStmt>) {
    let mut i = 0;
    while i < stmts.len() {
        if let HirStmt::Label(outer) = stmts[i].clone() {
            if i + 1 < stmts.len() {
                let inner_matches = match &mut stmts[i + 1] {
                    HirStmt::While { body, .. }
                    | HirStmt::DoWhile { body, .. }
                    | HirStmt::For { body, .. } => {
                        dedupe_structured_region_entry_labels_in_place(body);
                        first_meaningful_label(body) == Some(outer.as_str())
                    }
                    _ => false,
                };
                if inner_matches {
                    stmts.remove(i);
                    continue;
                }
            }
        }
        dedupe_structured_region_entry_labels_stmt(&mut stmts[i]);
        i += 1;
    }
}

fn dedupe_structured_region_entry_labels_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Block(body) => dedupe_structured_region_entry_labels_in_place(body),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            dedupe_structured_region_entry_labels_in_place(then_body);
            dedupe_structured_region_entry_labels_in_place(else_body);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } | HirStmt::For { body, .. } => {
            dedupe_structured_region_entry_labels_in_place(body);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                if case.body.len() >= 2 {
                    if let HirStmt::Label(outer) = case.body[0].clone() {
                        let inner_matches = match &mut case.body[1] {
                            HirStmt::While { body, .. }
                            | HirStmt::DoWhile { body, .. }
                            | HirStmt::For { body, .. } => {
                                dedupe_structured_region_entry_labels_in_place(body);
                                first_meaningful_label(body) == Some(outer.as_str())
                            }
                            _ => false,
                        };
                        if inner_matches {
                            case.body.remove(0);
                        }
                    }
                }
                dedupe_structured_region_entry_labels_in_place(&mut case.body);
            }
            dedupe_structured_region_entry_labels_in_place(default);
        }
        _ => {}
    }
}

/// True when `child_body` is a single loop whose body already begins with `label`.
pub fn child_body_has_entry_label(child_body: &[HirStmt], label: &str) -> bool {
    child_body.iter().any(|stmt| match stmt {
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } | HirStmt::For { body, .. } => {
            first_meaningful_label(body) == Some(label)
        }
        _ => false,
    })
}

/// Returns true if the body contains any `Goto(label)` whose corresponding
/// `Label(label)` is absent from the body.  Such "orphan" gotos indicate
/// a structuring failure where a back-edge or cross-edge target was referenced
/// but the emitter never placed the matching label statement.
pub fn has_orphan_goto_labels(body: &[HirStmt]) -> bool {
    !orphan_goto_labels(body).is_empty()
}

/// Label names referenced by `Goto` but absent from any `Label` declaration in `body`.
pub fn orphan_goto_labels(body: &[HirStmt]) -> Vec<String> {
    let goto_targets = collect_referenced_labels(body);
    if goto_targets.is_empty() {
        return Vec::new();
    }
    let declared = collect_declared_labels(body);
    let mut orphans: Vec<String> = goto_targets
        .into_iter()
        .filter(|label| !declared.contains(label))
        .collect();
    orphans.sort();
    orphans
}

/// Collects the set of label names that are *declared* (i.e. `Label(name)`)
/// anywhere in the body, recursing into nested statement blocks.
fn collect_declared_labels(body: &[HirStmt]) -> HashSet<String> {
    let mut declared = HashSet::default();
    for stmt in body {
        collect_stmt_declared_labels(stmt, &mut declared);
    }
    declared
}

fn collect_stmt_declared_labels(stmt: &HirStmt, declared: &mut HashSet<String>) {
    match stmt {
        HirStmt::Label(label) => {
            declared.insert(label.clone());
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for s in body {
                collect_stmt_declared_labels(s, declared);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    collect_stmt_declared_labels(s, declared);
                }
            }
            for s in default {
                collect_stmt_declared_labels(s, declared);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_stmt_declared_labels(s, declared);
            }
            for s in else_body {
                collect_stmt_declared_labels(s, declared);
            }
        }
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
}

// ---------------------------------------------------------------------------
// Existing label-cleanup utilities
// ---------------------------------------------------------------------------

pub fn cleanup_redundant_labels(
    body: Vec<HirStmt>,
    global_refs: Option<&std::collections::HashSet<String>>,
) -> Vec<HirStmt> {
    core_labels::cleanup_redundant_labels(body, global_refs)
}

pub fn normalize_guarded_tail_layout(
    body: Vec<HirStmt>,
    protected: &HashSet<String>,
) -> (Vec<HirStmt>, usize) {
    let cleaned = cleanup_redundant_labels_protecting(body, protected);
    let (canonicalized, rewritten_aliases) = canonicalize_top_level_forward_label_aliases(cleaned);
    let cleaned = cleanup_redundant_labels_protecting(canonicalized, protected);
    (cleaned, rewritten_aliases)
}

pub fn canonicalize_top_level_forward_label_aliases(
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
    let mut aliases = HashMap::default();
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
    let mut aliases = HashMap::default();
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
    let mut seen = HashSet::default();
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
    let mut referenced = HashSet::default();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub fn collect_referenced_label_counts(body: &[HirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
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

pub fn single_goto_target(body: &[HirStmt]) -> Option<&str> {
    match body {
        [HirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

pub fn has_top_level_label(body: &[HirStmt]) -> bool {
    body.iter().any(|stmt| matches!(stmt, HirStmt::Label(_)))
}

pub fn is_ignorable_discovery_stmt(stmt: &HirStmt) -> bool {
    matches!(stmt, HirStmt::Label(_)) || matches!(stmt, HirStmt::Block(body) if body.is_empty())
}

pub fn trim_ignorable_stmt_bounds(body: &[HirStmt]) -> Option<(usize, usize)> {
    let start = body
        .iter()
        .position(|stmt| !is_ignorable_discovery_stmt(stmt))?;
    let end = body
        .iter()
        .rposition(|stmt| !is_ignorable_discovery_stmt(stmt))
        .unwrap_or(start);
    Some((start, end + 1))
}

pub fn has_non_ignorable_payload(body: &[HirStmt]) -> bool {
    trim_ignorable_stmt_bounds(body).is_some()
}

#[cfg(test)]
mod tests {
    use fission_midend_core::ir::*;
    use super::*;

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
        assert_eq!(
            result.len(),
            1,
            "expected single If after inversion: {result:?}"
        );
        let HirStmt::If {
            else_body,
            then_body,
            ..
        } = &result[0]
        else {
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

        let (normalized, _) = normalize_guarded_tail_layout(body, &HashSet::default());
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

    #[test]
    fn orphan_goto_labels_detects_missing_declarations() {
        let body = vec![HirStmt::While {
            cond: HirExpr::Const(1, NirType::Bool),
            body: vec![HirStmt::Goto("block_140001890".to_string())],
        }];
        assert_eq!(
            orphan_goto_labels(&body),
            vec!["block_140001890".to_string()]
        );
        assert!(has_orphan_goto_labels(&body));
    }

    #[test]
    fn orphan_goto_labels_empty_when_all_targets_declared() {
        let body = vec![
            HirStmt::Label("block_140001890".to_string()),
            HirStmt::Return(None),
        ];
        assert!(orphan_goto_labels(&body).is_empty());
        assert!(!has_orphan_goto_labels(&body));
    }

    #[test]
    fn finalize_structured_body_inlines_single_predecessor_dead_forward_segment() {
        let body = vec![
            HirStmt::Goto("block_join".to_string()),
            HirStmt::Expr(HirExpr::Var("dead_unreachable".to_string())),
            HirStmt::Goto("block_join".to_string()),
            HirStmt::Label("block_join".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];

        let finalized = finalize_structured_body(&HashSet::default(), body);
        assert_eq!(
            finalized,
            vec![HirStmt::Return(Some(HirExpr::Var("ret".to_string())))]
        );
    }

    /// A block reachable only via a synthetic edge (e.g. a C++ exception
    /// landing pad -- see `StructuringHost::lsda_landing_pad_labels`) has
    /// no `Goto` anywhere in the text pointing at its label. Without
    /// protection, `finalize_structured_body` would treat it exactly like
    /// the genuinely-dead segment in the test above and delete it -- this
    /// pins that a `protected` label survives instead.
    #[test]
    fn finalize_structured_body_protects_unreferenced_landing_pad_label() {
        let body = vec![
            HirStmt::Goto("block_join".to_string()),
            HirStmt::Label("block_landing_pad".to_string()),
            HirStmt::Expr(HirExpr::Var("catch_handler_call".to_string())),
            HirStmt::Goto("block_join".to_string()),
            HirStmt::Label("block_join".to_string()),
            HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
        ];
        let protected: HashSet<String> = ["block_landing_pad".to_string()].into_iter().collect();

        let finalized = finalize_structured_body(&protected, body);
        // The landing pad's label survives (the actual point of this test).
        // The trailing `Goto("block_join")` -> `Label("block_join")` pair
        // collapses via ordinary "empty jump removal" -- unrelated to
        // protection, just `eliminate_redundant_gotos` doing its normal job.
        assert_eq!(
            finalized,
            vec![
                HirStmt::Goto("block_join".to_string()),
                HirStmt::Label("block_landing_pad".to_string()),
                HirStmt::Expr(HirExpr::Var("catch_handler_call".to_string())),
                HirStmt::Label("block_join".to_string()),
                HirStmt::Return(Some(HirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn cleanup_redundant_labels_protecting_keeps_unreferenced_protected_label() {
        // The label must NOT be the first statement (`cleaned.is_empty()`
        // already keeps a leading label unconditionally) -- placing an
        // unrelated statement first exercises the actual `referenced`/
        // `protected` check this test is pinning.
        let body = vec![
            HirStmt::Expr(HirExpr::Var("leading".to_string())),
            HirStmt::Label("block_unreferenced".to_string()),
            HirStmt::Return(None),
        ];
        let protected: HashSet<String> = ["block_unreferenced".to_string()].into_iter().collect();

        // Baseline: with no protection, an unreferenced mid-body label is
        // dropped by ordinary cleanup (matching `cleanup_redundant_labels`'s
        // behavior with `global_refs: None`).
        let unprotected = cleanup_redundant_labels_protecting(body.clone(), &HashSet::default());
        assert_eq!(
            unprotected,
            vec![
                HirStmt::Expr(HirExpr::Var("leading".to_string())),
                HirStmt::Return(None),
            ]
        );

        let protected_result = cleanup_redundant_labels_protecting(body, &protected);
        assert_eq!(
            protected_result,
            vec![
                HirStmt::Expr(HirExpr::Var("leading".to_string())),
                HirStmt::Label("block_unreferenced".to_string()),
                HirStmt::Return(None),
            ]
        );
    }

    #[test]
    fn dedupe_structured_region_entry_labels_removes_outer_loop_header_duplicate() {
        let body = vec![
            HirStmt::Label("block_140001890".to_string()),
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: vec![
                    HirStmt::Label("block_140001890".to_string()),
                    HirStmt::Assign {
                        lhs: HirLValue::Var("x".to_string()),
                        rhs: HirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    },
                ],
            },
        ];

        let deduped = dedupe_structured_region_entry_labels(body);
        assert_eq!(deduped.len(), 1);
        let HirStmt::While { body, .. } = &deduped[0] else {
            panic!("expected while");
        };
        assert_eq!(body.len(), 2);
        assert!(matches!(body[0], HirStmt::Label(ref l) if l == "block_140001890"));
    }

    #[test]
    fn guard_clause_promotion_converts_forward_goto_to_early_return() {
        // Pattern: if (n <= 0) { goto end }; loop_body; return sum; end: return 0
        // Expected: if (n <= 0) { return 0 }; loop_body; return sum
        let stmts = vec![
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Goto("block_end".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("sum".to_string()),
                rhs: HirExpr::Var("work".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Var("sum".to_string()))),
            HirStmt::Label("block_end".to_string()),
            HirStmt::Return(Some(HirExpr::Var("zero".to_string()))),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_string()),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Var("zero".to_string())))],
                    else_body: Vec::new(),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("sum".to_string()),
                    rhs: HirExpr::Var("work".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("sum".to_string()))),
            ]
        );
    }
}
