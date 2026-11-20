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
use fission_midend_core::ir::NirType;
use fission_midend_prehir::util::{negate_expr, simplify_logical_expr};
use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr};

/// Node identifier, matching [`crate::collapse_graph::NodeId`].
pub type NodeId = usize;

/// Why reaching conditions could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReachingError {
    /// The region contains a cycle; loops are structured separately.
    Cyclic,
    /// `head` is not a node of the graph.
    UnknownHead(NodeId),
    /// A node's condition outgrew [`MAX_REACHING_CONDITION_TERMS`].
    ///
    /// Each node's condition is built from its predecessors' conditions, so a
    /// branchy region doubles the expression at every merge: the growth is in
    /// the number of *paths*, not the number of blocks. Measured on `bzip2`'s
    /// `main` (113 blocks): the largest condition reached 8,729,059 expression
    /// nodes and the process peaked at 9.2GB across 140 million live
    /// allocations, against 439MB for `sendMTFValues` at 112 blocks, whose
    /// conditions all stayed at a single term.
    ///
    /// Truncating individual conditions to `true` was rejected: 22% of that
    /// function's conditions are over the bound, and a body structured from a
    /// mix of exact and unknown conditions is worse than one the caller
    /// structures by the fallback it already has. Callers read any error here
    /// as "not amenable to this path".
    ConditionTooLarge { node: NodeId, terms: usize },
}

/// The point past which a reaching condition stops being worth computing.
///
/// The median condition in the pathological case measured 71 terms and the
/// 75th percentile 637, so this clears ordinary work by a wide margin; what it
/// cuts is the exponential tail.
pub const MAX_REACHING_CONDITION_TERMS: usize = 4096;

/// Nodes in `e`, counting the shapes conditions are built from.
fn condition_terms(e: &PreHirExpr) -> usize {
    match e {
        PreHirExpr::Binary { lhs, rhs, .. } => 1 + condition_terms(lhs) + condition_terms(rhs),
        PreHirExpr::Unary { expr, .. } | PreHirExpr::Cast { expr, .. } => 1 + condition_terms(expr),
        _ => 1,
    }
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
    reaching_over(successors, head, &order, edge_condition, None)
}

/// [`compute_reaching_conditions`] for a region emitted in `order`, where the
/// nodes in `leaves` end by returning from the function.
///
/// Emission evaluates guards in `order`, one after another. Once a node in
/// `leaves` has run, nothing after it runs at all -- so by the time a later
/// node's guard is evaluated, every path into a returning node that precedes
/// it is known not to have been taken. The condition does not have to say so
/// again. Without this, the code after `if (c) { ...; return; }` is guarded by
/// `!c` spelled out as every other path through the region, and every later
/// guard repeats that formula as a conjunct: the source's straight-line code
/// after an early return comes back as a chain of re-tested decisions.
///
/// So each node's condition is simplified *where it can be evaluated* -- under
/// the negation of the returning paths before it -- by [`prune_guard`], which
/// only ever removes operands. A guard that is true in every such state
/// becomes `true`, i.e. the node is unconditional, which is what it is.
///
/// `order` must be the order the region is emitted in; a different order
/// would let a guard assume a return that has not been evaluated yet.
/// `leaves` must hold only nodes that return on every path through their own
/// statements -- the caller decides that from the statements, not the graph.
pub fn compute_reaching_conditions_after_returns(
    successors: &[Vec<NodeId>],
    head: NodeId,
    order: &[NodeId],
    edge_condition: impl Fn(NodeId, NodeId) -> Option<PreHirExpr>,
    leaves: &HashSet<NodeId>,
) -> Result<HashMap<NodeId, PreHirExpr>, ReachingError> {
    if head >= successors.len() {
        return Err(ReachingError::UnknownHead(head));
    }
    reaching_over(successors, head, order, edge_condition, Some(leaves))
}

fn reaching_over(
    successors: &[Vec<NodeId>],
    head: NodeId,
    order: &[NodeId],
    edge_condition: impl Fn(NodeId, NodeId) -> Option<PreHirExpr>,
    leaves: Option<&HashSet<NodeId>>,
) -> Result<HashMap<NodeId, PreHirExpr>, ReachingError> {
    // Disjunction of the reaching conditions of the returning nodes visited
    // so far; `None` while there are none.
    let mut returned: Option<PreHirExpr> = None;
    let note_return = |n: NodeId, cond: &PreHirExpr, returned: &mut Option<PreHirExpr>| {
        if leaves.is_some_and(|l| l.contains(&n)) {
            *returned = Some(match returned.take() {
                None => cond.clone(),
                Some(prev) => PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalOr,
                    lhs: Box::new(prev),
                    rhs: Box::new(cond.clone()),
                    ty: NirType::Bool,
                },
            });
        }
    };

    let mut predecessors: Vec<Vec<NodeId>> = vec![Vec::new(); successors.len()];
    for (u, outs) in successors.iter().enumerate() {
        for &v in outs {
            if v < successors.len() {
                predecessors[v].push(u);
            }
        }
    }

    let mut reaching: HashMap<NodeId, PreHirExpr> = HashMap::default();
    let mut sizes: HashMap<NodeId, usize> = HashMap::default();
    reaching.insert(head, always());
    sizes.insert(head, 1);
    note_return(head, &always(), &mut returned);

    for &n in order {
        if n == head {
            continue;
        }
        let mut acc: Option<PreHirExpr> = None;
        let mut acc_terms = 0usize;
        for &p in &predecessors[n] {
            // A predecessor that is itself unreachable contributes nothing.
            let Some(pred_cond) = reaching.get(&p) else {
                continue;
            };
            let edge = edge_condition(p, n).unwrap_or_else(always);
            // Projected before anything is built, so the bound caps what gets
            // allocated rather than what gets kept.
            let projected = acc_terms
                + sizes
                    .get(&p)
                    .copied()
                    .unwrap_or_else(|| condition_terms(pred_cond))
                + condition_terms(&edge)
                + 2;
            if projected > MAX_REACHING_CONDITION_TERMS {
                return Err(ReachingError::ConditionTooLarge {
                    node: n,
                    terms: projected,
                });
            }
            let term = and(pred_cond.clone(), edge);
            acc_terms = projected;
            acc = Some(match acc {
                None => term,
                Some(prev) => or(term, prev),
            });
        }
        if let Some(cond) = acc {
            let mut simplified = simplify_logical_expr(cond);
            if leaves.is_some() {
                // The states this guard can be evaluated in: none of the
                // returning paths before it was taken. When those paths cover
                // everything, the node is dead and no state is left to
                // simplify against; leave its condition as computed.
                let care = returned
                    .as_ref()
                    .map_or_else(always, |r| negate_expr(r.clone()));
                if !crate::boolean::is_contradiction(&care) {
                    simplified = prune_guard(simplified, &care);
                }
            }
            note_return(n, &simplified, &mut returned);
            sizes.insert(n, condition_terms(&simplified));
            reaching.insert(n, simplified);
        }
    }
    Ok(reaching)
}

/// Guards larger than this are left as they are: each removal is checked
/// with a truth table over the whole guard, so the cost is quadratic in it.
const MAX_PRUNED_GUARD_TERMS: usize = 256;

/// Remove the operands of `cond` that do not change its value anywhere `care`
/// holds.
///
/// Only two removals are ever made, and both are chosen so that the guard
/// still reads a decision variable only after the decisions that lead to it:
///
/// - a disjunct, from any position -- each remaining disjunct stays the
///   ordered path formula it was;
/// - the *last* conjunct of a conjunction -- a prefix of a path formula is
///   still a path formula.
///
/// A leading conjunct is never removed, even when that would be equivalent:
/// it is what keeps a later operand from reading a variable its branch never
/// assigned. Nothing is ever added or reordered. Returns `true` when `cond`
/// holds everywhere `care` does.
pub fn prune_guard(cond: PreHirExpr, care: &PreHirExpr) -> PreHirExpr {
    if is_always(&cond) || condition_terms(&cond) > MAX_PRUNED_GUARD_TERMS {
        return cond;
    }
    if crate::boolean::agree_where(&always(), &cond, care) {
        return always();
    }
    let mut current = cond;
    'shrink: loop {
        let mut site = 0usize;
        loop {
            let mut k = site;
            let Some(candidate) = without_operand(&current, &mut k, false) else {
                break 'shrink;
            };
            if crate::boolean::agree_where(&candidate, &current, care) {
                current = candidate;
                continue 'shrink;
            }
            site += 1;
        }
    }
    current
}

/// `e` with its `k`-th removable operand (pre-order) taken out, or `None`
/// when it has fewer than `k + 1`.
///
/// `followed` marks a subformula that some enclosing conjunction evaluates
/// another operand after. Removing a disjunct makes a formula true in fewer
/// states, so what follows is read no more often than before; removing a
/// trailing conjunct makes it true in more, so it is allowed only where
/// nothing follows.
fn without_operand(e: &PreHirExpr, k: &mut usize, followed: bool) -> Option<PreHirExpr> {
    let PreHirExpr::Binary { op, lhs, rhs, ty } = e else {
        return None;
    };
    let rebuild = |l: PreHirExpr, r: PreHirExpr| PreHirExpr::Binary {
        op: *op,
        lhs: Box::new(l),
        rhs: Box::new(r),
        ty: ty.clone(),
    };
    match op {
        PreHirBinaryOp::LogicalOr => {
            for keep in [rhs, lhs] {
                if *k == 0 {
                    return Some((**keep).clone());
                }
                *k -= 1;
            }
            if let Some(l) = without_operand(lhs, k, followed) {
                return Some(rebuild(l, (**rhs).clone()));
            }
            without_operand(rhs, k, followed).map(|r| rebuild((**lhs).clone(), r))
        }
        PreHirBinaryOp::LogicalAnd => {
            if !followed {
                if *k == 0 {
                    return Some((**lhs).clone());
                }
                *k -= 1;
            }
            if let Some(l) = without_operand(lhs, k, true) {
                return Some(rebuild(l, (**rhs).clone()));
            }
            without_operand(rhs, k, followed).map(|r| rebuild((**lhs).clone(), r))
        }
        _ => None,
    }
}

/// Kahn's algorithm; `None` when the graph has a cycle.
pub fn topological_order(successors: &[Vec<NodeId>]) -> Option<Vec<NodeId>> {
    topological_order_returning_first(successors, &HashSet::default())
}

/// [`topological_order`], taking a node of `returning` whenever one is ready.
///
/// Any topological order is a correct emission order; this one puts an early
/// return ahead of the code that runs when it is not taken, as `if (c)
/// return; rest;` is written. That is also the order in which
/// [`compute_reaching_conditions_after_returns`] can use the return: a guard
/// may only assume returns emitted before it.
pub fn topological_order_returning_first(
    successors: &[Vec<NodeId>],
    returning: &HashSet<NodeId>,
) -> Option<Vec<NodeId>> {
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
    loop {
        let next = match ready.iter().rposition(|u| returning.contains(u)) {
            Some(i) => Some(ready.remove(i)),
            None => ready.pop(),
        };
        let Some(u) = next else {
            break;
        };
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
        assert_eq!(
            guarded, a,
            "the shared part survives, the decision does not"
        );
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
    fn diamond() -> (
        Vec<Vec<NodeId>>,
        impl Fn(NodeId, NodeId) -> Option<PreHirExpr>,
    ) {
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
            matches!(
                cond,
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::LogicalOr,
                    ..
                }
            ) || is_always(cond),
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

    /// `if (a) { if (b) return; }  rest;` -- node 3 returns, node 2 is the
    /// code after the early return.
    fn early_return() -> (
        Vec<Vec<NodeId>>,
        impl Fn(NodeId, NodeId) -> Option<PreHirExpr>,
    ) {
        let succ = vec![vec![1, 2], vec![3, 2], vec![], vec![]];
        let edge = |p: NodeId, n: NodeId| match (p, n) {
            (0, 1) => Some(var("a")),
            (0, 2) => Some(negate_expr(var("a"))),
            (1, 3) => Some(var("b")),
            (1, 2) => Some(negate_expr(var("b"))),
            _ => None,
        };
        (succ, edge)
    }

    #[test]
    fn code_after_an_early_return_is_unconditional() {
        let (succ, edge) = early_return();
        let leaves: HashSet<NodeId> = [3].into_iter().collect();
        let order = topological_order_returning_first(&succ, &leaves).unwrap();
        // The order matters: the returning node has to be emitted first.
        assert!(
            order.iter().position(|&n| n == 3) < order.iter().position(|&n| n == 2),
            "{order:?}"
        );
        let r =
            compute_reaching_conditions_after_returns(&succ, 0, &order, &edge, &leaves).unwrap();
        assert!(is_always(&r[&2]), "got {:?}", r[&2]);
        // The returning node keeps the condition it returns under.
        assert_eq!(r[&3], and(var("a"), var("b")));
    }

    #[test]
    fn without_the_return_the_rest_stays_guarded() {
        let (succ, edge) = early_return();
        let order = topological_order(&succ).unwrap();
        let plain = compute_reaching_conditions(&succ, 0, &edge).unwrap();
        assert!(!is_always(&plain[&2]), "got {:?}", plain[&2]);
        // Node 3 here does not return, so nothing may be assumed about it.
        let none = HashSet::default();
        let r = compute_reaching_conditions_after_returns(&succ, 0, &order, &edge, &none).unwrap();
        assert!(!is_always(&r[&2]), "got {:?}", r[&2]);
    }

    #[test]
    fn a_return_emitted_after_a_node_simplifies_nothing_before_it() {
        // Same graph, but node 2 is emitted before node 3: a guard may not
        // assume a return that has not been evaluated yet.
        let (succ, edge) = early_return();
        let order = topological_order(&succ).unwrap();
        assert_eq!(order, vec![0, 1, 2, 3]);
        let leaves: HashSet<NodeId> = [3].into_iter().collect();
        let r =
            compute_reaching_conditions_after_returns(&succ, 0, &order, &edge, &leaves).unwrap();
        assert!(!is_always(&r[&2]), "got {:?}", r[&2]);
    }

    #[test]
    fn pruning_drops_a_trailing_conjunct_but_never_a_leading_one() {
        let a_and_b = and(var("a"), var("b"));
        // Where `b` always holds, `a && b` is `a`: the last operand goes.
        assert_eq!(prune_guard(a_and_b.clone(), &var("b")), var("a"));
        // Where `a` always holds it is just as much `b` -- but `a` is the
        // decision that makes `b` meaningful, so it stays.
        assert_eq!(prune_guard(a_and_b.clone(), &var("a")), a_and_b);
    }

    #[test]
    fn pruning_drops_a_disjunct_that_cannot_hold() {
        let a_or_b = or(var("a"), var("b"));
        let not_a = negate_expr(var("a"));
        assert_eq!(prune_guard(a_or_b, &not_a), var("b"));
    }

    #[test]
    fn pruning_keeps_a_conjunct_something_is_read_after() {
        // `(a && b) && c` where `b` always holds: `b` is not last, so
        // removing it would read `c` in states it was not read in before.
        let chain = and(and(var("a"), var("b")), var("c"));
        assert_eq!(prune_guard(chain.clone(), &var("b")), chain);
    }
}
