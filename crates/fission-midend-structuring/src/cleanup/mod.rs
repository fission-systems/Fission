use fission_midend_core::ir::{NirType};
use fission_midend_dir::{DirExpr, DirLValue, DirStmt, DirSwitchCase};
use fission_midend_dir::util::label_cleanup as core_labels;
use crate::HashMap;
use crate::HashSet;


mod goto;
pub use goto::eliminate_redundant_gotos;

/// `protected` labels are never removed by the cleanup passes below even
/// when nothing in `body` textually references them via `Goto` -- see
/// [`cleanup_redundant_labels_protecting`]. Pass `host.lsda_landing_pad_labels()`
/// (empty for the overwhelming majority of functions, which have no C++
/// exception handling at all).
pub fn finalize_structured_body(protected: &HashSet<String>, mut body: Vec<DirStmt>) -> Vec<DirStmt> {
    body = eliminate_redundant_gotos(body);
    body = dedupe_structured_region_entry_labels(body);
    // Secondary multi-emit guard (belt-and-suspenders after exclusive emission
    // + TraceDAG virtualization). Prefer graph-native contracts; these only
    // fire when residual still re-hosts absorbed labels.
    body = strip_post_total_infloop_label_residuals(body);
    body = strip_duplicate_label_definitions(body);
    body = cleanup_redundant_labels_protecting(body, protected);
    let referenced = collect_referenced_labels(&body);
    while matches!(body.first(), Some(DirStmt::Label(label)) if !referenced.contains(label) && !protected.contains(label))
    {
        body.remove(0);
    }
    body
}

/// C labels are function-scoped. Keep the first `Label(L)` and drop later
/// definitions of the same name (multi-emit of the same CFG block header).
pub fn strip_duplicate_label_definitions(mut body: Vec<DirStmt>) -> Vec<DirStmt> {
    let mut seen = HashSet::default();
    strip_duplicate_label_definitions_in_place(&mut body, &mut seen);
    body
}

fn strip_duplicate_label_definitions_in_place(
    stmts: &mut Vec<DirStmt>,
    seen: &mut HashSet<String>,
) {
    let mut i = 0;
    while i < stmts.len() {
        match &mut stmts[i] {
            DirStmt::Label(label) => {
                if !seen.insert(label.clone()) {
                    stmts.remove(i);
                    continue;
                }
            }
            DirStmt::Block(body) => strip_duplicate_label_definitions_in_place(body, seen),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                strip_duplicate_label_definitions_in_place(then_body, seen);
                strip_duplicate_label_definitions_in_place(else_body, seen);
            }
            DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                strip_duplicate_label_definitions_in_place(body, seen);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    strip_duplicate_label_definitions_in_place(&mut case.body, seen);
                }
                strip_duplicate_label_definitions_in_place(default, seen);
            }
            _ => {}
        }
        i += 1;
    }
}

/// After `while (1) { …; if (c) break; else continue; }` (total exit), drop
/// following siblings that only re-host labels already defined inside that
/// while — classic multi-emit of nested loop bodies after the outer infloop
/// already absorbed them.
pub fn strip_post_total_infloop_label_residuals(mut body: Vec<DirStmt>) -> Vec<DirStmt> {
    strip_post_total_infloop_label_residuals_in_place(&mut body);
    body
}

fn strip_post_total_infloop_label_residuals_in_place(stmts: &mut Vec<DirStmt>) {
    let mut i = 0;
    while i < stmts.len() {
        // Recurse into nested compounds first.
        match &mut stmts[i] {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                strip_post_total_infloop_label_residuals_in_place(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                strip_post_total_infloop_label_residuals_in_place(then_body);
                strip_post_total_infloop_label_residuals_in_place(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    strip_post_total_infloop_label_residuals_in_place(&mut case.body);
                }
                strip_post_total_infloop_label_residuals_in_place(default);
            }
            _ => {}
        }

        if let DirStmt::While { cond, body } = &stmts[i] {
            if is_constant_true_cond(cond) && while_body_ends_with_total_break_continue(body) {
                let inner_labels = collect_defined_labels(body);
                if !inner_labels.is_empty() {
                    if let Some(end) =
                        residual_suffix_end_after_total_infloop(stmts, i + 1, &inner_labels)
                    {
                        stmts.drain(i + 1..end);
                    }
                }
            }
        }
        i += 1;
    }
}

/// Largest `from..end` suffix (stopping before Return) whose only label traffic
/// is to `allowed`. Includes unlabeled multi-emitted compute (do-while bodies)
/// when later gotos re-enter the absorbed region.
fn residual_suffix_end_after_total_infloop(
    stmts: &[DirStmt],
    from: usize,
    allowed: &HashSet<String>,
) -> Option<usize> {
    if from >= stmts.len() {
        return None;
    }
    let mut end = from;
    while end < stmts.len() {
        if matches!(stmts[end], DirStmt::Return(_)) {
            break;
        }
        end += 1;
    }
    if end == from {
        return None;
    }
    let suffix = &stmts[from..end];
    let mut defined = HashSet::default();
    let mut gotos = HashSet::default();
    collect_defined_labels_in(suffix, &mut defined);
    collect_goto_targets_in(suffix, &mut gotos);
    if !defined.is_subset(allowed) || !gotos.is_subset(allowed) {
        return None;
    }
    // Require some re-entry into the absorbed region; bare assigns after a loop
    // are not residual multi-emit.
    if defined.is_empty() && gotos.is_empty() {
        return None;
    }
    if !defined.is_empty() && !defined.is_subset(allowed) {
        return None;
    }
    // Prefer evidence of jumping back into the region, or re-defining its labels.
    if gotos.is_empty() && defined.is_disjoint(allowed) {
        return None;
    }
    Some(end)
}

fn is_constant_true_cond(cond: &DirExpr) -> bool {
    matches!(
        cond,
        DirExpr::Const(v, _) if *v != 0
    )
}

/// Last meaningful control at the end of a while body is `if (…) break; else continue`
/// or the reverse (possibly after trailing labels).
fn while_body_ends_with_total_break_continue(body: &[DirStmt]) -> bool {
    let Some(last) = body.iter().rev().find(|s| !matches!(s, DirStmt::Label(_))) else {
        return false;
    };
    match last {
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let then_bc = is_solo_break_or_continue(then_body);
            let else_bc = is_solo_break_or_continue(else_body);
            // One arm break, the other continue — no fall-through past the loop.
            matches!(
                (then_bc, else_bc),
                (Some(true), Some(false)) | (Some(false), Some(true))
            )
        }
        _ => false,
    }
}

/// `Some(true)` = only Break, `Some(false)` = only Continue, `None` = other.
fn is_solo_break_or_continue(body: &[DirStmt]) -> Option<bool> {
    let meaningful: Vec<&DirStmt> = body
        .iter()
        .filter(|s| !matches!(s, DirStmt::Label(_)))
        .collect();
    if meaningful.len() != 1 {
        return None;
    }
    match meaningful[0] {
        DirStmt::Break => Some(true),
        DirStmt::Continue => Some(false),
        _ => None,
    }
}

fn collect_defined_labels(stmts: &[DirStmt]) -> HashSet<String> {
    let mut out = HashSet::default();
    collect_defined_labels_in(stmts, &mut out);
    out
}

fn collect_defined_labels_in(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Label(l) => {
                out.insert(l.clone());
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => collect_defined_labels_in(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defined_labels_in(then_body, out);
                collect_defined_labels_in(else_body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defined_labels_in(&case.body, out);
                }
                collect_defined_labels_in(default, out);
            }
            _ => {}
        }
    }
}

fn collect_goto_targets_in(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Goto(l) => {
                out.insert(l.clone());
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => collect_goto_targets_in(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_goto_targets_in(then_body, out);
                collect_goto_targets_in(else_body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_goto_targets_in(&case.body, out);
                }
                collect_goto_targets_in(default, out);
            }
            _ => {}
        }
    }
}


/// Like [`cleanup_redundant_labels`], but additionally protects every label
/// in `protected` from removal even when nothing in `body` textually
/// references it via `Goto` -- for labels reachable only via an edge with
/// no `DirStmt` representation at all (see
/// `StructuringHost::lsda_landing_pad_labels`). Ordinary label cleanup
/// (`referenced.contains`) is exactly right for real dead labels; it just
/// has no way to know a C++ exception landing pad's label is a live entry
/// point when nothing in the text ever does `Goto` to it.
pub fn cleanup_redundant_labels_protecting(
    body: Vec<DirStmt>,
    protected: &HashSet<String>,
) -> Vec<DirStmt> {
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
pub fn dedupe_structured_region_entry_labels(mut body: Vec<DirStmt>) -> Vec<DirStmt> {
    dedupe_structured_region_entry_labels_in_place(&mut body);
    body
}

fn first_meaningful_label(stmts: &[DirStmt]) -> Option<&str> {
    stmts.iter().find_map(|stmt| {
        if let DirStmt::Label(label) = stmt {
            Some(label.as_str())
        } else {
            None
        }
    })
}

fn dedupe_structured_region_entry_labels_in_place(stmts: &mut Vec<DirStmt>) {
    let mut i = 0;
    while i < stmts.len() {
        if let DirStmt::Label(outer) = stmts[i].clone() {
            if i + 1 < stmts.len() {
                let inner_matches = match &mut stmts[i + 1] {
                    DirStmt::While { body, .. }
                    | DirStmt::DoWhile { body, .. }
                    | DirStmt::For { body, .. } => {
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

fn dedupe_structured_region_entry_labels_stmt(stmt: &mut DirStmt) {
    match stmt {
        DirStmt::Block(body) => dedupe_structured_region_entry_labels_in_place(body),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            dedupe_structured_region_entry_labels_in_place(then_body);
            dedupe_structured_region_entry_labels_in_place(else_body);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } | DirStmt::For { body, .. } => {
            dedupe_structured_region_entry_labels_in_place(body);
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                if case.body.len() >= 2 {
                    if let DirStmt::Label(outer) = case.body[0].clone() {
                        let inner_matches = match &mut case.body[1] {
                            DirStmt::While { body, .. }
                            | DirStmt::DoWhile { body, .. }
                            | DirStmt::For { body, .. } => {
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
pub fn child_body_has_entry_label(child_body: &[DirStmt], label: &str) -> bool {
    child_body.iter().any(|stmt| match stmt {
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } | DirStmt::For { body, .. } => {
            first_meaningful_label(body) == Some(label)
        }
        _ => false,
    })
}

/// Returns true if the body contains any `Goto(label)` whose corresponding
/// `Label(label)` is absent from the body.  Such "orphan" gotos indicate
/// a structuring failure where a back-edge or cross-edge target was referenced
/// but the emitter never placed the matching label statement.
pub fn has_orphan_goto_labels(body: &[DirStmt]) -> bool {
    !orphan_goto_labels(body).is_empty()
}

/// Label names referenced by `Goto` but absent from any `Label` declaration in `body`.
pub fn orphan_goto_labels(body: &[DirStmt]) -> Vec<String> {
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
fn collect_declared_labels(body: &[DirStmt]) -> HashSet<String> {
    let mut declared = HashSet::default();
    for stmt in body {
        collect_stmt_declared_labels(stmt, &mut declared);
    }
    declared
}

fn collect_stmt_declared_labels(stmt: &DirStmt, declared: &mut HashSet<String>) {
    match stmt {
        DirStmt::Label(label) => {
            declared.insert(label.clone());
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for s in body {
                collect_stmt_declared_labels(s, declared);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    collect_stmt_declared_labels(s, declared);
                }
            }
            for s in default {
                collect_stmt_declared_labels(s, declared);
            }
        }
        DirStmt::If {
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
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Goto(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

// ---------------------------------------------------------------------------
// Existing label-cleanup utilities
// ---------------------------------------------------------------------------

pub fn cleanup_redundant_labels(
    body: Vec<DirStmt>,
    global_refs: Option<&std::collections::HashSet<String>>,
) -> Vec<DirStmt> {
    core_labels::cleanup_redundant_labels(body, global_refs)
}

pub fn normalize_guarded_tail_layout(
    body: Vec<DirStmt>,
    protected: &HashSet<String>,
) -> (Vec<DirStmt>, usize) {
    let cleaned = cleanup_redundant_labels_protecting(body, protected);
    let (canonicalized, rewritten_aliases) = canonicalize_top_level_forward_label_aliases(cleaned);
    let cleaned = cleanup_redundant_labels_protecting(canonicalized, protected);
    (cleaned, rewritten_aliases)
}

pub fn canonicalize_top_level_forward_label_aliases(
    body: Vec<DirStmt>,
) -> (Vec<DirStmt>, usize) {
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
    body: &[DirStmt],
) -> (HashMap<String, String>, Vec<(usize, usize)>) {
    let mut aliases = HashMap::default();
    let mut ranges = Vec::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let DirStmt::Label(alias_label) = &body[idx] else {
            idx += 1;
            continue;
        };
        let next_label_idx =
            (idx + 1..body.len()).find(|pos| matches!(body[*pos], DirStmt::Label(_)));
        let Some(next_label_idx) = next_label_idx else {
            idx += 1;
            continue;
        };
        let DirStmt::Label(next_label) = &body[next_label_idx] else {
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

fn is_top_level_forward_alias_segment(segment: &[DirStmt], next_label: &str) -> bool {
    let mut saw_forward_goto = false;
    for stmt in segment {
        if is_ignorable_discovery_stmt(stmt) {
            continue;
        }
        match stmt {
            DirStmt::Goto(label) if !saw_forward_goto && label == next_label => {
                saw_forward_goto = true;
            }
            _ => return false,
        }
    }
    saw_forward_goto
}

fn adjacent_label_aliases(body: &[DirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::default();
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
    let mut seen = HashSet::default();
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

fn collect_referenced_labels(body: &[DirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::default();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub fn collect_referenced_label_counts(body: &[DirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
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
        DirStmt::Assign { .. }
        | DirStmt::VaStart { .. }
        | DirStmt::Expr(_)
        | DirStmt::Label(_)
        | DirStmt::Return(_)
        | DirStmt::Break
        | DirStmt::Continue => {}
    }
}

pub fn single_goto_target(body: &[DirStmt]) -> Option<&str> {
    match body {
        [DirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

pub fn has_top_level_label(body: &[DirStmt]) -> bool {
    body.iter().any(|stmt| matches!(stmt, DirStmt::Label(_)))
}

pub fn is_ignorable_discovery_stmt(stmt: &DirStmt) -> bool {
    matches!(stmt, DirStmt::Label(_)) || matches!(stmt, DirStmt::Block(body) if body.is_empty())
}

pub fn trim_ignorable_stmt_bounds(body: &[DirStmt]) -> Option<(usize, usize)> {
    let start = body
        .iter()
        .position(|stmt| !is_ignorable_discovery_stmt(stmt))?;
    let end = body
        .iter()
        .rposition(|stmt| !is_ignorable_discovery_stmt(stmt))
        .unwrap_or(start);
    Some((start, end + 1))
}

pub fn has_non_ignorable_payload(body: &[DirStmt]) -> bool {
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
            DirStmt::Goto("exit".to_string()),
            DirStmt::Label("exit".to_string()),
            DirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![DirStmt::Label("exit".to_string()), DirStmt::Return(None)]
        );
    }

    #[test]
    fn goto_elim_removes_single_ref_label_and_goto_pair() {
        // Goto(L) immediately before Label(L) with a single reference → both removed.
        let stmts = vec![
            DirStmt::Goto("lbl".to_string()),
            DirStmt::Label("lbl".to_string()),
            DirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After empty-jump removal, Label(lbl) + Return remains.
        // Then single-ref inline removes both Goto and Label (they are adjacent).
        // The result should have no Goto and no Label.
        assert!(
            !result.iter().any(|s| matches!(s, DirStmt::Goto(_))),
            "goto should be eliminated: {result:?}"
        );
    }

    #[test]
    fn goto_elim_inverts_conditional_goto_followed_by_label() {
        // `if (cond) { Goto(L) }; Label(L); Return` →
        // `if (!cond) { Return }` (conditional inversion).
        let stmts = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("tail".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("tail".to_string()),
            DirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After inversion the Label should be gone and we should have a single If.
        assert_eq!(
            result.len(),
            1,
            "expected single If after inversion: {result:?}"
        );
        let DirStmt::If {
            else_body,
            then_body,
            ..
        } = &result[0]
        else {
            panic!("expected If: {result:?}");
        };
        assert!(else_body.is_empty(), "else should be empty: {result:?}");
        assert_eq!(then_body, &vec![DirStmt::Return(None)]);
    }

    #[test]
    fn normalize_guarded_tail_layout_collapses_adjacent_labels_before_alias_rewrite() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("block_alias_a".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("block_alias_a".to_string()),
            DirStmt::Label("block_alias_b".to_string()),
            DirStmt::Goto("block_tail".to_string()),
            DirStmt::Label("block_tail".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let (normalized, _) = normalize_guarded_tail_layout(body, &HashSet::default());
        assert_eq!(
            normalized,
            vec![
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_string()),
                    then_body: vec![DirStmt::Goto("block_tail".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Label("block_tail".to_string()),
                DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_rewrites_and_prunes_alias_segment() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("block_alias".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Label("block_alias".to_string()),
            DirStmt::Goto("block_tail".to_string()),
            DirStmt::Label("block_tail".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body);
        assert_eq!(rewritten, 1);
        assert_eq!(
            normalized,
            vec![
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_string()),
                    then_body: vec![DirStmt::Goto("block_tail".to_string())],
                    else_body: Vec::new(),
                },
                DirStmt::Label("block_tail".to_string()),
                DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_preserves_nontrivial_alias_payload() {
        let body = vec![
            DirStmt::Label("block_alias".to_string()),
            DirStmt::Expr(DirExpr::Var("work".to_string())),
            DirStmt::Goto("block_tail".to_string()),
            DirStmt::Label("block_tail".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body.clone());
        assert_eq!(rewritten, 0);
        assert_eq!(normalized, body);
    }

    #[test]
    fn orphan_goto_labels_detects_missing_declarations() {
        let body = vec![DirStmt::While {
            cond: DirExpr::Const(1, NirType::Bool),
            body: vec![DirStmt::Goto("block_140001890".to_string())],
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
            DirStmt::Label("block_140001890".to_string()),
            DirStmt::Return(None),
        ];
        assert!(orphan_goto_labels(&body).is_empty());
        assert!(!has_orphan_goto_labels(&body));
    }

    #[test]
    fn finalize_structured_body_inlines_single_predecessor_dead_forward_segment() {
        let body = vec![
            DirStmt::Goto("block_join".to_string()),
            DirStmt::Expr(DirExpr::Var("dead_unreachable".to_string())),
            DirStmt::Goto("block_join".to_string()),
            DirStmt::Label("block_join".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
        ];

        let finalized = finalize_structured_body(&HashSet::default(), body);
        assert_eq!(
            finalized,
            vec![DirStmt::Return(Some(DirExpr::Var("ret".to_string())))]
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
            DirStmt::Goto("block_join".to_string()),
            DirStmt::Label("block_landing_pad".to_string()),
            DirStmt::Expr(DirExpr::Var("catch_handler_call".to_string())),
            DirStmt::Goto("block_join".to_string()),
            DirStmt::Label("block_join".to_string()),
            DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
                DirStmt::Goto("block_join".to_string()),
                DirStmt::Label("block_landing_pad".to_string()),
                DirStmt::Expr(DirExpr::Var("catch_handler_call".to_string())),
                DirStmt::Label("block_join".to_string()),
                DirStmt::Return(Some(DirExpr::Var("ret".to_string()))),
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
            DirStmt::Expr(DirExpr::Var("leading".to_string())),
            DirStmt::Label("block_unreferenced".to_string()),
            DirStmt::Return(None),
        ];
        let protected: HashSet<String> = ["block_unreferenced".to_string()].into_iter().collect();

        // Baseline: with no protection, an unreferenced mid-body label is
        // dropped by ordinary cleanup (matching `cleanup_redundant_labels`'s
        // behavior with `global_refs: None`).
        let unprotected = cleanup_redundant_labels_protecting(body.clone(), &HashSet::default());
        assert_eq!(
            unprotected,
            vec![
                DirStmt::Expr(DirExpr::Var("leading".to_string())),
                DirStmt::Return(None),
            ]
        );

        let protected_result = cleanup_redundant_labels_protecting(body, &protected);
        assert_eq!(
            protected_result,
            vec![
                DirStmt::Expr(DirExpr::Var("leading".to_string())),
                DirStmt::Label("block_unreferenced".to_string()),
                DirStmt::Return(None),
            ]
        );
    }

    #[test]
    fn strip_duplicate_label_definitions_keeps_first_only() {
        let body = vec![
            DirStmt::Label("L".to_string()),
            DirStmt::Expr(DirExpr::Var("a".to_string())),
            DirStmt::While {
                cond: DirExpr::Const(1, NirType::Bool),
                body: vec![
                    DirStmt::Label("L".to_string()),
                    DirStmt::Continue,
                ],
            },
        ];
        let cleaned = strip_duplicate_label_definitions(body);
        let mut labels = 0usize;
        fn count(stmts: &[DirStmt], n: &mut usize) {
            for s in stmts {
                match s {
                    DirStmt::Label(_) => *n += 1,
                    DirStmt::While { body, .. } => count(body, n),
                    _ => {}
                }
            }
        }
        count(&cleaned, &mut labels);
        assert_eq!(labels, 1, "{cleaned:?}");
    }

    #[test]
    fn strip_post_total_infloop_removes_rehosted_label_residual() {
        // while(1) { L: …; if (c) break; else continue; }
        // do { work } while;   // unlabeled multi-emit body
        // goto L;
        let body = vec![
            DirStmt::While {
                cond: DirExpr::Const(1, NirType::Bool),
                body: vec![
                    DirStmt::Label("block_inner".to_string()),
                    DirStmt::Expr(DirExpr::Var("work".to_string())),
                    DirStmt::If {
                        cond: DirExpr::Var("done".to_string()),
                        then_body: vec![DirStmt::Break],
                        else_body: vec![DirStmt::Continue],
                    },
                ],
            },
            DirStmt::DoWhile {
                body: vec![DirStmt::Expr(DirExpr::Var("work".to_string()))],
                cond: DirExpr::Var("more".to_string()),
            },
            DirStmt::Goto("block_inner".to_string()),
            DirStmt::Return(None),
        ];
        let cleaned = strip_post_total_infloop_label_residuals(body);
        assert_eq!(cleaned.len(), 2, "{cleaned:?}");
        assert!(matches!(cleaned[0], DirStmt::While { .. }));
        assert!(matches!(cleaned[1], DirStmt::Return(None)));
    }

    #[test]
    fn dedupe_structured_region_entry_labels_removes_outer_loop_header_duplicate() {
        let body = vec![
            DirStmt::Label("block_140001890".to_string()),
            DirStmt::While {
                cond: DirExpr::Const(1, NirType::Bool),
                body: vec![
                    DirStmt::Label("block_140001890".to_string()),
                    DirStmt::Assign {
                        lhs: DirLValue::Var("x".to_string()),
                        rhs: DirExpr::Const(
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
        let DirStmt::While { body, .. } = &deduped[0] else {
            panic!("expected while");
        };
        assert_eq!(body.len(), 2);
        assert!(matches!(body[0], DirStmt::Label(ref l) if l == "block_140001890"));
    }

    #[test]
    fn guard_clause_promotion_converts_forward_goto_to_early_return() {
        // Pattern: if (n <= 0) { goto end }; loop_body; return sum; end: return 0
        // Expected: if (n <= 0) { return 0 }; loop_body; return sum
        let stmts = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("block_end".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("sum".to_string()),
                rhs: DirExpr::Var("work".to_string()),
            },
            DirStmt::Return(Some(DirExpr::Var("sum".to_string()))),
            DirStmt::Label("block_end".to_string()),
            DirStmt::Return(Some(DirExpr::Var("zero".to_string()))),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![
                DirStmt::If {
                    cond: DirExpr::Var("cond".to_string()),
                    then_body: vec![DirStmt::Return(Some(DirExpr::Var("zero".to_string())))],
                    else_body: Vec::new(),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("sum".to_string()),
                    rhs: DirExpr::Var("work".to_string()),
                },
                DirStmt::Return(Some(DirExpr::Var("sum".to_string()))),
            ]
        );
    }
}
