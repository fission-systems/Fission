use fission_midend_core::ir::{NirType};
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt, PreHirSwitchCase};
use fission_midend_prehir::util::label_cleanup as core_labels;
use crate::HashMap;
use crate::HashSet;


mod goto;
pub use goto::eliminate_redundant_gotos;

mod tail_dup;
pub use tail_dup::duplicate_terminal_tails;

mod guard_invert;
pub use guard_invert::invert_forward_guard_gotos;

mod join_layout;
pub use join_layout::relocate_jump_only_joins;

/// `protected` labels are never removed by the cleanup passes below even
/// when nothing in `body` textually references them via `Goto` -- see
/// [`cleanup_redundant_labels_protecting`]. Pass `host.lsda_landing_pad_labels()`
/// (empty for the overwhelming majority of functions, which have no C++
/// exception handling at all).
pub fn finalize_structured_body(protected: &HashSet<String>, mut body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    body = eliminate_redundant_gotos(body);
    body = dedupe_structured_region_entry_labels(body);
    // Secondary multi-emit guard (belt-and-suspenders after exclusive emission
    // + TraceDAG virtualization). Prefer graph-native contracts; these only
    // fire when residual still re-hosts absorbed labels.
    body = strip_post_total_infloop_label_residuals(body);
    body = strip_duplicate_label_definitions(body);
    body = cleanup_redundant_labels_protecting(body, protected);
    let referenced = collect_referenced_labels(&body);
    while matches!(body.first(), Some(PreHirStmt::Label(label)) if !referenced.contains(label) && !protected.contains(label))
    {
        body.remove(0);
    }
    body
}

/// Remove `Label(alias); Goto(target)` from a sequential list when its
/// preceding sibling proves that ordinary control cannot fall into `alias`.
/// Goto rewriting is function-wide because C labels are function-scoped.
pub fn eliminate_nonfallthrough_label_aliases(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let mut definition_counts = HashMap::default();
    collect_defined_label_counts_in(&body, &mut definition_counts);

    let mut aliases = HashMap::default();
    collect_nonfallthrough_label_aliases(
        &body,
        protected,
        &definition_counts,
        &mut aliases,
    );
    if aliases.is_empty() {
        return (body, 0);
    }

    let discovered = aliases.clone();
    aliases.retain(|alias, _| !alias_chain_is_cyclic(alias, &discovered));
    if aliases.is_empty() {
        return (body, 0);
    }

    remove_nonfallthrough_alias_segments_in_place(&mut body, &aliases);
    let rewritten_count = aliases.len();
    (rewrite_stmt_labels(body, &aliases), rewritten_count)
}

fn collect_defined_label_counts_in(
    stmts: &[PreHirStmt],
    counts: &mut HashMap<String, usize>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(label) => *counts.entry(label.clone()).or_insert(0) += 1,
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_defined_label_counts_in(body, counts),
            PreHirStmt::If { then_body, else_body, .. } => {
                collect_defined_label_counts_in(then_body, counts);
                collect_defined_label_counts_in(else_body, counts);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defined_label_counts_in(&case.body, counts);
                }
                collect_defined_label_counts_in(default, counts);
            }
            _ => {}
        }
    }
}

fn collect_nonfallthrough_label_aliases(
    stmts: &[PreHirStmt],
    protected: &HashSet<String>,
    definition_counts: &HashMap<String, usize>,
    aliases: &mut HashMap<String, String>,
) {
    for window in stmts.windows(3) {
        let [previous, PreHirStmt::Label(alias), PreHirStmt::Goto(target)] = window else {
            continue;
        };
        if is_total_transfer(previous)
            && alias != target
            && !protected.contains(alias)
            && definition_counts.get(alias) == Some(&1)
            && definition_counts.contains_key(target)
        {
            aliases.insert(alias.clone(), target.clone());
        }
    }

    for stmt in stmts {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_nonfallthrough_label_aliases(
                body, protected, definition_counts, aliases,
            ),
            PreHirStmt::If { then_body, else_body, .. } => {
                collect_nonfallthrough_label_aliases(
                    then_body, protected, definition_counts, aliases,
                );
                collect_nonfallthrough_label_aliases(
                    else_body, protected, definition_counts, aliases,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_nonfallthrough_label_aliases(
                        &case.body, protected, definition_counts, aliases,
                    );
                }
                collect_nonfallthrough_label_aliases(
                    default, protected, definition_counts, aliases,
                );
            }
            _ => {}
        }
    }
}

fn is_total_transfer(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue
    )
}

fn alias_chain_is_cyclic(start: &str, aliases: &HashMap<String, String>) -> bool {
    let mut current = start;
    let mut seen = HashSet::default();
    while let Some(next) = aliases.get(current) {
        if !seen.insert(current.to_string()) {
            return true;
        }
        current = next;
    }
    false
}

fn remove_nonfallthrough_alias_segments_in_place(
    stmts: &mut Vec<PreHirStmt>,
    aliases: &HashMap<String, String>,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let (Some(PreHirStmt::Label(alias)), Some(PreHirStmt::Goto(target))) =
            (stmts.get(idx), stmts.get(idx + 1))
        {
            if aliases.get(alias) == Some(target) {
                stmts.drain(idx..idx + 2);
                continue;
            }
        }

        match &mut stmts[idx] {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => remove_nonfallthrough_alias_segments_in_place(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), aliases,
            ),
            PreHirStmt::If { then_body, else_body, .. } => {
                remove_nonfallthrough_alias_segments_in_place(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), aliases,
                );
                remove_nonfallthrough_alias_segments_in_place(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), aliases,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    remove_nonfallthrough_alias_segments_in_place(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body), aliases,
                    );
                }
                remove_nonfallthrough_alias_segments_in_place(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), aliases,
                );
            }
            _ => {}
        }
        idx += 1;
    }
}

/// C labels are function-scoped. Keep the first `Label(L)` and drop later
/// definitions of the same name (multi-emit of the same CFG block header).
pub fn strip_duplicate_label_definitions(mut body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    let mut seen = HashSet::default();
    strip_duplicate_label_definitions_in_place(&mut body, &mut seen);
    body
}

fn strip_duplicate_label_definitions_in_place(
    stmts: &mut Vec<PreHirStmt>,
    seen: &mut HashSet<String>,
) {
    let mut i = 0;
    while i < stmts.len() {
        match &mut stmts[i] {
            PreHirStmt::Label(label) => {
                if !seen.insert(label.clone()) {
                    stmts.remove(i);
                    continue;
                }
            }
            PreHirStmt::Block(body) => strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), seen),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body), seen);
                strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body), seen);
            }
            PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body), seen);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body), seen);
                }
                strip_duplicate_label_definitions_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default), seen);
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
pub fn strip_post_total_infloop_label_residuals(mut body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    strip_post_total_infloop_label_residuals_in_place(&mut body);
    body
}

fn strip_post_total_infloop_label_residuals_in_place(stmts: &mut Vec<PreHirStmt>) {
    let mut i = 0;
    while i < stmts.len() {
        // Recurse into nested compounds first.
        match &mut stmts[i] {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                strip_post_total_infloop_label_residuals_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                strip_post_total_infloop_label_residuals_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
                strip_post_total_infloop_label_residuals_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    strip_post_total_infloop_label_residuals_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
                }
                strip_post_total_infloop_label_residuals_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            }
            _ => {}
        }

        if let PreHirStmt::While { cond, body } = &stmts[i] {
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
    stmts: &[PreHirStmt],
    from: usize,
    allowed: &HashSet<String>,
) -> Option<usize> {
    if from >= stmts.len() {
        return None;
    }
    let mut end = from;
    while end < stmts.len() {
        if matches!(stmts[end], PreHirStmt::Return(_)) {
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

fn is_constant_true_cond(cond: &PreHirExpr) -> bool {
    matches!(
        cond,
        PreHirExpr::Const(v, _) if *v != 0
    )
}

/// Last meaningful control at the end of a while body is `if (…) break; else continue`
/// or the reverse (possibly after trailing labels).
fn while_body_ends_with_total_break_continue(body: &[PreHirStmt]) -> bool {
    let Some(last) = body.iter().rev().find(|s| !matches!(s, PreHirStmt::Label(_))) else {
        return false;
    };
    match last {
        PreHirStmt::If {
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
fn is_solo_break_or_continue(body: &[PreHirStmt]) -> Option<bool> {
    let meaningful: Vec<&PreHirStmt> = body
        .iter()
        .filter(|s| !matches!(s, PreHirStmt::Label(_)))
        .collect();
    if meaningful.len() != 1 {
        return None;
    }
    match meaningful[0] {
        PreHirStmt::Break => Some(true),
        PreHirStmt::Continue => Some(false),
        _ => None,
    }
}

/// Labels already defined in a structured region body (for exclusive emission).
pub fn collect_defined_labels(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut out = HashSet::default();
    collect_defined_labels_in(stmts, &mut out);
    out
}

fn collect_defined_labels_in(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Label(l) => {
                out.insert(l.clone());
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_defined_labels_in(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defined_labels_in(then_body, out);
                collect_defined_labels_in(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defined_labels_in(&case.body, out);
                }
                collect_defined_labels_in(default, out);
            }
            _ => {}
        }
    }
}

fn collect_goto_targets_in(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Goto(l) => {
                out.insert(l.clone());
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_goto_targets_in(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_goto_targets_in(then_body, out);
                collect_goto_targets_in(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
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
/// no `PreHirStmt` representation at all (see
/// `StructuringHost::lsda_landing_pad_labels`). Ordinary label cleanup
/// (`referenced.contains`) is exactly right for real dead labels; it just
/// has no way to know a C++ exception landing pad's label is a live entry
/// point when nothing in the text ever does `Goto` to it.
pub fn cleanup_redundant_labels_protecting(
    body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> Vec<PreHirStmt> {
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
pub fn dedupe_structured_region_entry_labels(mut body: Vec<PreHirStmt>) -> Vec<PreHirStmt> {
    dedupe_structured_region_entry_labels_in_place(&mut body);
    body
}

fn first_meaningful_label(stmts: &[PreHirStmt]) -> Option<&str> {
    stmts.iter().find_map(|stmt| {
        if let PreHirStmt::Label(label) = stmt {
            Some(label.as_str())
        } else {
            None
        }
    })
}

fn dedupe_structured_region_entry_labels_in_place(stmts: &mut Vec<PreHirStmt>) {
    let mut i = 0;
    while i < stmts.len() {
        if let PreHirStmt::Label(outer) = stmts[i].clone() {
            if i + 1 < stmts.len() {
                let inner_matches = match &mut stmts[i + 1] {
                    PreHirStmt::While { body, .. }
                    | PreHirStmt::DoWhile { body, .. }
                    | PreHirStmt::For { body, .. } => {
                        dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
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

fn dedupe_structured_region_entry_labels_stmt(stmt: &mut PreHirStmt) {
    match stmt {
        PreHirStmt::Block(body) => dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body)),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body));
            dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body));
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } | PreHirStmt::For { body, .. } => {
            dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                if case.body.len() >= 2 {
                    if let PreHirStmt::Label(outer) = case.body[0].clone() {
                        let inner_matches = match &mut std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body)[1] {
                            PreHirStmt::While { body, .. }
                            | PreHirStmt::DoWhile { body, .. }
                            | PreHirStmt::For { body, .. } => {
                                dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body));
                                first_meaningful_label(body) == Some(outer.as_str())
                            }
                            _ => false,
                        };
                        if inner_matches {
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body).remove(0);
                        }
                    }
                }
                dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            dedupe_structured_region_entry_labels_in_place(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
        }
        _ => {}
    }
}

/// True when `child_body` is a single loop whose body already begins with `label`.
pub fn child_body_has_entry_label(child_body: &[PreHirStmt], label: &str) -> bool {
    child_body.iter().any(|stmt| match stmt {
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } | PreHirStmt::For { body, .. } => {
            first_meaningful_label(body) == Some(label)
        }
        _ => false,
    })
}

/// Returns true if the body contains any `Goto(label)` whose corresponding
/// `Label(label)` is absent from the body.  Such "orphan" gotos indicate
/// a structuring failure where a back-edge or cross-edge target was referenced
/// but the emitter never placed the matching label statement.
pub fn has_orphan_goto_labels(body: &[PreHirStmt]) -> bool {
    !orphan_goto_labels(body).is_empty()
}

/// Label names referenced by `Goto` but absent from any `Label` declaration in `body`.
pub fn orphan_goto_labels(body: &[PreHirStmt]) -> Vec<String> {
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
fn collect_declared_labels(body: &[PreHirStmt]) -> HashSet<String> {
    let mut declared = HashSet::default();
    for stmt in body {
        collect_stmt_declared_labels(stmt, &mut declared);
    }
    declared
}

fn collect_stmt_declared_labels(stmt: &PreHirStmt, declared: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Label(label) => {
            declared.insert(label.clone());
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            for s in body.iter() {
                collect_stmt_declared_labels(s, declared);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    collect_stmt_declared_labels(s, declared);
                }
            }
            for s in default.iter() {
                collect_stmt_declared_labels(s, declared);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                collect_stmt_declared_labels(s, declared);
            }
            for s in else_body.iter() {
                collect_stmt_declared_labels(s, declared);
            }
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

// ---------------------------------------------------------------------------
// Existing label-cleanup utilities
// ---------------------------------------------------------------------------

pub fn cleanup_redundant_labels(
    body: Vec<PreHirStmt>,
    global_refs: Option<&std::collections::HashSet<String>>,
) -> Vec<PreHirStmt> {
    core_labels::cleanup_redundant_labels(body, global_refs)
}

pub fn normalize_guarded_tail_layout(
    body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let cleaned = cleanup_redundant_labels_protecting(body, protected);
    let (canonicalized, rewritten_aliases) = canonicalize_top_level_forward_label_aliases(cleaned);
    let cleaned = cleanup_redundant_labels_protecting(canonicalized, protected);
    (cleaned, rewritten_aliases)
}

pub fn canonicalize_top_level_forward_label_aliases(
    body: Vec<PreHirStmt>,
) -> (Vec<PreHirStmt>, usize) {
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
    body: &[PreHirStmt],
) -> (HashMap<String, String>, Vec<(usize, usize)>) {
    let mut aliases = HashMap::default();
    let mut ranges = Vec::new();
    let mut idx = 0usize;
    while idx < body.len() {
        let PreHirStmt::Label(alias_label) = &body[idx] else {
            idx += 1;
            continue;
        };
        let next_label_idx =
            (idx + 1..body.len()).find(|pos| matches!(body[*pos], PreHirStmt::Label(_)));
        let Some(next_label_idx) = next_label_idx else {
            idx += 1;
            continue;
        };
        let PreHirStmt::Label(next_label) = &body[next_label_idx] else {
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

fn is_top_level_forward_alias_segment(segment: &[PreHirStmt], next_label: &str) -> bool {
    let mut saw_forward_goto = false;
    for stmt in segment {
        if is_ignorable_discovery_stmt(stmt) {
            continue;
        }
        match stmt {
            PreHirStmt::Goto(label) if !saw_forward_goto && label == next_label => {
                saw_forward_goto = true;
            }
            _ => return false,
        }
    }
    saw_forward_goto
}

fn adjacent_label_aliases(body: &[PreHirStmt]) -> HashMap<String, String> {
    let mut aliases = HashMap::default();
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
    let mut seen = HashSet::default();
    while let Some(next) = aliases.get(&current) {
        if !seen.insert(current.clone()) {
            break;
        }
        current = next.clone();
    }
    current
}

fn rewrite_stmt_labels(body: Vec<PreHirStmt>, aliases: &HashMap<String, String>) -> Vec<PreHirStmt> {
    body.into_iter()
        .map(|stmt| rewrite_stmt_label(stmt, aliases))
        .collect()
}

fn rewrite_stmt_label(stmt: PreHirStmt, aliases: &HashMap<String, String>) -> PreHirStmt {
    match stmt {
        PreHirStmt::Block(body) => PreHirStmt::Block(std::rc::Rc::new(rewrite_stmt_labels(
            std::rc::Rc::unwrap_or_clone(body),
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
                    body: std::rc::Rc::new(rewrite_stmt_labels(
                        std::rc::Rc::unwrap_or_clone(case.body),
                        aliases,
                    )),
                })
                .collect(),
            default: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(default),
                aliases,
            )),
        },
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond,
            then_body: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(then_body),
                aliases,
            )),
            else_body: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(else_body),
                aliases,
            )),
        },
        PreHirStmt::While { cond, body } => PreHirStmt::While {
            cond,
            body: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(body),
                aliases,
            )),
        },
        PreHirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(body),
                aliases,
            )),
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
            body: std::rc::Rc::new(rewrite_stmt_labels(
                std::rc::Rc::unwrap_or_clone(body),
                aliases,
            )),
        },
        PreHirStmt::Label(label) => PreHirStmt::Label(canonicalize_label(&label, aliases)),
        PreHirStmt::Goto(label) => PreHirStmt::Goto(canonicalize_label(&label, aliases)),
        other => other,
    }
}

fn collect_referenced_labels(body: &[PreHirStmt]) -> HashSet<String> {
    let mut referenced = HashSet::default();
    for stmt in body {
        collect_stmt_referenced_labels(stmt, &mut referenced);
    }
    referenced
}

pub fn collect_referenced_label_counts(body: &[PreHirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
    for stmt in body {
        collect_stmt_referenced_label_counts(stmt, &mut counts);
    }
    counts
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
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => {}
    }
}

pub fn single_goto_target(body: &[PreHirStmt]) -> Option<&str> {
    match body {
        [PreHirStmt::Goto(label)] => Some(label.as_str()),
        _ => None,
    }
}

pub fn has_top_level_label(body: &[PreHirStmt]) -> bool {
    body.iter().any(|stmt| matches!(stmt, PreHirStmt::Label(_)))
}

pub fn is_ignorable_discovery_stmt(stmt: &PreHirStmt) -> bool {
    matches!(stmt, PreHirStmt::Label(_)) || matches!(stmt, PreHirStmt::Block(body) if body.is_empty())
}

pub fn trim_ignorable_stmt_bounds(body: &[PreHirStmt]) -> Option<(usize, usize)> {
    let start = body
        .iter()
        .position(|stmt| !is_ignorable_discovery_stmt(stmt))?;
    let end = body
        .iter()
        .rposition(|stmt| !is_ignorable_discovery_stmt(stmt))
        .unwrap_or(start);
    Some((start, end + 1))
}

pub fn has_non_ignorable_payload(body: &[PreHirStmt]) -> bool {
    trim_ignorable_stmt_bounds(body).is_some()
}

#[cfg(test)]
mod tests {
    use fission_midend_core::ir::*;
    use super::*;

    use super::*;

    #[test]
    fn nonfallthrough_aliases_retarget_function_scoped_gotos() {
        for transfer in [
            PreHirStmt::Return(None),
            PreHirStmt::Goto("escape".to_string()),
            PreHirStmt::Break,
            PreHirStmt::Continue,
        ] {
            let body = vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_string()),
                    then_body: vec![PreHirStmt::Goto("alias".to_string())].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::While {
                    cond: PreHirExpr::Const(1, NirType::Bool),
                    body: vec![
                        transfer,
                        PreHirStmt::Label("alias".to_string()),
                        PreHirStmt::Goto("target".to_string()),
                    ].into(),
                },
                PreHirStmt::Label("escape".to_string()),
                PreHirStmt::Label("target".to_string()),
                PreHirStmt::Return(None),
            ];

            let (rewritten, count) =
                eliminate_nonfallthrough_label_aliases(body, &HashSet::default());
            assert_eq!(count, 1);
            let PreHirStmt::If { then_body, .. } = &rewritten[0] else {
                panic!("expected if: {rewritten:?}");
            };
            assert_eq!(**then_body, vec![PreHirStmt::Goto("target".to_string())]);
            let PreHirStmt::While { body, .. } = &rewritten[1] else {
                panic!("expected while: {rewritten:?}");
            };
            assert_eq!(body.len(), 1, "alias pair should be removed: {rewritten:?}");
        }
    }

    #[test]
    fn nonfallthrough_aliases_keep_protected_and_unproven_shapes() {
        let cases = [
            vec![
                PreHirStmt::Return(None),
                PreHirStmt::Label("protected".to_string()),
                PreHirStmt::Goto("target".to_string()),
                PreHirStmt::Label("target".to_string()),
            ],
            vec![
                PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
                PreHirStmt::Label("fallthrough".to_string()),
                PreHirStmt::Goto("target".to_string()),
                PreHirStmt::Label("target".to_string()),
            ],
            vec![
                PreHirStmt::Return(None),
                PreHirStmt::Label("duplicate".to_string()),
                PreHirStmt::Goto("target".to_string()),
                PreHirStmt::Label("duplicate".to_string()),
                PreHirStmt::Label("target".to_string()),
            ],
            vec![
                PreHirStmt::Return(None),
                PreHirStmt::Label("undefined".to_string()),
                PreHirStmt::Goto("missing".to_string()),
            ],
            vec![
                PreHirStmt::Return(None),
                PreHirStmt::Label("self_alias".to_string()),
                PreHirStmt::Goto("self_alias".to_string()),
            ],
            vec![
                PreHirStmt::Return(None),
                PreHirStmt::Label("cycle_a".to_string()),
                PreHirStmt::Goto("cycle_b".to_string()),
                PreHirStmt::Return(None),
                PreHirStmt::Label("cycle_b".to_string()),
                PreHirStmt::Goto("cycle_a".to_string()),
            ],
        ];
        let protected = HashSet::from_iter(["protected".to_string()]);

        for body in cases {
            let (rewritten, count) =
                eliminate_nonfallthrough_label_aliases(body.clone(), &protected);
            assert_eq!(count, 0, "unexpected alias admission: {body:?}");
            assert_eq!(rewritten, body);
        }
    }

    #[test]
    fn goto_elim_removes_empty_jump_before_label() {
        let stmts = vec![
            PreHirStmt::Goto("exit".to_string()),
            PreHirStmt::Label("exit".to_string()),
            PreHirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![PreHirStmt::Label("exit".to_string()), PreHirStmt::Return(None)]
        );
    }

    #[test]
    fn goto_elim_removes_nested_block_fallthrough_to_enclosing_label() {
        let stmts = vec![
            PreHirStmt::Block(
                vec![
                    PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
                    PreHirStmt::Goto("join".to_string()),
                ]
                .into(),
            ),
            PreHirStmt::Label("join".to_string()),
            PreHirStmt::Return(None),
        ];

        let result = eliminate_redundant_gotos(stmts);
        let PreHirStmt::Block(body) = &result[0] else {
            panic!("expected nested block: {result:?}");
        };
        assert_eq!(
            **body,
            vec![PreHirStmt::Expr(PreHirExpr::Var("work".to_string()))]
        );
    }

    #[test]
    fn goto_elim_removes_if_arm_fallthrough_to_enclosing_label() {
        let stmts = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("join".to_string())].into(),
                else_body: vec![
                    PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
                    PreHirStmt::Goto("join".to_string()),
                ]
                .into(),
            },
            PreHirStmt::Label("join".to_string()),
            PreHirStmt::Return(None),
        ];

        let result = eliminate_redundant_gotos(stmts);
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = &result[0]
        else {
            panic!("expected if: {result:?}");
        };
        assert!(then_body.is_empty());
        assert_eq!(
            **else_body,
            vec![PreHirStmt::Expr(PreHirExpr::Var("work".to_string()))]
        );
    }

    #[test]
    fn post_layout_finalization_removes_fallthrough_created_by_guard_inversion() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("skip".to_string()),
                then_body: vec![PreHirStmt::Goto("join".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: PreHirExpr::Var("inner".to_string()),
                then_body: vec![
                    PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
                    PreHirStmt::Goto("join".to_string()),
                ]
                .into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("join".to_string()),
            PreHirStmt::Return(None),
        ];

        let (inverted, removed) = invert_forward_guard_gotos(body, &HashSet::default());
        assert_eq!(removed, 1);
        assert!(
            collect_referenced_labels(&inverted).contains("join"),
            "guard inversion should first create the nested fallthrough goto"
        );

        let finalized = finalize_structured_body(&HashSet::default(), inverted);
        assert!(
            !collect_referenced_labels(&finalized).contains("join"),
            "post-layout finalization should remove the nested fallthrough goto"
        );
    }

    #[test]
    fn goto_elim_keeps_loop_tail_goto_to_enclosing_label() {
        let stmts = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Var("cond".to_string()),
                body: vec![PreHirStmt::Goto("exit".to_string())].into(),
            },
            PreHirStmt::Label("exit".to_string()),
            PreHirStmt::Return(None),
        ];

        let result = eliminate_redundant_gotos(stmts);
        let PreHirStmt::While { body, .. } = &result[0] else {
            panic!("expected while: {result:?}");
        };
        assert_eq!(**body, vec![PreHirStmt::Goto("exit".to_string())]);
    }

    #[test]
    fn goto_elim_keeps_switch_case_tail_goto_to_enclosing_label() {
        let stmts = vec![
            PreHirStmt::Switch {
                expr: PreHirExpr::Var("selector".to_string()),
                cases: vec![PreHirSwitchCase {
                    values: vec![1],
                    body: vec![PreHirStmt::Goto("join".to_string())].into(),
                }],
                default: Vec::new().into(),
            },
            PreHirStmt::Label("join".to_string()),
            PreHirStmt::Return(None),
        ];

        let result = eliminate_redundant_gotos(stmts);
        let PreHirStmt::Switch { cases, .. } = &result[0] else {
            panic!("expected switch: {result:?}");
        };
        assert_eq!(
            *cases[0].body,
            vec![PreHirStmt::Goto("join".to_string())]
        );
    }

    #[test]
    fn goto_elim_removes_single_ref_label_and_goto_pair() {
        // Goto(L) immediately before Label(L) with a single reference → both removed.
        let stmts = vec![
            PreHirStmt::Goto("lbl".to_string()),
            PreHirStmt::Label("lbl".to_string()),
            PreHirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After empty-jump removal, Label(lbl) + Return remains.
        // Then single-ref inline removes both Goto and Label (they are adjacent).
        // The result should have no Goto and no Label.
        assert!(
            !result.iter().any(|s| matches!(s, PreHirStmt::Goto(_))),
            "goto should be eliminated: {result:?}"
        );
    }

    #[test]
    fn goto_elim_inverts_conditional_goto_followed_by_label() {
        // `if (cond) { Goto(L) }; Label(L); Return` →
        // `if (!cond) { Return }` (conditional inversion).
        let stmts = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("tail".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("tail".to_string()),
            PreHirStmt::Return(None),
        ];
        let result = eliminate_redundant_gotos(stmts);
        // After inversion the Label should be gone and we should have a single If.
        assert_eq!(
            result.len(),
            1,
            "expected single If after inversion: {result:?}"
        );
        let PreHirStmt::If {
            else_body,
            then_body,
            ..
        } = &result[0]
        else {
            panic!("expected If: {result:?}");
        };
        assert!(else_body.is_empty(), "else should be empty: {result:?}");
        assert_eq!(**then_body, vec![PreHirStmt::Return(None)]);
    }

    #[test]
    fn normalize_guarded_tail_layout_collapses_adjacent_labels_before_alias_rewrite() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("block_alias_a".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("block_alias_a".to_string()),
            PreHirStmt::Label("block_alias_b".to_string()),
            PreHirStmt::Goto("block_tail".to_string()),
            PreHirStmt::Label("block_tail".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        let (normalized, _) = normalize_guarded_tail_layout(body, &HashSet::default());
        assert_eq!(
            normalized,
            vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_string()),
                    then_body: vec![PreHirStmt::Goto("block_tail".to_string())].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Label("block_tail".to_string()),
                PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_rewrites_and_prunes_alias_segment() {
        let body = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("block_alias".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("block_alias".to_string()),
            PreHirStmt::Goto("block_tail".to_string()),
            PreHirStmt::Label("block_tail".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body);
        assert_eq!(rewritten, 1);
        assert_eq!(
            normalized,
            vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_string()),
                    then_body: vec![PreHirStmt::Goto("block_tail".to_string())].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Label("block_tail".to_string()),
                PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
            ]
        );
    }

    #[test]
    fn canonicalize_top_level_forward_aliases_preserves_nontrivial_alias_payload() {
        let body = vec![
            PreHirStmt::Label("block_alias".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
            PreHirStmt::Goto("block_tail".to_string()),
            PreHirStmt::Label("block_tail".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        let (normalized, rewritten) = canonicalize_top_level_forward_label_aliases(body.clone());
        assert_eq!(rewritten, 0);
        assert_eq!(normalized, body);
    }

    #[test]
    fn orphan_goto_labels_detects_missing_declarations() {
        let body = vec![PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: vec![PreHirStmt::Goto("block_140001890".to_string())].into(),
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
            PreHirStmt::Label("block_140001890".to_string()),
            PreHirStmt::Return(None),
        ];
        assert!(orphan_goto_labels(&body).is_empty());
        assert!(!has_orphan_goto_labels(&body));
    }

    #[test]
    fn finalize_structured_body_inlines_single_predecessor_dead_forward_segment() {
        let body = vec![
            PreHirStmt::Goto("block_join".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("dead_unreachable".to_string())),
            PreHirStmt::Goto("block_join".to_string()),
            PreHirStmt::Label("block_join".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
        ];

        let finalized = finalize_structured_body(&HashSet::default(), body);
        assert_eq!(
            finalized,
            vec![PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string())))]
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
            PreHirStmt::Goto("block_join".to_string()),
            PreHirStmt::Label("block_landing_pad".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("catch_handler_call".to_string())),
            PreHirStmt::Goto("block_join".to_string()),
            PreHirStmt::Label("block_join".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
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
                PreHirStmt::Goto("block_join".to_string()),
                PreHirStmt::Label("block_landing_pad".to_string()),
                PreHirStmt::Expr(PreHirExpr::Var("catch_handler_call".to_string())),
                PreHirStmt::Label("block_join".to_string()),
                PreHirStmt::Return(Some(PreHirExpr::Var("ret".to_string()))),
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
            PreHirStmt::Expr(PreHirExpr::Var("leading".to_string())),
            PreHirStmt::Label("block_unreferenced".to_string()),
            PreHirStmt::Return(None),
        ];
        let protected: HashSet<String> = ["block_unreferenced".to_string()].into_iter().collect();

        // Baseline: with no protection, an unreferenced mid-body label is
        // dropped by ordinary cleanup (matching `cleanup_redundant_labels`'s
        // behavior with `global_refs: None`).
        let unprotected = cleanup_redundant_labels_protecting(body.clone(), &HashSet::default());
        assert_eq!(
            unprotected,
            vec![
                PreHirStmt::Expr(PreHirExpr::Var("leading".to_string())),
                PreHirStmt::Return(None),
            ]
        );

        let protected_result = cleanup_redundant_labels_protecting(body, &protected);
        assert_eq!(
            protected_result,
            vec![
                PreHirStmt::Expr(PreHirExpr::Var("leading".to_string())),
                PreHirStmt::Label("block_unreferenced".to_string()),
                PreHirStmt::Return(None),
            ]
        );
    }

    #[test]
    fn strip_duplicate_label_definitions_keeps_first_only() {
        let body = vec![
            PreHirStmt::Label("L".to_string()),
            PreHirStmt::Expr(PreHirExpr::Var("a".to_string())),
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    PreHirStmt::Label("L".to_string()),
                    PreHirStmt::Continue,
                ].into(),
            },
        ];
        let cleaned = strip_duplicate_label_definitions(body);
        let mut labels = 0usize;
        fn count(stmts: &[PreHirStmt], n: &mut usize) {
            for s in stmts {
                match s {
                    PreHirStmt::Label(_) => *n += 1,
                    PreHirStmt::While { body, .. } => count(body, n),
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
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    PreHirStmt::Label("block_inner".to_string()),
                    PreHirStmt::Expr(PreHirExpr::Var("work".to_string())),
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("done".to_string()),
                        then_body: vec![PreHirStmt::Break].into(),
                        else_body: vec![PreHirStmt::Continue].into(),
                    },
                ].into(),
            },
            PreHirStmt::DoWhile {
                body: vec![PreHirStmt::Expr(PreHirExpr::Var("work".to_string()))].into(),
                cond: PreHirExpr::Var("more".to_string()),
            },
            PreHirStmt::Goto("block_inner".to_string()),
            PreHirStmt::Return(None),
        ];
        let cleaned = strip_post_total_infloop_label_residuals(body);
        assert_eq!(cleaned.len(), 2, "{cleaned:?}");
        assert!(matches!(cleaned[0], PreHirStmt::While { .. }));
        assert!(matches!(cleaned[1], PreHirStmt::Return(None)));
    }

    #[test]
    fn dedupe_structured_region_entry_labels_removes_outer_loop_header_duplicate() {
        let body = vec![
            PreHirStmt::Label("block_140001890".to_string()),
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    PreHirStmt::Label("block_140001890".to_string()),
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("x".to_string()),
                        rhs: PreHirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    },
                ].into(),
            },
        ];

        let deduped = dedupe_structured_region_entry_labels(body);
        assert_eq!(deduped.len(), 1);
        let PreHirStmt::While { body, .. } = &deduped[0] else {
            panic!("expected while");
        };
        assert_eq!(body.len(), 2);
        assert!(matches!(body[0], PreHirStmt::Label(ref l) if l == "block_140001890"));
    }

    #[test]
    fn duplicate_structured_label_keeps_unique_residual_statements() {
        let body = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![PreHirStmt::Label("block_residual".to_string())].into(),
            },
            PreHirStmt::Label("block_residual".to_string()),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("output".to_string()),
                rhs: PreHirExpr::Const(
                    7,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
        ];

        let deduped = strip_duplicate_label_definitions(body);
        assert_eq!(
            deduped
                .iter()
                .filter(|stmt| matches!(stmt, PreHirStmt::Label(label) if label == "block_residual"))
                .count(),
            0,
            "the nested definition owns the C label"
        );
        assert!(
            deduped.iter().any(
                |stmt| matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(name), .. } if name == "output")
            ),
            "deduplicating a label must not delete the residual block's statements"
        );
    }

    #[test]
    fn guard_clause_promotion_converts_forward_goto_to_early_return() {
        // Pattern: if (n <= 0) { goto end }; loop_body; return sum; end: return 0
        // Expected: if (n <= 0) { return 0 }; loop_body; return sum
        let stmts = vec![
            PreHirStmt::If {
                cond: PreHirExpr::Var("cond".to_string()),
                then_body: vec![PreHirStmt::Goto("block_end".to_string())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("sum".to_string()),
                rhs: PreHirExpr::Var("work".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("sum".to_string()))),
            PreHirStmt::Label("block_end".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Var("zero".to_string()))),
        ];
        let result = eliminate_redundant_gotos(stmts);
        assert_eq!(
            result,
            vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("cond".to_string()),
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("zero".to_string())))].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("sum".to_string()),
                    rhs: PreHirExpr::Var("work".to_string()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("sum".to_string()))),
            ]
        );
    }
}
