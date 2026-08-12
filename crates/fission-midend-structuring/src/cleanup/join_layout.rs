//! Join-block layout.
//!
//! A block reached from `P` predecessors can be lexically adjacent to at most
//! one of them, so at least `P - 1` of its in-edges must be written as a
//! jump. That makes `sum(P - 1)` a structural floor on the goto count for a
//! given CFG shape -- but only when every join block actually *uses* its one
//! free adjacency. A block whose predecessors all reach it by `goto` is
//! paying `P` jumps where `P - 1` would do:
//!
//! ```c
//! if (a) { ...; goto L; }
//! ...
//! return x;      // control cannot fall into L
//! L:
//! cleanup();
//! return y;
//! ```
//!
//! Relocating such a block to sit immediately after one of its predecessors
//! turns that predecessor's jump into a fall-through and retires it. The
//! others keep jumping to the label, which stays put with the block.
//!
//! Measured on the DecBench sample set, 416 of 1,781 remaining gotos (23.4%)
//! target a label with no adjacent predecessor, so this is purely a layout
//! loss rather than anything inherent to the control flow.
//!
//! # Reference owner
//!
//! Ghidra performs the same class of decision in `ActionFinalStructure`
//! (`blockaction.cc`), which calls `BlockGraph::orderBlocks()` -- sorting by
//! `FlowBlock::compareFinalOrder` -- *before* `markUnstructured()` decides
//! which edges become gotos. Layout is chosen first; jumps are whatever the
//! layout could not make adjacent. This pass applies that ordering principle
//! to the finalized AST, where the remaining residual placement lives.
//!
//! # Why the move is sound
//!
//! - Nothing falls *into* the block: every predecessor is a jump, which is
//!   exactly the admission condition. So deleting it from its old position
//!   removes no reachable path.
//! - Nothing falls *out of* it: the region is required to end in `Return` or
//!   `Goto`, so wherever it lands it terminates the enclosing sequence just
//!   as the jump it replaces did.
//! - The chosen predecessor's `goto` must be the last statement of its
//!   sequence, so replacing it with the block preserves that sequence's own
//!   exit behaviour.
//! - `Break`/`Continue` anywhere inside the region are refused: they bind to
//!   the nearest enclosing loop, which the move can change.

use fission_midend_prehir::PreHirStmt;
use crate::HashMap;
use crate::HashSet;

/// Largest region this pass will relocate, in statements (recursive count).
/// A move does not grow the output, but an unbounded one would re-indent an
/// arbitrary amount of code.
pub const MAX_REGION_STMTS: usize = 60;
/// Ceiling on relocations per function.
pub const MAX_APPLICATIONS: usize = 64;

/// Give jump-only join blocks their one free adjacency.
///
/// Returns the rewritten body and the number of `goto` statements removed.
pub fn relocate_jump_only_joins(
    mut body: Vec<PreHirStmt>,
    protected: &HashSet<String>,
) -> (Vec<PreHirStmt>, usize) {
    let mut removed = 0usize;
    let mut done: HashSet<String> = HashSet::default();
    for _ in 0..MAX_APPLICATIONS {
        match apply_one(&mut body, protected, &done) {
            Some(label) => {
                // Each label gets one relocation; re-picking it could shuttle
                // the same block between predecessors forever.
                done.insert(label);
                removed += 1;
            }
            None => break,
        }
    }
    (body, removed)
}

fn apply_one(
    body: &mut Vec<PreHirStmt>,
    protected: &HashSet<String>,
    done: &HashSet<String>,
) -> Option<String> {
    let refs = super::collect_referenced_label_counts(body);
    if refs.is_empty() {
        return None;
    }
    let mut definitions = HashMap::default();
    super::collect_defined_label_counts_in(body, &mut definitions);

    let mut candidates: Vec<String> = Vec::new();
    find_candidates(body, protected, &refs, &definitions, done, &mut candidates);

    for label in candidates {
        // Take the block out of its current home.
        let Some(region) = extract_region(body, &label) else {
            continue;
        };
        // Drop it in after one of the jumps that reaches it. The search runs
        // on the already-modified tree, so no index recorded before the
        // extraction can be stale.
        if splice_into_terminal_goto(body, &label, &region) {
            return Some(label);
        }
        // No usable predecessor after all -- put it back exactly where it was.
        restore_region(body, region);
    }
    None
}

/// A relocatable region: the label, its statements, and where they came from.
struct Region {
    path: Vec<u32>,
    stmts: Vec<PreHirStmt>,
}

/// Labels that are worth relocating: no adjacent predecessor, a region that
/// ends the path, and nothing inside that would rebind on the move.
fn find_candidates(
    stmts: &[PreHirStmt],
    protected: &HashSet<String>,
    refs: &HashMap<String, usize>,
    definitions: &HashMap<String, usize>,
    done: &HashSet<String>,
    out: &mut Vec<String>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        if let PreHirStmt::Label(label) = stmt {
            if !protected.contains(label)
                && !done.contains(label)
                && definitions.get(label).copied() == Some(1)
                && refs.get(label).copied().unwrap_or(0) >= 1
                // Nothing may fall into the label: at index 0 control enters
                // by entering the enclosing block, so only a preceding total
                // transfer proves it.
                && idx > 0
                && super::is_total_transfer(&stmts[idx - 1])
            {
                let end = region_end(stmts, idx);
                if region_is_relocatable(&stmts[idx + 1..end]) {
                    out.push(label.clone());
                }
            }
        }
        for nested in child_sequences(stmt) {
            find_candidates(nested, protected, refs, definitions, done, out);
        }
    }
}

/// One past the last statement belonging to the label at `idx`.
fn region_end(stmts: &[PreHirStmt], idx: usize) -> usize {
    stmts[idx + 1..]
        .iter()
        .position(|s| matches!(s, PreHirStmt::Label(_)))
        .map(|p| idx + 1 + p)
        .unwrap_or(stmts.len())
}

/// The region must end the path and hold nothing that rebinds when moved.
fn region_is_relocatable(region: &[PreHirStmt]) -> bool {
    if region.is_empty() {
        return false;
    }
    if !matches!(
        region.last(),
        Some(PreHirStmt::Return(_)) | Some(PreHirStmt::Goto(_))
    ) {
        return false;
    }
    let mut total = 0usize;
    for stmt in region {
        if rebinds_on_move(stmt) {
            return false;
        }
        total += statement_count(stmt);
        if total > MAX_REGION_STMTS {
            return false;
        }
    }
    true
}

/// `Break`/`Continue` bind to the nearest enclosing loop, which relocation
/// can change.
fn rebinds_on_move(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Break | PreHirStmt::Continue => true,
        _ => child_sequences(stmt)
            .into_iter()
            .any(|seq| seq.iter().any(rebinds_on_move)),
    }
}

fn statement_count(stmt: &PreHirStmt) -> usize {
    1 + child_sequences(stmt)
        .into_iter()
        .map(|seq| seq.iter().map(statement_count).sum::<usize>())
        .sum::<usize>()
}

/// Remove `Label(label)` and its statements, remembering where they were.
fn extract_region(body: &mut Vec<PreHirStmt>, label: &str) -> Option<Region> {
    let mut path = Vec::new();
    extract_in(body, label, &mut path)
}

fn extract_in(
    stmts: &mut Vec<PreHirStmt>,
    label: &str,
    path: &mut Vec<u32>,
) -> Option<Region> {
    if let Some(idx) = stmts
        .iter()
        .position(|s| matches!(s, PreHirStmt::Label(l) if l == label))
    {
        let end = region_end(stmts, idx);
        let mut here = path.clone();
        here.push(idx as u32);
        let taken: Vec<PreHirStmt> = stmts.drain(idx..end).collect();
        return Some(Region {
            path: here,
            stmts: taken,
        });
    }
    for idx in 0..stmts.len() {
        let mut children = child_sequences_mut(&mut stmts[idx]);
        for slot in 0..children.len() {
            path.push(idx as u32);
            path.push(slot as u32);
            if let Some(found) = extract_in(children[slot], label, path) {
                return Some(found);
            }
            path.pop();
            path.pop();
        }
    }
    None
}

/// Put an extracted region back at the position it came from.
fn restore_region(body: &mut Vec<PreHirStmt>, region: Region) {
    if let Some((seq, idx)) = resolve_mut(body, &region.path) {
        let at = idx.min(seq.len());
        seq.splice(at..at, region.stmts);
    }
}

/// Replace a `goto label` that ends its sequence with the region itself.
fn splice_into_terminal_goto(
    body: &mut Vec<PreHirStmt>,
    label: &str,
    region: &Region,
) -> bool {
    splice_in(body, label, &region.stmts)
}

fn splice_in(stmts: &mut Vec<PreHirStmt>, label: &str, region: &[PreHirStmt]) -> bool {
    // Only a jump that ends its sequence may be replaced: the region ends the
    // path, so the sequence keeps exactly the exit behaviour it had.
    if let Some(last) = stmts.len().checked_sub(1) {
        if matches!(&stmts[last], PreHirStmt::Goto(l) if l == label) {
            stmts.splice(last..last + 1, region.iter().cloned());
            return true;
        }
    }
    for idx in 0..stmts.len() {
        let mut children = child_sequences_mut(&mut stmts[idx]);
        for slot in 0..children.len() {
            if splice_in(children[slot], label, region) {
                return true;
            }
        }
    }
    false
}

fn resolve_mut<'a>(
    body: &'a mut Vec<PreHirStmt>,
    path: &[u32],
) -> Option<(&'a mut Vec<PreHirStmt>, usize)> {
    let (last, descent) = path.split_last()?;
    let mut seq: &mut Vec<PreHirStmt> = body;
    for pair in descent.chunks(2) {
        let [stmt_idx, slot] = pair else {
            return None;
        };
        let stmt = seq.get_mut(*stmt_idx as usize)?;
        let mut children = child_sequences_mut(stmt);
        if (*slot as usize) >= children.len() {
            return None;
        }
        seq = children.swap_remove(*slot as usize);
    }
    Some((seq, *last as usize))
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
    fn relocates_block_whose_predecessors_all_jump() {
        // if (a) { goto L; }
        // return x;        <- nothing falls into L
        // L:
        // cleanup();
        // return y;
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("x"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("cleanup"),
            PreHirStmt::Return(Some(var("y"))),
        ];
        let before = count_gotos(&body);
        let (out, removed) = relocate_jump_only_joins(body, &HashSet::default());
        assert_eq!(removed, 1);
        assert_eq!(count_gotos(&out), before - 1);
        // The block now sits inside the arm that used to jump to it.
        let PreHirStmt::If { then_body, .. } = &out[0] else {
            panic!("expected if, got {:?}", out[0]);
        };
        assert_eq!(
            then_body.as_slice(),
            &[
                PreHirStmt::Label("L".into()),
                expr_stmt("cleanup"),
                PreHirStmt::Return(Some(var("y"))),
            ]
        );
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn keeps_other_predecessors_jumping_to_the_moved_label() {
        // Two jumps reach L; one becomes a fall-through, the other still needs
        // its jump, so exactly one goto is retired.
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::If {
                cond: var("b"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("x"))),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("y"))),
        ];
        let before = count_gotos(&body);
        let (out, removed) = relocate_jump_only_joins(body, &HashSet::default());
        assert_eq!(removed, 1);
        assert_eq!(count_gotos(&out), before - 1, "exactly one jump retired");
        // The label survives for the predecessor that still jumps.
        let remaining = format!("{out:?}");
        assert!(remaining.contains("Label(\"L\")"));
    }

    #[test]
    fn refuses_block_reachable_by_fallthrough() {
        // Control falls into L, so it already spends its one free adjacency.
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            expr_stmt("falls_in"),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("y"))),
        ];
        let (out, removed) = relocate_jump_only_joins(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_region_that_falls_out() {
        // The region does not end the path, so moving it changes what runs
        // after it at the destination.
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("x"))),
            PreHirStmt::Label("L".into()),
            expr_stmt("no_terminator"),
        ];
        let (out, removed) = relocate_jump_only_joins(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_region_containing_break() {
        let body = vec![PreHirStmt::While {
            cond: var("w"),
            body: vec![
                PreHirStmt::If {
                    cond: var("a"),
                    then_body: vec![PreHirStmt::Goto("L".into())].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(var("x"))),
                PreHirStmt::Label("L".into()),
                PreHirStmt::Break,
            ]
            .into(),
        }];
        let (out, removed) = relocate_jump_only_joins(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_protected_label() {
        let protected: HashSet<String> = ["L".to_string()].into_iter().collect();
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into())].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("x"))),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("y"))),
        ];
        let (out, removed) = relocate_jump_only_joins(body.clone(), &protected);
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }

    #[test]
    fn refuses_when_no_predecessor_ends_its_sequence() {
        // The jump is not the last statement of its arm, so replacing it would
        // revive the statements behind it.
        let body = vec![
            PreHirStmt::If {
                cond: var("a"),
                then_body: vec![PreHirStmt::Goto("L".into()), expr_stmt("after")].into(),
                else_body: Vec::new().into(),
            },
            PreHirStmt::Return(Some(var("x"))),
            PreHirStmt::Label("L".into()),
            PreHirStmt::Return(Some(var("y"))),
        ];
        let (out, removed) = relocate_jump_only_joins(body.clone(), &HashSet::default());
        assert_eq!(removed, 0);
        assert_eq!(out, body);
    }
}
