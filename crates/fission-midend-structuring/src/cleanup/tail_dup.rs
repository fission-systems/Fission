//! Shared terminal-tail duplication.
//!
//! A block that ends the function (a bare epilogue, a shared error/abort
//! handler, a small cleanup-and-return tail) is frequently reached from
//! several predecessors. Structuring emits its statements once and every
//! other predecessor reaches it with an explicit `goto`, even though the
//! tail is short and control never returns from it.
//!
//! Duplicating that tail into each `goto` site removes the jump without
//! changing behaviour: the tail is *terminal*, so replacing `goto L` with a
//! verbatim copy of L's statements executes exactly the statements that
//! `goto L` would have, and control ends in the same `return` either way.
//!
//! Reference owners for the same transform:
//!
//! - angr SAILR `ReturnDuplicatorHigh` (Basque et al., USENIX Security 2024)
//!   -- the *gotoless* duplicator, which splits a shared return block into
//!   its predecessors so early-return guards recover their source shape.
//! - Ghidra `ActionReturnSplit` -- the goto-driven epilogue splitter.
//! - kuna `ActionReturnDup` (`p8_structure/kuna_returndup.rs`) -- a port of
//!   the angr pass, bounded by a per-function split cap and an in-degree cap.
//!
//! Those owners duplicate at the CFG/p-code level before structuring. This
//! pass is the AST-level analog: it runs once on the finalized structured
//! body, so it cannot perturb builder state (type inference, materialization,
//! naming) the way a pre-structuring CFG rewrite would.
//!
//! # Admission
//!
//! A `Label(L)` region is duplicable only when all of the following hold.
//! Each is a structural fact about the emitted AST, not a heuristic:
//!
//! 1. The region ends in `Return`, so control provably never leaves it by
//!    falling through -- the copy is a complete substitute for the jump.
//! 2. The region contains no `Break`/`Continue` anywhere. Those bind to the
//!    nearest enclosing loop, and a `goto L` site can sit at a different loop
//!    nesting than `L` itself, so copying them could rebind them to a
//!    different loop.
//! 3. The region contains no `Label` or `Goto` anywhere. A copied `Label`
//!    would be a duplicate definition (C labels are function-scoped), and a
//!    copied `Goto` would multiply edges this pass has not proven terminal.
//! 4. The region is at most [`MAX_TAIL_STMTS`] statements and is referenced
//!    by at most [`MAX_TAIL_REFS`] gotos, and the function stays within
//!    [`MAX_DUPLICATED_STMTS`] copied statements overall -- the growth bound
//!    the reference owners also impose.
//!
//! The original `Label(L)` and its region are removed only when the preceding
//! sibling proves ordinary control cannot fall into it. Otherwise the label
//! stays (one predecessor keeps the shared copy, exactly as the reference
//! owners duplicate into "each predecessor but one") and only the `goto`s are
//! replaced.

use fission_midend_prehir::PreHirStmt;
use crate::HashMap;
use crate::HashSet;

/// Longest tail this pass will copy, in statements (recursive count).
pub const MAX_TAIL_STMTS: usize = 6;
/// Most `goto`s to one label this pass will rewrite.
pub const MAX_TAIL_REFS: usize = 8;
/// Per-function ceiling on total copied statements.
pub const MAX_DUPLICATED_STMTS: usize = 160;

/// Duplicate shared terminal tails into their `goto` sites.
///
/// Returns the rewritten body and the number of `goto` statements removed.
pub fn duplicate_terminal_tails(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let counts = super::collect_referenced_label_counts(&body);
    if counts.is_empty() {
        return (body, 0);
    }
    let mut definitions = HashMap::default();
    super::collect_defined_label_counts_in(&body, &mut definitions);

    let mut candidates: Vec<(String, Vec<PreHirStmt>)> = Vec::new();
    let mut budget = MAX_DUPLICATED_STMTS;
    collect_candidates(
        &body,
        protected,
        &counts,
        &definitions,
        &mut candidates,
        &mut budget,
    );
    if candidates.is_empty() {
        return (body, 0);
    }

    let plans: HashMap<String, Vec<PreHirStmt>> = candidates.into_iter().collect();
    let mut removed = 0usize;
    replace_gotos_in_place(&mut body, &plans, &mut removed);
    if removed == 0 {
        return (body, 0);
    }
    // A duplicated tail whose label is no longer reachable by fallthrough has
    // no remaining reference; drop the now-dead original. `finalize_*` would
    // also prune the bare label, but not the statements behind it.
    drop_unreachable_tail_definitions(&mut body, &plans, protected);
    (body, removed)
}

/// Walk every statement sequence looking for `Label(L)` followed by a
/// duplicable terminal region.
fn collect_candidates(
    stmts: &[PreHirStmt],
    protected: &HashSet<String>,
    counts: &HashMap<String, usize>,
    definitions: &HashMap<String, usize>,
    out: &mut Vec<(String, Vec<PreHirStmt>)>,
    budget: &mut usize,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        if let PreHirStmt::Label(label) = stmt {
            if let Some(region) = duplicable_region_at(stmts, idx, label, protected, counts, definitions)
            {
                let refs = counts.get(label).copied().unwrap_or(0);
                let cost = region.len().saturating_mul(refs);
                if cost <= *budget {
                    *budget -= cost;
                    out.push((label.clone(), region));
                }
            }
        }
        for nested in child_sequences(stmt) {
            collect_candidates(nested, protected, counts, definitions, out, budget);
        }
    }
}

/// The statements after `Label(L)` at `idx`, when they form a duplicable
/// terminal region under the admission rules in the module docs.
fn duplicable_region_at(
    stmts: &[PreHirStmt],
    idx: usize,
    label: &str,
    protected: &HashSet<String>,
    counts: &HashMap<String, usize>,
    definitions: &HashMap<String, usize>,
) -> Option<Vec<PreHirStmt>> {
    if protected.contains(label) {
        return None;
    }
    // A label defined more than once is already ambiguous; rewriting its
    // gotos would pick one definition arbitrarily.
    if definitions.get(label).copied().unwrap_or(0) != 1 {
        return None;
    }
    let refs = counts.get(label).copied().unwrap_or(0);
    if refs == 0 || refs > MAX_TAIL_REFS {
        return None;
    }

    let mut region: Vec<PreHirStmt> = Vec::new();
    let mut total = 0usize;
    for stmt in &stmts[idx + 1..] {
        if matches!(stmt, PreHirStmt::Label(_)) {
            // Reached the next label without terminating: control could fall
            // into it, so the region is not a complete substitute for a jump.
            return None;
        }
        total += statement_count(stmt);
        if total > MAX_TAIL_STMTS {
            return None;
        }
        let terminal = matches!(stmt, PreHirStmt::Return(_));
        region.push(stmt.clone());
        if terminal {
            return region_is_copyable(&region).then_some(region);
        }
    }
    None
}

/// No `Break`/`Continue`/`Goto`/`Label` anywhere in the region -- see the
/// admission rules in the module docs for why each is disqualifying.
fn region_is_copyable(region: &[PreHirStmt]) -> bool {
    region.iter().all(stmt_is_copyable)
}

fn stmt_is_copyable(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Break
        | PreHirStmt::Continue
        | PreHirStmt::Goto(_)
        | PreHirStmt::Label(_) => false,
        _ => child_sequences(stmt)
            .into_iter()
            .all(|seq| seq.iter().all(stmt_is_copyable)),
    }
}

fn statement_count(stmt: &PreHirStmt) -> usize {
    1 + child_sequences(stmt)
        .into_iter()
        .map(|seq| seq.iter().map(statement_count).sum::<usize>())
        .sum::<usize>()
}

fn child_sequences(stmt: &PreHirStmt) -> Vec<&[PreHirStmt]> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => vec![body.as_slice()],
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![then_body.as_slice(), else_body.as_slice()],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&[PreHirStmt]> =
                cases.iter().map(|case| case.body.as_slice()).collect();
            out.push(default.as_slice());
            out
        }
        _ => Vec::new(),
    }
}

/// Replace every `Goto(L)` for a planned label with a copy of its tail.
fn replace_gotos_in_place(
    stmts: &mut Vec<PreHirStmt>,
    plans: &HashMap<String, Vec<PreHirStmt>>,
    removed: &mut usize,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let PreHirStmt::Goto(target) = &stmts[idx] {
            if let Some(region) = plans.get(target.as_str()) {
                stmts.splice(idx..idx + 1, region.iter().cloned());
                *removed += 1;
                idx += region.len();
                continue;
            }
        }
        for seq in child_sequences_mut(&mut stmts[idx]) {
            replace_gotos_in_place(seq, plans, removed);
        }
        idx += 1;
    }
}

/// Remove `Label(L)` and its duplicated tail when ordinary control cannot
/// fall into the label.
fn drop_unreachable_tail_definitions(
    stmts: &mut Vec<PreHirStmt>,
    plans: &HashMap<String, Vec<PreHirStmt>>,
    protected: &HashSet<String>,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        let removable = match &stmts[idx] {
            PreHirStmt::Label(label) => {
                !protected.contains(label)
                    && plans.contains_key(label.as_str())
                    && idx > 0
                    && super::is_total_transfer(&stmts[idx - 1])
            }
            _ => false,
        };
        if removable {
            let PreHirStmt::Label(label) = &stmts[idx] else {
                unreachable!("checked above");
            };
            let region_len = plans[label.as_str()].len();
            // The label plus exactly the statements the plan copied: the
            // region was read from this position, so the lengths agree.
            let end = (idx + 1 + region_len).min(stmts.len());
            stmts.drain(idx..end);
            continue;
        }
        for seq in child_sequences_mut(&mut stmts[idx]) {
            drop_unreachable_tail_definitions(seq, plans, protected);
        }
        idx += 1;
    }
}

fn child_sequences_mut(stmt: &mut PreHirStmt) -> Vec<&mut Vec<PreHirStmt>> {
    match stmt {
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => {
            vec![std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body)]
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => vec![
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
        ],
        PreHirStmt::Switch { cases, default, .. } => {
            let mut out: Vec<&mut Vec<PreHirStmt>> = Vec::new();
            for case in cases.iter_mut() {
                out.push(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body));
            }
            out.push(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default));
            out
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::PreHirExpr;

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn expr_stmt(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(var(name))
    }

    #[test]
    fn duplicates_unreachable_shared_return_tail_and_drops_original() {
        // if (c) { goto L; }
        // return a;
        // L:
        // return b;
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body, &HashSet::default());
        assert_eq!(removed, 1);
        // The goto became the tail itself, and the now-unreachable original
        // label/region is gone.
        assert_eq!(
            out,
            vec![
                PreHirStmt::If {
                    cond: var("c"),
                    then_body: vec![PreHirStmt::Return(Some(var("b")))].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(var("a"))),
            ]
        );
    }

    #[test]
    fn keeps_original_when_label_is_reachable_by_fallthrough() {
        // if (c) { goto L; }
        // x;            <- falls into L
        // L:
        // return b;
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            expr_stmt("x"),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body, &HashSet::default());
        assert_eq!(removed, 1);
        // Tail copied into the goto site, original retained for fallthrough.
        assert!(matches!(&out[0], PreHirStmt::If { then_body, .. }
            if matches!(then_body.as_slice(), [PreHirStmt::Return(_)])));
        assert!(out.contains(&PreHirStmt::Label("L".into())));
        assert_eq!(out.last(), Some(&PreHirStmt::Return(Some(var("b")))));
    }

    #[test]
    fn refuses_region_containing_break() {
        // `break` binds to the enclosing loop, which differs between the
        // label site and the goto site.
        let body = vec![
            PreHirStmt::While {
                cond: var("c"),
                body: vec![PreHirStmt::Goto("L".into())].into(),
            },
            PreHirStmt::Label("L".into()),
            PreHirStmt::Break,
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_non_terminal_region() {
        // Region falls through into the next label instead of returning.
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Label("M".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_protected_label() {
        let protected: HashSet<String> = ["L".to_string()].into_iter().collect();
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body.clone(), &protected);
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn respects_reference_cap() {
        let mut body = vec![PreHirStmt::Return(Some(var("a")))];
        for _ in 0..(MAX_TAIL_REFS + 1) {
            body.insert(
                0,
                PreHirStmt::If {
                    cond: var("c"),
                    then_body: vec![PreHirStmt::Goto("L".into())].into(),
                    else_body: Vec::new().into(),
                },
            );
        }
        body.push(PreHirStmt::Label("L".into()));
        body.push(PreHirStmt::Return(Some(var("b"))));
        let (_, removed) = duplicate_terminal_tails(body, &HashSet::default());
        assert_eq!(removed, 0, "over-referenced tail must not be duplicated");
    }
}
