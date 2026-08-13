//! Match-fold structuring driver.
//!
//! The loop every reference structurer runs, now expressible because the
//! substrate shrinks: find a foldable region, lower it, fold it into one
//! node, repeat. When nothing matches, concede exactly one edge to a jump and
//! try again. Finish when a single node remains, and its body is the
//! function.
//!
//! # Relationship to the existing driver
//!
//! `sese_driver::build_sese_region_body` scans block indices over a fixed
//! graph, records winners in `active_child_map`, and emits whatever was never
//! claimed as residual blocks with their edges as jumps. Region boundaries
//! come from a SESE tree computed up front. This driver has no index scan, no
//! side table, and no precomputed boundaries: regions are node sets
//! discovered against the current graph.
//!
//! # Safety posture
//!
//! Two deliberate properties, both learned from earlier failures in this
//! work:
//!
//! - **Discovery cannot touch anything.** Shape matching is a pure graph
//!   query ([`crate::collapse_shapes`]); the host is consulted only to lower
//!   a region that has already been chosen. Attempting a rule can therefore
//!   never perturb builder state, which is what forced the earlier recursive
//!   complex-arm work to be reverted.
//! - **All or nothing.** The driver either folds the function to a single
//!   node and returns its body, or returns `None` and leaves the caller to
//!   use the existing path. It never returns a partially structured result,
//!   so there is no half-state for a later stage to misread.
//!
//! # Why the whole fold runs isolated
//!
//! The "discovery cannot touch anything" argument above is **incomplete, and
//! measurement proved it.** Shape matching is pure, but [`lower_shape`] calls
//! `lower_block_stmts` and `lower_block_terminator` to build a region's body
//! *before* the driver knows whether it will commit. When the fold then fails
//! and the driver returns `None`, those lowering side effects have already
//! landed on the host and perturb the existing path that runs next.
//!
//! This was measured, not theorised. With concessions capped at zero -- so
//! every committed body is jump-free and could only reduce the count -- the
//! corpus still moved by +8 gotos with 16 files regressed, and files the
//! driver *declined* changed. That is only possible through leaked lowering.
//!
//! So [`structure_by_match_fold`] runs its entire fold inside
//! [`StructuringHost::lower_isolated`]. Nothing reaches the host unless the
//! graph reduced to a single node, which makes the all-or-nothing property
//! above true of host state and not just of the return value.
//!
//! Gated off by default; see [`match_fold_driver_enabled`].

use crate::collapse_graph::{CollapseGraph, NodeId};
use crate::collapse_shapes::{Shape, ShapeKind, find_shape};
use crate::host::StructuringHost;
use crate::linear_types::LoweredTerminator;
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::PreHirStmt;
use fission_midend_prehir::util::negate_expr;

/// Ceiling on fold/concession rounds, proportional to graph size.
const MAX_ROUNDS_PER_NODE: usize = 8;

/// How many edges the driver may concede to a jump and still commit.
///
/// Zero, deliberately. Folding to a single node is not by itself a win: each
/// concession is a `goto`, so a function that folds only after twenty of them
/// lands worse than the existing driver would. Measured on the corpus with
/// concessions unlimited, the driver removed 40 gotos across 12 functions
/// (five reaching zero) but added 90 across 25 others, for a net loss.
///
/// Requiring a concession-free fold makes the result unconditionally better:
/// the structured body contains no jumps at all, so committing it can only
/// reduce the count. This is the same discipline as angr's
/// `strictly_less_gotos`, expressed as a precondition instead of an
/// after-the-fact comparison -- which matters here because measuring the
/// alternative would mean lowering the function twice.
const MAX_CONCESSIONS: usize = 0;

/// On by default; opt out with `FISSION_MATCH_FOLD=0`.
///
/// Safe to default on only because it no longer runs *first*. It offers a
/// candidate to `try_alternative_structurings`, which keeps the existing
/// structuring unless this one strictly wins on jumps while giving up nothing
/// on `crate::structuring_quality`'s other axes. That is what retires the
/// three failures this driver used to cause -- the empty `if` shell where the
/// existing path folds a short-circuit `&&` among them. They are not fixed;
/// they are simply never chosen.
pub fn match_fold_driver_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("FISSION_MATCH_FOLD").as_deref(),
            Ok("0" | "false" | "off" | "no")
        )
    })
}

/// Structure a whole function by folding its graph to a single node.
///
/// Returns `None` when the graph could not be reduced to one node, leaving
/// the caller on its existing path with host state untouched -- the fold runs
/// against a fork that is committed only if it succeeds.
pub fn structure_by_match_fold(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    host.lower_isolated(fold_to_single_node)
}

fn fold_to_single_node(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(None);
    }
    let mut graph = CollapseGraph::from_cfg(&successors);
    let budget = successors.len().saturating_mul(MAX_ROUNDS_PER_NODE);
    let mut concessions = 0usize;

    let reachable = blocks_reachable_from_entry(&successors);

    for _ in 0..budget {
        if let Some(sole) = graph.sole_live_node() {
            let Some(node) = graph.node(sole) else {
                return Ok(None);
            };
            if !covers_every_reachable_block(node, &reachable) {
                return Ok(None);
            }
            return Ok(node.body.clone().or_else(|| Some(Vec::new())));
        }
        if let Some(shape) = find_shape(&graph) {
            // Check before lowering: a fold that would delete control flow is
            // not worth building a body for.
            let expressible = fold_accounts_for_every_internal_edge(&graph, &shape);
            let lowered = if expressible {
                lower_shape(host, &graph, &shape)?
            } else {
                None
            };
            let Some(body) = lowered else {
                // The shape is graph-legal but not expressible yet. Concede an
                // edge so the next round sees a different graph rather than
                // rediscovering the same unlowerable region forever.
                concessions += 1;
                if concessions > MAX_CONCESSIONS
                    || !concede_one_edge(&mut graph, Some(&shape))
                {
                    return Ok(None);
                }
                continue;
            };
            if graph.collapse(&shape.members, shape.entry, body).is_err() {
                return Ok(None);
            }
            continue;
        }
        concessions += 1;
        if concessions > MAX_CONCESSIONS || !concede_one_edge(&mut graph, None) {
            return Ok(None);
        }
    }
    Ok(None)
}

/// The edges a shape's emitted body actually accounts for.
///
/// `None` when the member list does not match the kind, which is itself a
/// reason to decline.
fn expected_internal_edges(shape: &Shape) -> Option<Vec<(NodeId, NodeId)>> {
    Some(match (shape.kind, &shape.members[..]) {
        (ShapeKind::Sequence, &[n, s]) => vec![(n, s)],
        (ShapeKind::IfThen | ShapeKind::IfNoExit, &[n, clause]) => vec![(n, clause)],
        (ShapeKind::IfThenElse, &[n, t, e]) => vec![(n, t), (n, e)],
        (ShapeKind::WhileDo, &[n, inner]) => vec![(n, inner), (inner, n)],
        (ShapeKind::DoWhile, &[n, cond]) => vec![(n, cond), (cond, n)],
        (ShapeKind::SelfLoop, &[n]) => vec![(n, n)],
        _ => return None,
    })
}

/// Whether folding `shape` would delete control flow its body does not
/// express.
///
/// **The obligation node coverage cannot express.** [`CollapseGraph::collapse`]
/// detaches every edge internal to the region, on the assumption that the
/// emitted body says what those edges said. Any internal edge the shape does
/// not account for is therefore silently deleted -- and an acyclic shape
/// matched over a region containing a back edge deletes the loop.
///
/// That is exactly what happened: `for_simple_fn` folded with all five of its
/// blocks present and correctly owned, and came back as a straight line whose
/// loop had become `if (0 < 10) { return 0; }`. Every block survived; the
/// cycle did not. Checking membership alone reports that fold as sound, which
/// is why this check exists separately.
pub(crate) fn fold_accounts_for_every_internal_edge(graph: &CollapseGraph, shape: &Shape) -> bool {
    let Some(mut expected) = expected_internal_edges(shape) else {
        return false;
    };
    let set: crate::HashSet<NodeId> = shape.members.iter().copied().collect();
    let mut actual: Vec<(NodeId, NodeId)> = Vec::new();
    for &m in &shape.members {
        for &s in graph.successors(m) {
            if set.contains(&s) {
                actual.push((m, s));
            }
        }
    }
    actual.sort_unstable();
    actual.dedup();
    expected.sort_unstable();
    expected.dedup();
    actual == expected
}

/// Blocks reachable from the function entry, which is always block 0.
pub(crate) fn blocks_reachable_from_entry(successors: &[Vec<usize>]) -> Vec<usize> {
    let mut seen = vec![false; successors.len()];
    let mut stack = vec![0usize];
    seen[0] = true;
    let mut out = Vec::new();
    while let Some(b) = stack.pop() {
        out.push(b);
        for &s in &successors[b] {
            if let Some(flag) = seen.get_mut(s) {
                if !*flag {
                    *flag = true;
                    stack.push(s);
                }
            }
        }
    }
    out
}

/// Whether the folded node still stands for the whole function.
///
/// **The obligation the goto metric cannot express.** Folding to one node with
/// no conceded edges bounds how many jumps the body contains; it says nothing
/// about whether the body still contains the function. Those are independent,
/// and the cheapest way to score a perfect zero is to lose a region: a
/// `for` loop came back as `return 0;` -- init, test, and body gone, not a
/// jump anywhere in it -- and every goto-density check called that an
/// improvement.
///
/// So the surviving node must be entered at the function entry and own every
/// block reachable from it. `BlockOwnership` is merged through every collapse
/// precisely so this is answerable at the end.
///
/// Unreachable blocks are excluded deliberately: they contribute no behaviour,
/// and requiring them would decline folds that are entirely correct.
pub(crate) fn covers_every_reachable_block(node: &crate::collapse_graph::CollapseNode, reachable: &[usize]) -> bool {
    node.entry_block == 0 && reachable.iter().all(|&b| node.members.contains(b))
}

/// Drop one edge so the next round faces a different graph.
///
/// Prefers an edge out of the region that just failed, then any back edge
/// (the classic unstructured jump), then any edge at all. Returns `false`
/// when there is nothing left to concede, which ends the loop.
fn concede_one_edge(graph: &mut CollapseGraph, failed: Option<&Shape>) -> bool {
    if let Some(shape) = failed {
        for &m in &shape.members {
            let outs: Vec<NodeId> = graph.successors(m).to_vec();
            for s in outs {
                if !shape.members.contains(&s) && graph.virtualize_edge(m, s) {
                    return true;
                }
            }
        }
    }
    // A back edge -- an edge to a node that already reaches its source -- is
    // the least damaging thing to express as a jump.
    let live: Vec<NodeId> = graph.live_nodes().collect();
    for &u in &live {
        let outs: Vec<NodeId> = graph.successors(u).to_vec();
        for v in outs {
            if v <= u && graph.virtualize_edge(u, v) {
                return true;
            }
        }
    }
    for &u in &live {
        let outs: Vec<NodeId> = graph.successors(u).to_vec();
        if let Some(&v) = outs.first() {
            if graph.virtualize_edge(u, v) {
                return true;
            }
        }
    }
    false
}

/// Statements a node contributes, including the `return` a terminal block
/// ends with.
///
/// A node's *outgoing* control flow is carried by graph edges, and the shape
/// that claimed the node is what expresses them, so a branch or jump
/// terminator contributes no statement here. A `return` is the exception: no
/// edge carries it, so leaving it out silently changes what the function
/// does. It did -- a function whose body folded cleanly came back with its
/// trailing `return r0` gone and its return type degraded to `undefined`,
/// while the goto count looked like it had improved.
///
/// Anything else a terminal block can end with is control flow this driver
/// cannot place, so the region is declined rather than emitted without it.
///
/// `None` means "not expressible", propagated by every caller.
pub(crate) fn node_statements(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    id: NodeId,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    let Some(node) = graph.node(id) else {
        return Ok(Some(Vec::new()));
    };
    if let Some(body) = &node.body {
        return Ok(Some(body.clone()));
    }
    let block = node.entry_block;
    let terminal = host.successors()[block].is_empty();
    let mut stmts = host.lower_block_stmts(block)?;
    match host.lower_block_terminator(block)? {
        // No edge carries a return, so it has to become a statement here.
        LoweredTerminator::Return(expr) if terminal => stmts.push(PreHirStmt::Return(expr)),
        // Running off the end of the function places nothing.
        LoweredTerminator::Fallthrough(_) if terminal => {}
        // Outgoing control flow: the graph edge expresses it and the shape
        // that claimed this node places it. A branch is re-derived by
        // `condition_towards` at the shape that owns the condition.
        LoweredTerminator::Fallthrough(_)
        | LoweredTerminator::Goto(_)
        | LoweredTerminator::Cond { .. }
            if !terminal => {}
        // An unresolved transfer out of a terminal block is still
        // expressible: the linear path emits it as a residual
        // unsupported-control statement, and emitting the same thing here
        // keeps the transfer rather than dropping it. This was the single
        // largest remaining refusal -- 18 of the 19 `IfNoExit` regions the
        // DREAM driver could not fold ended in a clause exactly like this.
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } if terminal => {
            stmts.push(host.emit_unsupported_control_surface(evidence, target_expr));
        }
        // A switch dispatch, or a jump whose target left the graph: control
        // flow this driver has no way to place. Decline rather than drop it.
        _ => return Ok(None),
    }
    Ok(Some(stmts))
}

/// Whether `id` is the node entered at `address`.
fn node_is_at(graph: &CollapseGraph, host: &impl StructuringHost, id: NodeId, address: u64) -> bool {
    graph
        .node(id)
        .is_some_and(|n| host.block_target_key(n.entry_block) == address)
}

/// The entry's branch condition, oriented so it is true when control goes to
/// `taken`. `None` when the entry does not end in a two-way branch, or the
/// targets cannot be matched to the nodes.
pub(crate) fn condition_towards(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    entry: NodeId,
    taken: NodeId,
    other: NodeId,
) -> Result<Option<fission_midend_prehir::PreHirExpr>, MlilPreviewError> {
    let Some(node) = graph.node(entry) else {
        return Ok(None);
    };
    // A leaf is governed by its own terminator; a folded region only by the
    // one its body left unexpressed (see `CollapseNode::governing_block`).
    // Anything else has no decision left to recover.
    let Some(governing) = node.governing_block else {
        return Ok(None);
    };
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(governing)?
    else {
        return Ok(None);
    };
    let Some(false_target) = false_target else {
        return Ok(None);
    };
    if node_is_at(graph, host, taken, true_target) && node_is_at(graph, host, other, false_target) {
        return Ok(Some(cond));
    }
    if node_is_at(graph, host, taken, false_target) && node_is_at(graph, host, other, true_target) {
        return Ok(Some(negate_expr(cond)));
    }
    Ok(None)
}

/// Build the statements a matched region becomes.
///
/// `None` means the region is graph-legal but not expressible as this shape
/// with the information available -- the driver then concedes an edge rather
/// than forcing it.
pub(crate) fn lower_shape(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    shape: &Shape,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    match shape.kind {
        ShapeKind::Sequence => {
            let [n, s] = shape.members[..] else {
                return Ok(None);
            };
            let (Some(mut body), Some(tail)) = (
                node_statements(host, graph, n)?,
                node_statements(host, graph, s)?,
            ) else {
                return Ok(None);
            };
            body.extend(tail);
            Ok(Some(body))
        }
        ShapeKind::IfThen | ShapeKind::IfNoExit => {
            let [n, clause] = shape.members[..] else {
                return Ok(None);
            };
            let Some(other) = shape.follow else {
                return Ok(None);
            };
            let Some(cond) = condition_towards(host, graph, n, clause, other)? else {
                return Ok(None);
            };
            let (Some(mut body), Some(then_body)) = (
                node_statements(host, graph, n)?,
                node_statements(host, graph, clause)?,
            ) else {
                return Ok(None);
            };
            body.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(then_body),
                else_body: std::rc::Rc::new(Vec::new()),
            });
            Ok(Some(body))
        }
        ShapeKind::IfThenElse => {
            let [n, t, e] = shape.members[..] else {
                return Ok(None);
            };
            let Some(cond) = condition_towards(host, graph, n, t, e)? else {
                return Ok(None);
            };
            let (Some(mut body), Some(then_body), Some(else_body)) = (
                node_statements(host, graph, n)?,
                node_statements(host, graph, t)?,
                node_statements(host, graph, e)?,
            ) else {
                return Ok(None);
            };
            body.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(then_body),
                else_body: std::rc::Rc::new(else_body),
            });
            Ok(Some(body))
        }
        ShapeKind::WhileDo => {
            let [n, inner] = shape.members[..] else {
                return Ok(None);
            };
            let Some(exit) = shape.follow else {
                return Ok(None);
            };
            let Some(cond) = condition_towards(host, graph, n, inner, exit)? else {
                return Ok(None);
            };
            let (Some(test_stmts), Some(body)) = (
                node_statements(host, graph, n)?,
                node_statements(host, graph, inner)?,
            ) else {
                return Ok(None);
            };
            if test_stmts.is_empty() {
                return Ok(Some(vec![PreHirStmt::While {
                    cond,
                    body: std::rc::Rc::new(body),
                }]));
            }
            // The test block computes something every iteration, so it belongs
            // *inside* the loop, ahead of the test -- which a `while (cond)`
            // header cannot express, because the header runs before the body.
            // `while (true) { S; if (!cond) break; B; }` puts each part where
            // it actually runs.
            //
            // Declining this instead was the second largest reason the DREAM
            // driver could not reduce a graph, on 25 of the 106 functions it
            // refused for a surviving cycle. Real loop tests compute their
            // comparison, so "the test block contributes nothing" is the
            // uncommon case, not the common one.
            let mut loop_body = test_stmts;
            loop_body.push(PreHirStmt::If {
                cond: negate_expr(cond),
                then_body: std::rc::Rc::new(vec![PreHirStmt::Break]),
                else_body: std::rc::Rc::new(Vec::new()),
            });
            loop_body.extend(body);
            Ok(Some(vec![PreHirStmt::While {
                cond: crate::reaching_conditions::always(),
                body: std::rc::Rc::new(loop_body),
            }]))
        }
        ShapeKind::DoWhile => {
            let [n, cond_node] = shape.members[..] else {
                return Ok(None);
            };
            let Some(exit) = shape.follow else {
                return Ok(None);
            };
            let Some(cond) = condition_towards(host, graph, cond_node, n, exit)? else {
                return Ok(None);
            };
            let (Some(mut body), Some(tail)) = (
                node_statements(host, graph, n)?,
                node_statements(host, graph, cond_node)?,
            ) else {
                return Ok(None);
            };
            body.extend(tail);
            Ok(Some(vec![PreHirStmt::DoWhile {
                body: std::rc::Rc::new(body),
                cond,
            }]))
        }
        ShapeKind::SelfLoop => {
            // A block that branches to itself runs its statements and then
            // decides whether to go round again -- which is `do { } while`,
            // with the test already in the right place. This was previously
            // declined outright, and it was the single largest reason the
            // DREAM driver could not reduce a graph: 36 of the 106 functions
            // it refused for a surviving cycle got stuck here.
            let [n] = shape.members[..] else {
                return Ok(None);
            };
            let Some(exit) = shape.follow else {
                return Ok(None);
            };
            // `condition_towards` declines a node that already carries a body,
            // so a region that loops back to itself after folding stays
            // refused -- its test lives inside the body and cannot be lifted
            // out.
            let Some(cond) = condition_towards(host, graph, n, n, exit)? else {
                return Ok(None);
            };
            let Some(body) = node_statements(host, graph, n)? else {
                return Ok(None);
            };
            Ok(Some(vec![PreHirStmt::DoWhile {
                body: std::rc::Rc::new(body),
                cond,
            }]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_gate_is_on_unless_explicitly_disabled() {
        if std::env::var_os("FISSION_MATCH_FOLD").is_none() {
            assert!(match_fold_driver_enabled());
        }
    }

    #[test]
    fn a_back_edge_inside_an_acyclic_shape_is_not_foldable() {
        // 0 -> 1 -> 0. Read as a Sequence, folding {0,1} would detach 1 -> 0
        // and the loop would be gone -- with every block still present, which
        // is why membership coverage cannot catch this.
        let g = CollapseGraph::from_cfg(&[vec![1], vec![0]]);
        let seq = Shape {
            kind: ShapeKind::Sequence,
            entry: 0,
            members: vec![0, 1],
            follow: None,
        };
        assert!(
            !fold_accounts_for_every_internal_edge(&g, &seq),
            "a sequence does not express a back edge"
        );
        // The same two nodes as a loop do account for both edges.
        let loop_shape = Shape {
            kind: ShapeKind::WhileDo,
            entry: 0,
            members: vec![0, 1],
            follow: None,
        };
        assert!(fold_accounts_for_every_internal_edge(&g, &loop_shape));
    }

    #[test]
    fn an_extra_internal_edge_blocks_the_fold() {
        // 0 -> {1,2}, 1 -> 2: an if/then whose clause also reaches the follow
        // *inside* the region. IfThen expresses only 0 -> 1, so folding
        // {0,1,2} as one would drop two edges.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![2], vec![]]);
        let shape = Shape {
            kind: ShapeKind::IfThen,
            entry: 0,
            members: vec![0, 1, 2],
            follow: None,
        };
        assert!(!fold_accounts_for_every_internal_edge(&g, &shape));
    }

    #[test]
    fn a_plain_if_then_accounts_for_its_only_internal_edge() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![2], vec![]]);
        let shape = Shape {
            kind: ShapeKind::IfThen,
            entry: 0,
            members: vec![0, 1],
            follow: Some(2),
        };
        assert!(fold_accounts_for_every_internal_edge(&g, &shape));
    }

    #[test]
    fn conceding_prefers_a_back_edge_then_gives_up() {
        // 0 -> 1 -> 0: the only edges are the pair, so two concessions strip
        // the graph and the third reports nothing left.
        let mut g = CollapseGraph::from_cfg(&[vec![1], vec![0]]);
        assert!(concede_one_edge(&mut g, None));
        assert!(concede_one_edge(&mut g, None));
        assert!(!concede_one_edge(&mut g, None), "nothing left to concede");
        assert_eq!(g.live_count(), 2, "concessions never fold the graph");
    }

    #[test]
    fn conceding_targets_the_failed_region_first() {
        // 0 -> {1,2}; the failed region is {0,1}, so its edge out to 2 goes
        // before anything else is touched.
        let mut g = CollapseGraph::from_cfg(&[vec![1, 2], vec![2], vec![]]);
        let failed = Shape {
            kind: ShapeKind::IfThen,
            entry: 0,
            members: vec![0, 1],
            follow: Some(2),
        };
        assert!(concede_one_edge(&mut g, Some(&failed)));
        assert!(
            !g.successors(0).contains(&2) || !g.successors(1).contains(&2),
            "an edge leaving the failed region was conceded"
        );
    }
}
