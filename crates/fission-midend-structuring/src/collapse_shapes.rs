//! Region discovery over a [`CollapseGraph`], as pure graph queries.
//!
//! These answer only "which nodes form a foldable region, and where is it
//! entered" -- they never lower statements, consult the host, or mutate
//! anything. That separation is deliberate: shape discovery is where a live
//! graph pays off, and keeping it side-effect free means it can be exercised
//! in isolation and, unlike the existing `try_lower_*` rules, cannot perturb
//! builder state by being *attempted*.
//!
//! Two properties distinguish this from the index-range scan it is meant to
//! replace:
//!
//! - A region is an arbitrary **set of nodes**, not a contiguous index range.
//!   The static driver could only ever accept `[entry, skip_to)`, which is
//!   why a collapse rule consuming past a SESE boundary was a real bug.
//! - Matching runs against the **current** graph, so a shape that is absent
//!   now can appear once a neighbouring region folds -- see
//!   [`CollapseGraph::check_single_entry`].
//!
//! The rule set follows Ghidra's `CollapseStructure` (`blockaction.cc`):
//! `ruleBlockCat`, `ruleBlockProperIf`, `ruleBlockIfElse`, `ruleBlockIfNoExit`,
//! `ruleBlockWhileDo`, `ruleBlockDoWhile`, and self-loops.

use crate::collapse_graph::{CollapseGraph, NodeId};

/// What a matched region will become once lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// `a; b` -- Ghidra `ruleBlockCat`.
    Sequence,
    /// `if (c) { t }` -- Ghidra `ruleBlockProperIf`.
    IfThen,
    /// `if (c) { t } else { e }` -- Ghidra `ruleBlockIfElse`.
    IfThenElse,
    /// `if (c) { t }` where `t` never returns control -- Ghidra
    /// `ruleBlockIfNoExit`. Distinct from [`ShapeKind::IfThen`] because there
    /// is no join to find, which is exactly what the existing follow-based
    /// lowering cannot express.
    IfNoExit,
    /// `while (1) { n }` -- a node that branches to itself.
    SelfLoop,
    /// `while (c) { body }` -- Ghidra `ruleBlockWhileDo`.
    WhileDo,
    /// `do { n } while (c)` -- Ghidra `ruleBlockDoWhile`.
    DoWhile,
}

/// A foldable region: the nodes it consumes and the node it is entered at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub entry: NodeId,
    pub members: Vec<NodeId>,
    /// For a conditional, the node control reaches after the region. `None`
    /// when the region does not hand control on (`IfNoExit`, or a loop whose
    /// exit is outside the matched members).
    pub follow: Option<NodeId>,
}

/// Try every shape at `entry`, most specific first.
///
/// Order matters the same way Ghidra's rule order does: a two-armed
/// conditional must be recognised before the one-armed form would claim half
/// of it, and a loop must be recognised before its body looks like a
/// sequence.
pub fn match_shape(g: &CollapseGraph, entry: NodeId) -> Option<Shape> {
    match_self_loop(g, entry)
        .or_else(|| match_do_while(g, entry))
        .or_else(|| match_while_do(g, entry))
        .or_else(|| match_if_then_else(g, entry))
        .or_else(|| match_if_no_exit(g, entry))
        .or_else(|| match_if_then(g, entry))
        .or_else(|| match_sequence(g, entry))
}

/// Scan the whole graph for a foldable region.
pub fn find_shape(g: &CollapseGraph) -> Option<Shape> {
    g.live_nodes().find_map(|n| match_shape(g, n))
}

/// `n -> s` where nothing else reaches `s`: the two run in sequence.
pub fn match_sequence(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let [s] = *g.successors(n) else { return None };
    if s == n || g.predecessors(s) != [n] {
        return None;
    }
    Some(Shape {
        kind: ShapeKind::Sequence,
        entry: n,
        members: vec![n, s],
        // Whatever `s` reaches is reached by the merged node instead.
        follow: single_successor(g, s),
    })
}

/// `if (c) { t }` -- `t` is entered only from `n` and rejoins `n`'s other arm.
pub fn match_if_then(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let (a, b) = two_successors(g, n)?;
    for (clause, join) in [(a, b), (b, a)] {
        if clause == n || clause == join {
            continue;
        }
        if g.predecessors(clause) != [n] {
            continue;
        }
        if g.successors(clause) != [join] {
            continue;
        }
        return Some(Shape {
            kind: ShapeKind::IfThen,
            entry: n,
            members: vec![n, clause],
            follow: Some(join),
        });
    }
    None
}

/// `if (c) { t }` where the clause has no successors at all -- it returns or
/// does not come back. Ghidra `ruleBlockIfNoExit`.
pub fn match_if_no_exit(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let (a, b) = two_successors(g, n)?;
    for (clause, other) in [(a, b), (b, a)] {
        if clause == n || clause == other {
            continue;
        }
        if g.predecessors(clause) != [n] {
            continue;
        }
        if !g.successors(clause).is_empty() {
            continue;
        }
        return Some(Shape {
            kind: ShapeKind::IfNoExit,
            entry: n,
            members: vec![n, clause],
            // Control that does not take the clause continues at `other`,
            // which is not part of the region.
            follow: Some(other),
        });
    }
    None
}

/// `if (c) { t } else { e }` -- both arms private to `n` and rejoining.
pub fn match_if_then_else(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let (t, e) = two_successors(g, n)?;
    if t == n || e == n || t == e {
        return None;
    }
    if g.predecessors(t) != [n] || g.predecessors(e) != [n] {
        return None;
    }
    let tj = single_successor(g, t);
    let ej = single_successor(g, e);
    match (tj, ej) {
        // Both arms fall to the same join.
        (Some(j1), Some(j2)) if j1 == j2 && j1 != n && j1 != t && j1 != e => Some(Shape {
            kind: ShapeKind::IfThenElse,
            entry: n,
            members: vec![n, t, e],
            follow: Some(j1),
        }),
        // Both arms end the path (each returns): still a two-armed if.
        (None, None) if g.successors(t).is_empty() && g.successors(e).is_empty() => Some(Shape {
            kind: ShapeKind::IfThenElse,
            entry: n,
            members: vec![n, t, e],
            follow: None,
        }),
        _ => None,
    }
}

/// A node that branches to itself: `while (1) { n }`.
pub fn match_self_loop(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    if !g.successors(n).contains(&n) {
        return None;
    }
    Some(Shape {
        kind: ShapeKind::SelfLoop,
        entry: n,
        members: vec![n],
        follow: g.successors(n).iter().copied().find(|s| *s != n),
    })
}

/// `while (c) { body }` -- `n` tests, `body` is private to `n` and loops back.
pub fn match_while_do(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let (a, b) = two_successors(g, n)?;
    for (body, exit) in [(a, b), (b, a)] {
        if body == n || body == exit {
            continue;
        }
        if g.predecessors(body) != [n] {
            continue;
        }
        if g.successors(body) != [n] {
            continue;
        }
        return Some(Shape {
            kind: ShapeKind::WhileDo,
            entry: n,
            members: vec![n, body],
            follow: Some(exit),
        });
    }
    None
}

/// `do { n } while (c)` -- `n` runs, then `cond` decides whether to repeat.
pub fn match_do_while(g: &CollapseGraph, n: NodeId) -> Option<Shape> {
    let [cond] = *g.successors(n) else {
        return None;
    };
    if cond == n || g.predecessors(cond) != [n] {
        return None;
    }
    let (a, b) = two_successors(g, cond)?;
    for (back, exit) in [(a, b), (b, a)] {
        if back != n || exit == n || exit == cond {
            continue;
        }
        return Some(Shape {
            kind: ShapeKind::DoWhile,
            entry: n,
            members: vec![n, cond],
            follow: Some(exit),
        });
    }
    None
}

fn two_successors(g: &CollapseGraph, n: NodeId) -> Option<(NodeId, NodeId)> {
    match *g.successors(n) {
        [a, b] => Some((a, b)),
        _ => None,
    }
}

fn single_successor(g: &CollapseGraph, n: NodeId) -> Option<NodeId> {
    match *g.successors(n) {
        [s] => Some(s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::{PreHirExpr, PreHirStmt};

    fn body(name: &str) -> Vec<PreHirStmt> {
        vec![PreHirStmt::Expr(PreHirExpr::Var(name.to_string()))]
    }

    #[test]
    fn matches_a_sequence() {
        // 0 -> 1 -> 2, and 1 is private to 0.
        let g = CollapseGraph::from_cfg(&[vec![1], vec![2], vec![]]);
        let s = match_sequence(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::Sequence);
        assert_eq!(s.members, vec![0, 1]);
        assert_eq!(s.follow, Some(2));
    }

    #[test]
    fn refuses_a_sequence_whose_tail_is_shared() {
        // 2 also reaches 1, so folding {0,1} would swallow 2's target.
        let g = CollapseGraph::from_cfg(&[vec![1], vec![], vec![1]]);
        assert!(match_sequence(&g, 0).is_none());
    }

    #[test]
    fn matches_if_then() {
        // 0 -> {1, 2}; 1 -> 2. The clause is 1, the join is 2.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![2], vec![]]);
        let s = match_if_then(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::IfThen);
        assert_eq!(s.members, vec![0, 1]);
        assert_eq!(s.follow, Some(2));
    }

    #[test]
    fn matches_if_then_else() {
        // 0 -> {1, 2}; 1 -> 3; 2 -> 3.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![]]);
        let s = match_if_then_else(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::IfThenElse);
        assert_eq!(s.members, vec![0, 1, 2]);
        assert_eq!(s.follow, Some(3));
    }

    #[test]
    fn matches_if_then_else_where_both_arms_return() {
        // 0 -> {1, 2}; neither arm continues.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![], vec![]]);
        let s = match_if_then_else(&g, 0).unwrap();
        assert_eq!(s.members, vec![0, 1, 2]);
        assert_eq!(s.follow, None);
    }

    #[test]
    fn matches_if_no_exit() {
        // 0 -> {1, 2}; 1 returns. There is no join, which is precisely the
        // case a follow-based rule cannot express.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![], vec![]]);
        let s = match_if_no_exit(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::IfNoExit);
        assert_eq!(s.members, vec![0, 1]);
        assert_eq!(s.follow, Some(2));
    }

    #[test]
    fn matches_self_loop_and_while_and_do_while() {
        let g = CollapseGraph::from_cfg(&[vec![0, 1], vec![]]);
        let s = match_self_loop(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::SelfLoop);
        assert_eq!(s.members, vec![0]);
        assert_eq!(s.follow, Some(1));

        // while: 0 tests, 1 is the body looping back to 0.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![0], vec![]]);
        let s = match_while_do(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::WhileDo);
        assert_eq!(s.members, vec![0, 1]);
        assert_eq!(s.follow, Some(2));

        // do-while: 0 runs, 1 tests and may repeat 0.
        let g = CollapseGraph::from_cfg(&[vec![1], vec![0, 2], vec![]]);
        let s = match_do_while(&g, 0).unwrap();
        assert_eq!(s.kind, ShapeKind::DoWhile);
        assert_eq!(s.members, vec![0, 1]);
        assert_eq!(s.follow, Some(2));
    }

    #[test]
    fn a_loop_is_preferred_over_reading_its_body_as_a_sequence() {
        // 0 -> 1 -> 0 would also satisfy do-while's prefix; ordering must not
        // let `Sequence` claim a cyclic shape.
        let g = CollapseGraph::from_cfg(&[vec![1], vec![0, 2], vec![]]);
        assert_eq!(match_shape(&g, 0).unwrap().kind, ShapeKind::DoWhile);
    }

    #[test]
    fn two_armed_if_is_preferred_over_one_armed() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![]]);
        assert_eq!(match_shape(&g, 0).unwrap().kind, ShapeKind::IfThenElse);
    }

    #[test]
    fn every_shape_it_reports_is_actually_foldable() {
        // The contract that ties this module to the graph: a matched region
        // must pass the graph's own single-entry admission.
        let graphs = [
            vec![vec![1], vec![2], vec![]],
            vec![vec![1, 2], vec![2], vec![]],
            vec![vec![1, 2], vec![3], vec![3], vec![]],
            vec![vec![1, 2], vec![0], vec![]],
            vec![vec![1], vec![0, 2], vec![]],
            vec![vec![0, 1], vec![]],
        ];
        for cfg in graphs {
            let g = CollapseGraph::from_cfg(&cfg);
            if let Some(s) = find_shape(&g) {
                assert_eq!(
                    g.check_single_entry(&s.members, s.entry),
                    Ok(()),
                    "shape {s:?} was reported but is not foldable in {cfg:?}"
                );
            }
        }
    }

    #[test]
    fn folding_a_matched_shape_drives_the_graph_toward_one_node() {
        // The loop step 3 will run: match, fold, repeat.
        let mut g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![]]);
        let mut folds = 0;
        while let Some(s) = find_shape(&g) {
            g.collapse(&s.members, s.entry, body("region")).unwrap();
            folds += 1;
            if folds > 8 {
                panic!("did not converge");
            }
        }
        assert_eq!(g.sole_live_node(), Some(0), "diamond folds to one node");
    }

    #[test]
    fn a_shape_can_appear_only_after_a_neighbour_folds() {
        // 0 -> {1,2}; 1 -> 3; 2 -> 3; 3 -> 4. Nothing matches at 3 initially
        // because it has two predecessors, so it cannot be a private tail.
        let mut g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![4], vec![]]);
        assert!(match_sequence(&g, 3).is_some(), "3 -> 4 is a sequence");
        assert!(
            match_sequence(&g, 1).is_none(),
            "3 is shared, so {{1,3}} is not a sequence yet"
        );
        // Fold the diamond; now 3 has a single predecessor.
        let s = match_if_then_else(&g, 0).unwrap();
        g.collapse(&s.members, s.entry, body("ite")).unwrap();
        assert!(
            match_sequence(&g, 0).is_some(),
            "the merged node and 3 are now a sequence -- unreachable under a static graph"
        );
    }
}
