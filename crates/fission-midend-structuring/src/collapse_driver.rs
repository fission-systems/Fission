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
//! # Not yet safe to wire in
//!
//! The safety argument above is **incomplete, and measurement proved it.**
//! Shape discovery is pure, but [`lower_shape`] calls `lower_block_stmts` and
//! `lower_block_terminator` to build a region's body *before* the driver
//! knows whether it will commit. When it then declines, those lowering side
//! effects have already landed on the host and perturb the existing path that
//! runs next.
//!
//! This was measured, not theorised. Wired in with concessions capped at zero
//! -- so every committed body is jump-free and could only reduce the count --
//! the corpus still moved by +8 gotos with 16 files regressed. Files the
//! driver *declined* changed, which is only possible through leaked lowering.
//!
//! The fix is the clone-and-isolate pattern this codebase already uses for
//! the same hazard (`midend/structuring/host_impl.rs`,
//! `lower_virtual_exit_if_else_isolated`): lower against a cloned host and
//! merge identity bindings back only on success. That needs a host-side entry
//! point, since `StructuringHost` is generic here and cannot be cloned
//! through the trait. Until that exists this module stays unwired.
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

/// Opt in with `FISSION_MATCH_FOLD=1`.
///
/// Off by default while the driver is measured against the existing one. The
/// codebase already uses this pattern for `FISSION_COLLAPSE_LOOP`.
pub fn match_fold_driver_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("FISSION_MATCH_FOLD").as_deref(),
            Ok("1" | "true" | "on" | "yes")
        )
    })
}

/// Structure a whole function by folding its graph to a single node.
///
/// Returns `None` when the graph could not be reduced to one node, leaving
/// the caller on its existing path.
pub fn structure_by_match_fold(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(None);
    }
    let mut graph = CollapseGraph::from_cfg(&successors);
    let budget = successors.len().saturating_mul(MAX_ROUNDS_PER_NODE);
    let mut concessions = 0usize;

    for _ in 0..budget {
        if let Some(sole) = graph.sole_live_node() {
            return Ok(graph
                .node(sole)
                .and_then(|n| n.body.clone())
                .or_else(|| Some(Vec::new())));
        }
        if let Some(shape) = find_shape(&graph) {
            let Some(body) = lower_shape(host, &graph, &shape)? else {
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

/// Statements a node contributes, excluding its terminator.
fn node_statements(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    id: NodeId,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    let Some(node) = graph.node(id) else {
        return Ok(Vec::new());
    };
    if let Some(body) = &node.body {
        return Ok(body.clone());
    }
    host.lower_block_stmts(node.entry_block)
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
fn condition_towards(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    entry: NodeId,
    taken: NodeId,
    other: NodeId,
) -> Result<Option<fission_midend_prehir::PreHirExpr>, MlilPreviewError> {
    let Some(node) = graph.node(entry) else {
        return Ok(None);
    };
    // Only a leaf still carries a real terminator; a folded region's exit is
    // already expressed by its body.
    if node.body.is_some() {
        return Ok(None);
    }
    let LoweredTerminator::Cond {
        cond,
        true_target,
        false_target,
    } = host.lower_block_terminator(node.entry_block)?
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
fn lower_shape(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    shape: &Shape,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    match shape.kind {
        ShapeKind::Sequence => {
            let [n, s] = shape.members[..] else {
                return Ok(None);
            };
            let mut body = node_statements(host, graph, n)?;
            body.extend(node_statements(host, graph, s)?);
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
            let mut body = node_statements(host, graph, n)?;
            body.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(node_statements(host, graph, clause)?),
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
            let mut body = node_statements(host, graph, n)?;
            body.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(node_statements(host, graph, t)?),
                else_body: std::rc::Rc::new(node_statements(host, graph, e)?),
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
            // The test block's own statements run every iteration, so they
            // belong inside the loop, ahead of the condition -- which only
            // reads correctly when the test block contributes nothing.
            if !node_statements(host, graph, n)?.is_empty() {
                return Ok(None);
            }
            Ok(Some(vec![PreHirStmt::While {
                cond,
                body: std::rc::Rc::new(node_statements(host, graph, inner)?),
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
            let mut body = node_statements(host, graph, n)?;
            body.extend(node_statements(host, graph, cond_node)?);
            Ok(Some(vec![PreHirStmt::DoWhile {
                body: std::rc::Rc::new(body),
                cond,
            }]))
        }
        // A self loop's own terminator decides whether it repeats; expressing
        // that needs the break placement the linear lowering already owns.
        ShapeKind::SelfLoop => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_gate_is_off_unless_explicitly_enabled() {
        // The driver must never engage by accident while it is being measured.
        if std::env::var_os("FISSION_MATCH_FOLD").is_none() {
            assert!(!match_fold_driver_enabled());
        }
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
