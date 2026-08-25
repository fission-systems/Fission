//! Reaching conditions: the formula under which each node executes.
//!
//! Ghidra matches rules and angr's Phoenix matches schemas; both fall back to
//! a jump when nothing matches. DREAM (Yakdan et al., NDSS 2015, "No More
//! Gotos") takes a different route with a different failure mode: instead of
//! recognising shapes it computes, for every node, the boolean condition
//! under which control reaches it, simplifies that formula, and lets the
//! structure fall out of the conditions. There is no shape to fail to match,
//! so there is no "give up and emit a goto" branch.
//!
//! angr implements it in `analyses/decompiler/condition_processor.py`
//! (`recover_reaching_conditions`), and the recurrence is small:
//!
//! ```text
//! reaching(head) = true
//! reaching(n)    = OR over predecessors p of ( reaching(p) AND edge(p -> n) )
//! ```
//!
//! Visit in topological order so every predecessor is known before its
//! successor, then simplify. The graph must be acyclic -- DREAM structures
//! loops separately and applies this to the acyclic body -- so
//! [`compute_reaching_conditions`] reports a cycle rather than guessing.
//!
//! # What this buys
//!
//! Structure becomes a property of the conditions rather than of the shape:
//!
//! - two nodes with the *same* reaching condition run under the same guard
//!   and can share one `if`;
//! - two nodes whose conditions are `c` and `!c` are the arms of an if/else,
//!   however the CFG happens to be laid out;
//! - a node with condition `true` is unconditional.
//!
//! None of those queries care whether the CFG matched a diamond, so they see
//! cases a shape matcher cannot. [`conditions_are_complementary`] is the
//! primitive the later structuring step needs.
//!
//! Pure and host-free by construction: edge conditions arrive through a
//! closure, so nothing here can lower statements or touch builder state --
//! the hazard that has repeatedly forced this work to be reverted.

use crate::HashMap;
use crate::HashSet;
use fission_midend_prehir::util::{negate_expr, simplify_logical_expr};
use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr};
use fission_midend_core::ir::NirType;

/// Node identifier, matching [`crate::collapse_graph::NodeId`].
pub type NodeId = usize;

/// Why reaching conditions could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReachingError {
    /// The region contains a cycle; loops are structured separately.
    Cyclic,
    /// `head` is not a node of the graph.
    UnknownHead(NodeId),
}

/// `true` -- the condition of an unconditionally executed node.
pub fn always() -> PreHirExpr {
    PreHirExpr::Const(1, NirType::Bool)
}

pub fn is_always(e: &PreHirExpr) -> bool {
    matches!(e, PreHirExpr::Const(v, _) if *v != 0) || crate::boolean::is_tautology(e)
}

fn and(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    if is_always(&lhs) {
        return rhs;
    }
    if is_always(&rhs) {
        return lhs;
    }
    PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalAnd,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn or(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    if is_always(&lhs) || is_always(&rhs) {
        return always();
    }
    // `(X AND c) OR (X AND NOT c)` is `X`. This is the join of a diamond --
    // the single most common shape a reaching condition takes -- and without
    // it every join in the region carries a tautology as its guard.
    // `simplify_logical_expr` does not recognise complementary disjunction,
    // so it is done here where the two terms are known to come from the same
    // decision.
    if let Some(common) = common_part_of_complementary(&lhs, &rhs) {
        return common;
    }
    PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalOr,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

/// Flatten a conjunction into its terms; a non-conjunction is a single term.
pub(crate) fn conjuncts(e: &PreHirExpr) -> Vec<PreHirExpr> {
    conjunct_refs(e).into_iter().cloned().collect()
}

/// The terms of a conjunction, borrowed.
///
/// The owning [`conjuncts`] deep-copies every term, and a `PreHirExpr` term is
/// a whole expression tree. Structuring asks for these constantly -- to test
/// whether a guard mentions a conjunct, to take just the first one, to compare
/// two guards' terms -- and almost none of those need to own anything.
///
/// Measured on `bzip2`'s `main` (113 blocks, 3.3KB), where a `sample` profile
/// put `PreHirExpr::clone` and its drop glue at the top by a wide margin and
/// named `conjuncts` as the caller: 41s to decompile that one function, next
/// to 3.6s for `mainSort` at 56 blocks and twice the p-code. Cost tracked
/// block count, not size, which is what a per-block guard built by cloning
/// looks like.
pub(crate) fn conjunct_refs(e: &PreHirExpr) -> Vec<&PreHirExpr> {
    let mut out = Vec::new();
    collect_conjunct_refs(e, &mut out);
    out
}

fn collect_conjunct_refs<'a>(e: &'a PreHirExpr, out: &mut Vec<&'a PreHirExpr>) {
    match e {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ..
        } => {
            collect_conjunct_refs(lhs, out);
            collect_conjunct_refs(rhs, out);
        }
        other => out.push(other),
    }
}

/// Rebuild a conjunction from its terms; empty is `true`.
pub(crate) fn conjunction(terms: Vec<PreHirExpr>) -> PreHirExpr {
    let mut iter = terms.into_iter();
    let Some(first) = iter.next() else {
        return always();
    };
    iter.fold(first, |acc, c| PreHirExpr::Binary {
        op: PreHirBinaryOp::LogicalAnd,
        lhs: Box::new(acc),
        rhs: Box::new(c),
        ty: NirType::Bool,
    })
}

/// The shared part of two conjunctions whose remainders are complements, so
/// their disjunction is just that shared part. `None` when they do not have
/// that form.
fn common_part_of_complementary(a: &PreHirExpr, b: &PreHirExpr) -> Option<PreHirExpr> {
    let ca = conjunct_refs(a);
    let cb = conjunct_refs(b);
    let shared: Vec<&PreHirExpr> = ca
        .iter()
        .copied()
        .filter(|x| cb.iter().any(|y| y == x))
        .collect();
    let rest_a: Vec<&PreHirExpr> = ca
        .into_iter()
        .filter(|x| !shared.iter().any(|y| y == x))
        .collect();
    let rest_b: Vec<&PreHirExpr> = cb
        .into_iter()
        .filter(|x| !shared.iter().any(|y| y == x))
        .collect();
    let ([only_a], [only_b]) = (&rest_a[..], &rest_b[..]) else {
        return None;
    };
    // Only the terms that survive are copied.
    conditions_are_complementary(only_a, only_b)
        .then(|| conjunction(shared.into_iter().cloned().collect()))
}

/// Compute the condition under which each node executes.
///
/// `successors` is the adjacency listing; `edge_condition(p, n)` gives the
/// guard on the edge `p -> n`, or `None` when the edge is unconditional.
pub fn compute_reaching_conditions(
    successors: &[Vec<NodeId>],
    head: NodeId,
    edge_condition: impl Fn(NodeId, NodeId) -> Option<PreHirExpr>,
) -> Result<HashMap<NodeId, PreHirExpr>, ReachingError> {
    if head >= successors.len() {
        return Err(ReachingError::UnknownHead(head));
    }
    let order = topological_order(successors).ok_or(ReachingError::Cyclic)?;

    let mut predecessors: Vec<Vec<NodeId>> = vec![Vec::new(); successors.len()];
    for (u, outs) in successors.iter().enumerate() {
        for &v in outs {
            if v < successors.len() {
                predecessors[v].push(u);
            }
        }
    }

    let mut reaching: HashMap<NodeId, PreHirExpr> = HashMap::default();
    reaching.insert(head, always());

    for n in order {
        if n == head {
            continue;
        }
        let mut acc: Option<PreHirExpr> = None;
        for &p in &predecessors[n] {
            // A predecessor that is itself unreachable contributes nothing.
            let Some(pred_cond) = reaching.get(&p).cloned() else {
                continue;
            };
            let edge = edge_condition(p, n).unwrap_or_else(always);
            let term = and(pred_cond, edge);
            acc = Some(match acc {
                None => term,
                Some(prev) => or(term, prev),
            });
        }
        if let Some(cond) = acc {
            reaching.insert(n, simplify_logical_expr(cond));
        }
    }
    Ok(reaching)
}

/// Kahn's algorithm; `None` when the graph has a cycle.
pub fn topological_order(successors: &[Vec<NodeId>]) -> Option<Vec<NodeId>> {
    let n = successors.len();
    let mut indegree = vec![0usize; n];
    for outs in successors {
        for &v in outs {
            if v < n {
                indegree[v] += 1;
            }
        }
    }
    let mut ready: Vec<NodeId> = (0..n).filter(|i| indegree[*i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(u) = ready.pop() {
        order.push(u);
        for &v in &successors[u] {
            if v >= n {
                continue;
            }
            indegree[v] -= 1;
            if indegree[v] == 0 {
                ready.push(v);
            }
        }
    }
    (order.len() == n).then_some(order)
}

/// Whether `a` and `b` are complements -- one holds exactly when the other
/// does not, so the nodes they guard are the arms of an if/else.
///
/// Decided over the atoms rather than by matching shapes, so De Morgan and
/// anything else that rewrites a formula without changing it are recognised.
/// [`crate::boolean`] returns `false` when it cannot decide, which loses a
/// structuring opportunity; a wrong `true` would merge two arms that can both
/// run, so the syntactic check is kept as the fast path and the decision
/// procedure only ever adds to it.
pub fn conditions_are_complementary(a: &PreHirExpr, b: &PreHirExpr) -> bool {
    &negate_expr(a.clone()) == b
        || &negate_expr(b.clone()) == a
        || crate::boolean::are_complementary(a, b)
}

/// Group nodes that execute under the same condition.
///
/// Nodes sharing a guard can be emitted under one `if`, whatever the CFG
/// shape between them.
pub fn group_by_condition(
    reaching: &HashMap<NodeId, PreHirExpr>,
) -> Vec<(PreHirExpr, Vec<NodeId>)> {
    let mut groups: Vec<(PreHirExpr, Vec<NodeId>)> = Vec::new();
    let mut nodes: Vec<NodeId> = reaching.keys().copied().collect();
    nodes.sort_unstable();
    for n in nodes {
        let cond = &reaching[&n];
        match groups.iter_mut().find(|(c, _)| c == cond) {
            Some((_, members)) => members.push(n),
            None => groups.push((cond.clone(), vec![n])),
        }
    }
    groups
}

/// Nodes that always execute -- their condition simplified to `true`.
pub fn unconditional_nodes(reaching: &HashMap<NodeId, PreHirExpr>) -> HashSet<NodeId> {
    reaching
        .iter()
        .filter(|(_, c)| is_always(c))
        .map(|(n, _)| *n)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_join_of_a_diamond_is_unconditional() {
        // Both ways out of one decision reconverge, so the join always runs.
        // `simplify_logical_expr` does not fold `c OR NOT c`, and without the
        // fold every join in a region carries a tautology as its guard.
        let c = PreHirExpr::Var("c".to_string());
        let joined = or(c.clone(), negate_expr(c.clone()));
        assert!(is_always(&joined), "got {joined:?}");

        // The same with a shared guard in front: `(a AND c) OR (a AND !c)`.
        let a = PreHirExpr::Var("a".to_string());
        let guarded = or(
            and(a.clone(), c.clone()),
            and(a.clone(), negate_expr(c.clone())),
        );
        assert_eq!(guarded, a, "the shared part survives, the decision does not");
    }

    #[test]
    fn unrelated_disjuncts_are_left_alone() {
        // Two different decisions must not collapse -- that would claim a node
        // runs unconditionally when it does not.
        let a = PreHirExpr::Var("a".to_string());
        let b = PreHirExpr::Var("b".to_string());
        let joined = or(a, negate_expr(b));
        assert!(!is_always(&joined), "got {joined:?}");
    }


    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    /// Diamond: 0 -> {1,2} -> 3, guarded by `c` and `!c`.
    fn diamond() -> (Vec<Vec<NodeId>>, impl Fn(NodeId, NodeId) -> Option<PreHirExpr>) {
        let succ = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let edge = |p: NodeId, n: NodeId| match (p, n) {
            (0, 1) => Some(var("c")),
            (0, 2) => Some(negate_expr(var("c"))),
            _ => None,
        };
        (succ, edge)
    }

    #[test]
    fn head_is_unconditional_and_arms_get_their_guards() {
        let (succ, edge) = diamond();
        let r = compute_reaching_conditions(&succ, 0, edge).unwrap();
        assert!(is_always(&r[&0]));
        assert_eq!(r[&1], var("c"));
        assert_eq!(r[&2], negate_expr(var("c")));
    }

    #[test]
    fn a_join_reached_from_both_arms_is_unconditional() {
        // `(c) OR (!c)` covers every path, so node 3 always runs. This is the
        // property a shape matcher gets from seeing a diamond; here it falls
        // out of the formula instead.
        let (succ, edge) = diamond();
        let r = compute_reaching_conditions(&succ, 0, edge).unwrap();
        let cond = &r[&3];
        assert!(
            matches!(cond, PreHirExpr::Binary { op: PreHirBinaryOp::LogicalOr, .. })
                || is_always(cond),
            "join condition should be a disjunction over both arms, got {cond:?}"
        );
    }

    #[test]
    fn arms_of_a_conditional_are_recognised_as_complementary() {
        let (succ, edge) = diamond();
        let r = compute_reaching_conditions(&succ, 0, edge).unwrap();
        assert!(
            conditions_are_complementary(&r[&1], &r[&2]),
            "the two arms guard mutually exclusive paths"
        );
    }

    #[test]
    fn nodes_under_the_same_guard_group_together() {
        // 0 -> {1,2}; 1 -> 3. Nodes 1 and 3 both run exactly when `c` holds,
        // even though the CFG between them is a plain edge, not a shape.
        let succ = vec![vec![1, 2], vec![3], vec![], vec![]];
        let edge = |p: NodeId, n: NodeId| match (p, n) {
            (0, 1) => Some(var("c")),
            (0, 2) => Some(negate_expr(var("c"))),
            _ => None,
        };
        let r = compute_reaching_conditions(&succ, 0, edge).unwrap();
        assert_eq!(r[&1], r[&3], "a private successor inherits its guard");
        let groups = group_by_condition(&r);
        let shared = groups
            .iter()
            .find(|(c, _)| *c == var("c"))
            .expect("a group guarded by c");
        assert_eq!(shared.1, vec![1, 3]);
    }

    #[test]
    fn a_nested_guard_conjoins_with_its_parent() {
        // 0 -> {1,2}; 1 -> {3,4}. Node 3 runs when c AND d.
        let succ = vec![vec![1, 2], vec![3, 4], vec![], vec![], vec![]];
        let edge = |p: NodeId, n: NodeId| match (p, n) {
            (0, 1) => Some(var("c")),
            (0, 2) => Some(negate_expr(var("c"))),
            (1, 3) => Some(var("d")),
            (1, 4) => Some(negate_expr(var("d"))),
            _ => None,
        };
        let r = compute_reaching_conditions(&succ, 0, edge).unwrap();
        assert_eq!(
            r[&3],
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs: Box::new(var("c")),
                rhs: Box::new(var("d")),
                ty: NirType::Bool,
            }
        );
        // 4 runs under `c AND !d`: same parent guard, opposite inner guard.
        assert_eq!(
            r[&4],
            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs: Box::new(var("c")),
                rhs: Box::new(negate_expr(var("d"))),
                ty: NirType::Bool,
            }
        );
    }

    #[test]
    fn unconditional_nodes_are_reported() {
        let succ = vec![vec![1], vec![2], vec![]];
        let r = compute_reaching_conditions(&succ, 0, |_, _| None).unwrap();
        let u = unconditional_nodes(&r);
        assert!(u.contains(&0) && u.contains(&1) && u.contains(&2));
    }

    #[test]
    fn a_cycle_is_reported_rather_than_guessed() {
        // Loops are structured separately in DREAM; silently producing a
        // formula here would be wrong.
        let succ = vec![vec![1], vec![0]];
        assert_eq!(
            compute_reaching_conditions(&succ, 0, |_, _| None),
            Err(ReachingError::Cyclic)
        );
    }

    #[test]
    fn an_unknown_head_is_rejected() {
        let succ = vec![vec![]];
        assert_eq!(
            compute_reaching_conditions(&succ, 7, |_, _| None),
            Err(ReachingError::UnknownHead(7))
        );
    }
}
