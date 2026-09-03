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
//! 1. The region ends in a total transfer, so control provably never leaves
//!    it by falling through -- the copy is a complete substitute for the jump.
//!    A `Return` tail may be *cloned* into any number of sites. A `Goto` tail
//!    may only be *relocated* to a single site: cloning it would clone its
//!    trailing jump N ways and win nothing, so that case additionally requires
//!    the label to have exactly one reference and to be unreachable by
//!    fallthrough, making the rewrite a move that retires the incoming jump.
//! 2. The region contains no `Break`/`Continue` anywhere. Those bind to the
//!    nearest enclosing loop, and a `goto L` site can sit at a different loop
//!    nesting than `L` itself, so copying them could rebind them to a
//!    different loop.
//! 3. The region contains no `Label` anywhere -- a copied `Label` would be a
//!    duplicate definition, since C labels are function-scoped. It contains no
//!    `Goto` either in the cloning case, for the multiplication reason above;
//!    in the relocating case jumps move rather than multiply, so they are
//!    admissible.
//! 4. The region is at most [`MAX_TAIL_STMTS`] statements and is referenced
//!    by at most [`MAX_TAIL_REFS`] gotos, and the function stays within
//!    [`MAX_DUPLICATED_STMTS`] copied statements overall -- the growth bound
//!    the reference owners also impose.
//!
//! The original `Label(L)` and its region are removed only when the preceding
//! sibling proves ordinary control cannot fall into it *and* nothing targets
//! the label any more. That second condition is checked against freshly
//! recomputed reference counts, because a `Goto` inside newly spliced content
//! is skipped by the replacement scan and would otherwise be stranded.
//! Otherwise the label stays (one predecessor keeps the shared copy, exactly
//! as the reference owners duplicate into "each predecessor but one") and only
//! the `goto`s are replaced.

use crate::HashMap;
use crate::HashSet;
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

/// Longest tail this pass will copy, in statements (recursive count).
pub const MAX_TAIL_STMTS: usize = 6;
/// Most `goto`s to one label this pass will rewrite.
pub const MAX_TAIL_REFS: usize = 8;
/// Per-function ceiling on total copied statements.
///
/// Derived from what one admissible application costs rather than assumed:
/// the motivating shape is a tail of at most [`MAX_TAIL_STMTS`] copied into
/// the two arms of one `if`, so 12 statements buys one, and this buys two.
///
/// The previous 160 was thirteen of them, and nothing spent it: the cleanup
/// loop called this pass twice a round for up to eight rounds, each call
/// creating its own budget, so the "per-function" ceiling was really sixteen
/// ceilings. `sshbuf_fromb` reached eleven copies of a four-statement error
/// tail -- a path its binary reaches four times and its source shares once.
pub const MAX_DUPLICATED_STMTS: usize = 160;

/// Duplicate shared terminal tails into their `goto` sites.
///
/// Returns the rewritten body and the number of `goto` statements removed.
pub fn duplicate_terminal_tails(
    body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let mut budget = MAX_DUPLICATED_STMTS;
    duplicate_terminal_tails_within(body, protected, &mut budget)
}

/// [`duplicate_terminal_tails`], spending a budget the caller owns.
///
/// The growth bound is per *function*, and the cleanup loop calls this twice a
/// round for up to eight rounds -- so a budget created here was really sixteen
/// budgets, and the doc comment's "overall" was not true of anything. On
/// `sshbuf_fromb` that let one four-statement error tail reach eleven copies
/// where the binary itself has four.
pub fn duplicate_terminal_tails_within(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
    budget: &mut usize,
) -> (Vec<PreHirStmt>, usize) {
    let gotoless_budget = budget;
    let (next, split) = split_fallthrough_return_tails(body, gotoless_budget);
    body = next;
    let counts = super::collect_referenced_label_counts(&body);
    if counts.is_empty() {
        return (body, split);
    }
    let mut definitions = HashMap::default();
    super::collect_defined_label_counts_in(&body, &mut definitions);

    let mut candidates: Vec<(String, Vec<PreHirStmt>)> = Vec::new();
    collect_candidates(
        &body,
        protected,
        &counts,
        &definitions,
        &mut candidates,
        gotoless_budget,
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
    // Drop an original only when nothing targets its label any more. The
    // reference count must be *recomputed*: a `Goto` sitting inside freshly
    // spliced content is skipped by the replacement scan, so a label can still
    // have a live reference even though every reference present at collection
    // time was rewritten. Dropping on the stale assumption would strand that
    // jump. `finalize_*` would prune a bare label but not the statements
    // behind it, so this pass owns the removal.
    let remaining = super::collect_referenced_label_counts(&body);
    drop_unreachable_tail_definitions(&mut body, &plans, protected, &remaining);
    (body, removed)
}

/// Walk every statement sequence looking for `Label(L)` followed by a
/// duplicable terminal region.
/// Split a shared `return` tail into the arms of the `if` that falls into it.
///
/// [`duplicate_terminal_tails`] is goto-driven: it rewrites `goto L` sites and
/// needs a `Label` to key on. A join reached by *fallthrough* has neither, so
/// this is the gotoless case -- the one angr's `ReturnDuplicatorHigh` covers
/// and the doc comment above already names.
///
/// ```text
/// if (c) { A } else { B }        if (c) { A; return X; }
/// return X;                 =>   else   { B; return X; }
/// ```
///
/// The source usually returned from each branch and the compiler merged the
/// tails; the merged join is a node the source CFG has no counterpart for, so
/// leaving it costs a node and both of its in-edges.
///
/// Admission is the same shape the goto case demands, for the same reasons:
/// the tail must be relocatable (no `Break`/`Continue`/`Label`/`Goto`, which
/// would rebind or duplicate), must end the function, and must be small. Both
/// arms must exist and must fall through -- an arm that already returns has no
/// join to remove, and an empty arm is a negated guard whose "else" is the
/// rest of the function rather than a branch.
fn split_fallthrough_return_tails(
    body: Vec<PreHirStmt>,
    budget: &mut usize,
) -> (Vec<PreHirStmt>, usize) {
    let mut split = 0usize;
    let mut out: Vec<PreHirStmt> = Vec::with_capacity(body.len());
    let mut rest = body.into_iter().collect::<Vec<_>>();
    let mut idx = 0usize;

    while idx < rest.len() {
        // The `if` is often the last statement of a `Block` the structurer
        // emitted, which puts the shared tail one level out -- a sibling of
        // the block rather than of the `if`. Look through it.
        let is_candidate_if = last_if_is_splittable(&rest[idx]);
        if !is_candidate_if {
            let stmt = std::mem::replace(&mut rest[idx], PreHirStmt::Break);
            out.push(recurse_split(stmt, budget, &mut split));
            idx += 1;
            continue;
        }

        let tail: Vec<PreHirStmt> = rest[idx + 1..].to_vec();
        let cost: usize = tail.iter().map(statement_count).sum();
        let admissible = !tail.is_empty()
            && region_leaves(&tail)
            && region_is_relocatable(&tail, false)
            && cost <= MAX_TAIL_STMTS
            && cost <= *budget
            // Both arms take a copy, so this is the refs >= 2 case above.
            && region_free_reads(&tail) > 0;
        if !admissible {
            let stmt = std::mem::replace(&mut rest[idx], PreHirStmt::Break);
            out.push(recurse_split(stmt, budget, &mut split));
            idx += 1;
            continue;
        }

        let host = std::mem::replace(&mut rest[idx], PreHirStmt::Break);
        let Some(host) = append_tail_to_last_if(host, &tail) else {
            out.push(recurse_split(
                std::mem::replace(&mut rest[idx], PreHirStmt::Break),
                budget,
                &mut split,
            ));
            idx += 1;
            continue;
        };
        *budget = budget.saturating_sub(cost);
        split += 1;
        out.push(host);
        return (out, split);
    }

    (out, split)
}

/// Whether control provably leaves `region` rather than running off its end.
/// Whether `stmt` is -- or ends in -- an `if` whose arms both fall through.
fn last_if_is_splittable(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            !then_body.is_empty()
                && !else_body.is_empty()
                && !region_leaves(then_body)
                && !region_leaves(else_body)
        }
        PreHirStmt::Block(body) => body.last().is_some_and(last_if_is_splittable),
        _ => false,
    }
}

/// Append `tail` to both arms of the `if` [`last_if_is_splittable`] found.
fn append_tail_to_last_if(stmt: PreHirStmt, tail: &[PreHirStmt]) -> Option<PreHirStmt> {
    match stmt {
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let mut then_next = then_body.as_ref().clone();
            then_next.extend(tail.iter().cloned());
            let mut else_next = else_body.as_ref().clone();
            else_next.extend(tail.iter().cloned());
            Some(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(then_next),
                else_body: std::rc::Rc::new(else_next),
            })
        }
        PreHirStmt::Block(body) => {
            let mut inner = body.as_ref().clone();
            let last = inner.pop()?;
            inner.push(append_tail_to_last_if(last, tail)?);
            Some(PreHirStmt::Block(std::rc::Rc::new(inner)))
        }
        _ => None,
    }
}

fn region_leaves(region: &[PreHirStmt]) -> bool {
    match region.last() {
        Some(
            PreHirStmt::Return(_) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue,
        ) => true,
        Some(PreHirStmt::If {
            then_body,
            else_body,
            ..
        }) => {
            !then_body.is_empty()
                && !else_body.is_empty()
                && region_leaves(then_body)
                && region_leaves(else_body)
        }
        Some(PreHirStmt::Block(body)) => region_leaves(body),
        _ => false,
    }
}

fn recurse_split(stmt: PreHirStmt, budget: &mut usize, split: &mut usize) -> PreHirStmt {
    let mut stmt = stmt;
    for seq in child_sequences_mut(&mut stmt) {
        let taken = std::mem::take(seq);
        let (next, n) = split_fallthrough_return_tails(taken, budget);
        *seq = next;
        *split += n;
    }
    stmt
}

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
            // Whether ordinary control can fall into this label from its
            // preceding sibling. A goto-terminated region may only be threaded
            // when it cannot, because the original has to disappear for the
            // rewrite to retire a jump rather than clone one.
            let droppable = idx > 0 && super::is_total_transfer(&stmts[idx - 1]);
            if let Some(region) =
                duplicable_region_at(stmts, idx, label, protected, counts, definitions, droppable)
            {
                let refs = counts.get(label).copied().unwrap_or(0);
                // A tail that reads nothing from its context computes the same
                // thing wherever it is reached, so several sites reaching it is
                // what a tail the *source* shared looks like -- not a merge to
                // undo. LLVM decides the same question from the other side:
                // `sinkCommonCodeFromPredecessors` merges when two or more
                // unconditional predecessors need at most one PHI, and a
                // context-free tail needs none.
                if refs >= 2 && region_free_reads(&region) == 0 {
                    continue;
                }
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

/// Names a region reads without defining them first.
///
/// LLVM decides whether to *merge* a shared tail by asking how many PHI nodes
/// the merge would need (`NumPHIInsts <= 1` in `sinkCommonCodeFromPredecessors`)
/// -- that is, how many values differ between the predecessors. A tail that
/// needs none computes the same thing wherever it is reached, which is what a
/// tail the source itself shared looks like. This is the AST-level reading of
/// that question: a closed region depends on nothing from its context.
fn region_free_reads(region: &[PreHirStmt]) -> usize {
    let mut defined: HashSet<String> = HashSet::default();
    let mut free: HashSet<String> = HashSet::default();
    for stmt in region {
        collect_reads_and_defs(stmt, &mut defined, &mut free);
    }
    free.len()
}

fn collect_reads_and_defs(
    stmt: &PreHirStmt,
    defined: &mut HashSet<String>,
    free: &mut HashSet<String>,
) {
    let note_expr = |expr: &PreHirExpr, defined: &HashSet<String>, free: &mut HashSet<String>| {
        let mut seen: HashSet<String> = HashSet::default();
        collect_expr_names(expr, &mut seen);
        for name in seen {
            if !defined.contains(&name) {
                free.insert(name);
            }
        }
    };
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            note_expr(rhs, defined, free);
            match lhs {
                PreHirLValue::Var(name) => {
                    defined.insert(name.clone());
                }
                PreHirLValue::Deref { ptr, .. } => note_expr(ptr, defined, free),
                _ => {}
            }
        }
        PreHirStmt::Expr(expr) => note_expr(expr, defined, free),
        PreHirStmt::Return(Some(expr)) => note_expr(expr, defined, free),
        PreHirStmt::Block(inner) => {
            for s in inner.iter() {
                collect_reads_and_defs(s, defined, free);
            }
        }
        _ => {}
    }
}

fn collect_expr_names(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name) => {
            out.insert(name.clone());
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            collect_expr_names(expr, out)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_expr_names(lhs, out);
            collect_expr_names(rhs, out);
        }
        PreHirExpr::Load { ptr, .. } => collect_expr_names(ptr, out),
        PreHirExpr::PtrOffset { base, .. } => collect_expr_names(base, out),
        PreHirExpr::Call { args, .. } => {
            for a in args.iter() {
                collect_expr_names(a, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_names(cond, out);
            collect_expr_names(then_expr, out);
            collect_expr_names(else_expr, out);
        }
        _ => {}
    }
}

fn duplicable_region_at(
    stmts: &[PreHirStmt],
    idx: usize,
    label: &str,
    protected: &HashSet<String>,
    counts: &HashMap<String, usize>,
    definitions: &HashMap<String, usize>,
    droppable: bool,
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
    // A region that hands control on with its own `goto` is threadable, not
    // duplicable: relocating it retires the incoming jump only because the
    // original disappears. That needs the sole reference (so nothing else
    // still targets the label) and an unreachable original (so deleting it
    // loses no path). Cloning such a region N ways would instead clone its
    // trailing jump N ways and win nothing.
    let allow_goto_tail = refs == 1 && droppable;

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
        let terminal = match stmt {
            PreHirStmt::Return(_) => true,
            PreHirStmt::Goto(target) => {
                // Threading its own label back into itself would delete the
                // very jump the rewrite depends on finding.
                if !allow_goto_tail || target == label {
                    return None;
                }
                true
            }
            _ => false,
        };
        region.push(stmt.clone());
        if terminal {
            return region_is_relocatable(&region, allow_goto_tail).then_some(region);
        }
    }
    None
}

/// No `Break`/`Continue`/`Label` anywhere in the region -- see the admission
/// rules in the module docs for why each is disqualifying.
///
/// `Goto` is disqualifying only when the region will be *cloned*: copying a
/// jump N ways multiplies it. When the region is instead being relocated to
/// its sole reference (`allow_goto`), jumps inside it move rather than
/// multiply, so they are admissible.
fn region_is_relocatable(region: &[PreHirStmt], allow_goto: bool) -> bool {
    region
        .iter()
        .all(|stmt| stmt_is_relocatable(stmt, allow_goto))
}

fn stmt_is_relocatable(stmt: &PreHirStmt, allow_goto: bool) -> bool {
    match stmt {
        PreHirStmt::Break | PreHirStmt::Continue | PreHirStmt::Label(_) => false,
        PreHirStmt::Goto(_) => allow_goto,
        _ => child_sequences(stmt)
            .into_iter()
            .all(|seq| seq.iter().all(|s| stmt_is_relocatable(s, allow_goto))),
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
    remaining: &HashMap<String, usize>,
) {
    let mut idx = 0usize;
    while idx < stmts.len() {
        let removable = match &stmts[idx] {
            PreHirStmt::Label(label) => {
                !protected.contains(label)
                    && plans.contains_key(label.as_str())
                    && remaining.get(label.as_str()).copied().unwrap_or(0) == 0
                    && idx > 0
                    && super::is_total_transfer(&stmts[idx - 1])
            }
            _ => false,
        };
        if removable {
            // Re-derive the region's extent rather than trusting the length
            // recorded at collection time: `replace_gotos_in_place` may have
            // expanded a `Goto` *inside* this region into its own tail, so the
            // statement count behind the label can differ from the plan's.
            if let Some(end) = region_end_after(stmts, idx) {
                stmts.drain(idx..end);
                continue;
            }
        }
        for seq in child_sequences_mut(&mut stmts[idx]) {
            drop_unreachable_tail_definitions(seq, plans, protected, remaining);
        }
        idx += 1;
    }
}

/// One past the terminal statement of the region following the `Label` at
/// `idx`, or `None` when the region runs into another label or off the end
/// without terminating.
fn region_end_after(stmts: &[PreHirStmt], idx: usize) -> Option<usize> {
    for (offset, stmt) in stmts[idx + 1..].iter().enumerate() {
        match stmt {
            PreHirStmt::Label(_) => return None,
            PreHirStmt::Return(_) | PreHirStmt::Goto(_) => return Some(idx + offset + 2),
            _ => {}
        }
    }
    None
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
    fn threads_single_reference_goto_terminated_region() {
        // if (c) { goto L; }
        // return a;      <- total transfer, so L is unreachable by fallthrough
        // L:
        // x;
        // goto M;
        // M:
        // return b;
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Goto("M".into()),
            PreHirStmt::Label("M".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];
        let (out, removed) = duplicate_terminal_tails(body, &HashSet::default());
        // L's region relocates to its sole reference and the original goes
        // away, so `goto L` and the original `goto M` collapse into one jump.
        // `Label(M)` survives: the relocated `goto M` still targets it, which
        // is exactly what the recomputed reference count protects.
        assert_eq!(removed, 2);
        assert!(matches!(&out[0], PreHirStmt::If { then_body, .. }
            if then_body.as_slice() == [expr_stmt("x"), PreHirStmt::Goto("M".into())]));
        assert!(!out.contains(&PreHirStmt::Label("L".into())));
        assert!(out.contains(&PreHirStmt::Label("M".into())));
        // Net effect: two jumps became one.
        assert_eq!(count_gotos(&out), 1);
    }

    fn count_gotos(stmts: &[PreHirStmt]) -> usize {
        stmts
            .iter()
            .map(|s| match s {
                PreHirStmt::Goto(_) => 1,
                other => child_sequences(other)
                    .into_iter()
                    .map(count_gotos)
                    .sum::<usize>(),
            })
            .sum()
    }

    #[test]
    fn refuses_goto_tail_when_label_is_reachable_by_fallthrough() {
        // Cloning here would clone the trailing jump instead of retiring one.
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            expr_stmt("falls_in"),
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Goto("M".into()),
            PreHirStmt::Label("M".into()),
        ];
        let (out, removed) = duplicate_terminal_tails(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_goto_tail_with_multiple_references() {
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: var("d"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Goto("M".into()),
            PreHirStmt::Label("M".into()),
        ];
        let (out, removed) = duplicate_terminal_tails(body.clone(), &HashSet::default());
        assert_eq!(removed, 0, "cloning a jump N ways wins nothing");
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_self_targeting_goto_tail() {
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Goto("L".into()),
        ];
        let (_, removed) = duplicate_terminal_tails(body, &HashSet::default());
        assert_eq!(removed, 0, "self-loop region must not be threaded");
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

    #[test]
    fn post_finalize_terminal_tail_catches_tail_exposed_by_first_duplication() {
        // L is not initially duplicable: two predecessors target it and its
        // region ends in `goto R`. The first pass duplicates R's return into
        // L, and finalization makes that new terminal shape canonical. A
        // second bounded pass can then duplicate L without relaxing any
        // admission rule.
        let body = vec![
            PreHirStmt::If {
                cond: var("c"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: var("d"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("a"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("x"),
            PreHirStmt::Goto("R".into()),
            PreHirStmt::Label("R".into()),
            PreHirStmt::Return(Some(var("b"))),
        ];

        let protected = HashSet::default();
        let (body, first_removed) = duplicate_terminal_tails(body, &protected);
        assert_eq!(first_removed, 1, "only the inner R tail is ready initially");
        assert_eq!(count_gotos(&body), 2);

        let body = super::super::finalize_structured_body(&protected, body);
        let (body, second_removed) = duplicate_terminal_tails(body, &protected);
        let body = super::super::finalize_structured_body(&protected, body);

        assert_eq!(second_removed, 2, "the newly terminal L tail is now ready");
        assert_eq!(count_gotos(&body), 0);
        assert!(!body.iter().any(|stmt| matches!(stmt, PreHirStmt::Label(_))));
        assert!(matches!(
            body.as_slice(),
            [
                PreHirStmt::If { then_body: first, .. },
                PreHirStmt::If { then_body: second, .. },
                PreHirStmt::Return(_),
            ] if first.as_slice() == [expr_stmt("x"), PreHirStmt::Return(Some(var("b")))]
                && second.as_slice() == first.as_slice()
        ));
    }
}
