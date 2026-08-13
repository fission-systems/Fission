//! DREAM structuring: fold the loops, then let the conditions do the rest.
//!
//! The third strategy, alongside Ghidra's rules ([`crate::collapse_structure`])
//! and angr's schemas ([`crate::collapse_driver`]). Those two share a failure
//! mode -- when nothing matches they emit a jump -- and the measurements in
//! this work say that failure mode is what bounds Fission: 80.5% of the
//! remaining gotos target true joins, whose structural floor is above the
//! number Ghidra actually reaches. No amount of shape matching gets under a
//! floor that shape matching creates.
//!
//! DREAM has no such branch. Loops are structured first, and everything left
//! is acyclic, where *every* node can be written down under the condition it
//! runs beneath ([`crate::reaching_conditions`], [`crate::reaching_emit`]).
//! There is no "unmatched" case to concede.
//!
//! # What it declines
//!
//! Having no fallback is not the same as always succeeding. This driver
//! returns `None` -- leaving the caller on its existing path, with host state
//! untouched -- when the region is not one it can describe:
//!
//! - a cycle survives loop folding (irreducible, or a shape not yet matched);
//! - a node branches more than two ways, or its condition cannot be recovered;
//! - the fold loses a block or an edge.
//!
//! Those are honest refusals, not concessions: nothing is emitted, so nothing
//! is emitted wrongly. The obligations in the third bullet are the ones the
//! match-fold driver learned the hard way -- a jump-free body is not a correct
//! one, and goto density scores a deleted region perfectly.

use crate::collapse_driver::{
    blocks_reachable_from_entry, condition_towards, covers_every_reachable_block,
    fold_accounts_for_every_internal_edge, lower_shape, node_statements,
};
use crate::collapse_graph::{CollapseGraph, NodeId};
use crate::collapse_shapes::{Shape, ShapeKind, match_do_while, match_self_loop, match_while_do};
use crate::host::StructuringHost;
use crate::reaching_conditions::{compute_reaching_conditions, topological_order};
use crate::reaching_emit::{
    Branch, emit_acyclic_region, inline_single_use_guards, materialize_branch_conditions,
};
use fission_midend_core::ir::{MlilPreviewError, NirType};
use fission_midend_prehir::PreHirStmt;

/// Ceiling on loop-folding rounds, proportional to graph size.
const MAX_ROUNDS_PER_NODE: usize = 8;

/// How many decisions a region may contain and still be worth describing this
/// way.
///
/// A node's reaching condition is a formula over every decision on the paths
/// that reach it, so the formulas grow with this number -- and so does what
/// the rest of the pipeline then has to chew through. Measured: a 51-block
/// region was structured in 141ms and cost **45 seconds downstream**, against
/// 3.1s for the same function on the existing path. The driver was not slow;
/// what it emitted was.
///
/// This is the safety valve, and it is the *only* one of the two caps that
/// protects anything: raising it to 32 changed the corpus not at all, and 48
/// brought the timeout back. The breaking region therefore has between 33 and
/// 48 decisions, which leaves 32 no margin at all -- so 16 stands, at a little
/// over 2x. The eleven gotos 32 would have bought are not worth a binary that
/// fails to decompile.
const MAX_DECISIONS: usize = 16;

/// Backstop on how deeply the emitted body may nest.
///
/// This used to be 8, and it was the binding constraint on the whole driver:
/// **every goto the cap relaxation bought came from raising it**, and none
/// from the decision count. That is because depth is not really this
/// driver's call to make. `structuring_quality` compares the candidate
/// against the structuring it would replace and allows one level per jump
/// removed, which is the same judgement made with the baseline actually in
/// hand -- so once that comparator exists, a fixed number here can only be
/// wrong in one direction or the other.
///
/// So it is deliberately far above where the comparator bites, and stays only
/// as a backstop against a pathological body reaching the rest of the
/// pipeline at all.
const MAX_NESTING_DEPTH: usize = 64;

/// On by default; opt out with `FISSION_DREAM=0`.
///
/// Measured across 250 functions with both drivers offering candidates: NIR
/// 1629 -> 1587, HIR 1615 -> 1573, eleven functions improved and none
/// regressed.
///
/// Safe to default on only because it no longer runs *first*. Pre-empting the
/// existing path cost `switch` recovery -- a dispatch is a cascade of two-way
/// branches, which this driver describes perfectly as nested `if`s, and goto
/// density scores that as a win. Offering a candidate to
/// `try_alternative_structurings` instead means a structuring that loses a
/// switch is simply never chosen, and the three switch tests pass untouched.
pub fn dream_driver_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("FISSION_DREAM").as_deref(),
            Ok("0" | "false" | "off" | "no")
        )
    })
}

/// Structure a whole function by reaching conditions.
///
/// Runs against a fork of the host and commits only on success, so a decline
/// costs nothing -- see [`StructuringHost::lower_isolated`].
pub fn structure_by_reaching_conditions(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    host.lower_isolated(structure_acyclic_remainder)
}

fn structure_acyclic_remainder(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(None);
    }
    let mut graph = CollapseGraph::from_cfg(&successors);

    // Phase 1: cycles. DREAM applies to acyclic regions, so every loop has to
    // become a single node before the conditions mean anything.
    fold_every_cycle(host, &mut graph)?;

    // Phase 2: the acyclic remainder.
    let dense = dense_successors(&graph);
    let Some(order) = topological_order(&dense) else {
        // A cycle survived folding. Guessing at it is exactly the failure this
        // approach exists to avoid.
        return Ok(None);
    };
    let Some(head) = live_node_owning_entry(&graph) else {
        return Ok(None);
    };

    let Some(branches) = collect_branches(host, &graph)? else {
        return Ok(None);
    };
    // Both the formula sizes and the nesting grow with the decision count,
    // and so does the downstream cost of the result.
    if branches.len() > MAX_DECISIONS {
        return Ok(None);
    }
    // Bind each condition where its branch actually happened; see
    // `reaching_emit`'s soundness note for why this is not optional.
    // Allocate through the host so the guard is a real binding with a
    // declaration, not a name this module invented. The fork's temps are
    // merged back only if the body is committed, so guards from a declined
    // attempt leave nothing behind.
    let mut guards: Vec<String> = Vec::new();
    let bound = {
        let mut mint = |_n: NodeId| {
            let name = host.alloc_temp_binding(NirType::Bool, None);
            guards.push(name.clone());
            name
        };
        materialize_branch_conditions(&branches, &mut mint)
    };
    let is_guard = |name: &str| guards.iter().any(|g| g == name);

    let Ok(reaching) = compute_reaching_conditions(&dense, head, bound.edge_condition()) else {
        return Ok(None);
    };

    // Every live node must have a condition, or emission would silently drop
    // it -- the same class of loss that rewrote a `for` loop as a straight
    // line in the match-fold driver.
    if graph.live_nodes().any(|n| !reaching.contains_key(&n)) {
        return Ok(None);
    }
    if !covers_every_live_block(&graph, &blocks_reachable_from_entry(&successors)) {
        return Ok(None);
    }

    let mut bodies: crate::HashMap<NodeId, Vec<PreHirStmt>> = crate::HashMap::default();
    for n in graph.live_nodes().collect::<Vec<_>>() {
        let Some(mut stmts) = node_statements(host, &graph, n)? else {
            return Ok(None);
        };
        // The condition binding belongs at the end of the block that decided
        // it, before control leaves.
        if let Some((_, binding)) = bound.bindings.iter().find(|(node, _)| *node == n) {
            stmts.push(binding.clone());
        }
        bodies.insert(n, stmts);
    }

    let body = emit_acyclic_region(&order, &reaching, |n| {
        bodies.get(&n).cloned().unwrap_or_default()
    });
    if nesting_depth(&body) > MAX_NESTING_DEPTH {
        return Ok(None);
    }
    // Most guards are consumed by the `if` directly after them; folding those
    // away is what keeps a simple two-block `if` reading as `if (param_1)`
    // rather than through a variable. The rest stay, and are legitimate --
    // they carry a decision to a node that is not adjacent to it.
    let body = inline_single_use_guards(body, &is_guard);
    Ok(Some(body))
}

/// How deeply `body` nests conditionals and loops.
fn nesting_depth(body: &[PreHirStmt]) -> usize {
    fn depth_of(stmt: &PreHirStmt) -> usize {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + nesting_depth(then_body).max(nesting_depth(else_body)),
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                1 + nesting_depth(body)
            }
            PreHirStmt::Block(inner) => nesting_depth(inner),
            _ => 0,
        }
    }
    body.iter().map(depth_of).max().unwrap_or(0)
}

/// Fold loops until none of the cyclic shapes match.
fn fold_every_cycle(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
) -> Result<(), MlilPreviewError> {
    let budget = graph.node_capacity().saturating_mul(MAX_ROUNDS_PER_NODE);
    for _ in 0..budget {
        let Some(shape) = find_cyclic_shape(graph) else {
            return Ok(());
        };
        if !fold_accounts_for_every_internal_edge(graph, &shape) {
            return Ok(());
        }
        let Some(body) = lower_shape(host, graph, &shape)? else {
            return Ok(());
        };
        if graph.collapse(&shape.members, shape.entry, body).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

fn find_cyclic_shape(graph: &CollapseGraph) -> Option<Shape> {
    graph.live_nodes().find_map(|n| {
        match_self_loop(graph, n)
            .or_else(|| match_while_do(graph, n))
            .or_else(|| match_do_while(graph, n))
            .filter(|s| {
                matches!(
                    s.kind,
                    ShapeKind::SelfLoop | ShapeKind::WhileDo | ShapeKind::DoWhile
                )
            })
    })
}

/// Adjacency over the graph's whole index space; retired slots are empty and
/// unreachable, so they cost nothing but keep node ids stable.
fn dense_successors(graph: &CollapseGraph) -> Vec<Vec<NodeId>> {
    (0..graph.node_capacity())
        .map(|n| {
            if graph.is_live(n) {
                graph.successors(n).to_vec()
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// The live node that block 0 ended up inside. Folding a loop whose head is
/// not the function entry retires node 0, so the region head has to be looked
/// up rather than assumed.
fn live_node_owning_entry(graph: &CollapseGraph) -> Option<NodeId> {
    graph
        .live_nodes()
        .find(|&n| graph.node(n).is_some_and(|x| x.members.contains(0)))
}

/// Whether the live nodes between them still own every reachable block.
fn covers_every_live_block(graph: &CollapseGraph, reachable: &[usize]) -> bool {
    reachable.iter().all(|&b| {
        graph
            .live_nodes()
            .any(|n| graph.node(n).is_some_and(|x| x.members.contains(b)))
    })
}

/// Recover the decision at every two-way node.
///
/// `None` when any node branches in a way this driver cannot describe --
/// a switch, or a condition that could not be recovered.
fn collect_branches(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
) -> Result<Option<Vec<Branch>>, MlilPreviewError> {
    let mut branches = Vec::new();
    for n in graph.live_nodes().collect::<Vec<_>>() {
        let outs = graph.successors(n).to_vec();
        match outs.len() {
            0 | 1 => continue,
            2 => {
                let (t, f) = (outs[0], outs[1]);
                let Some(cond) = condition_towards(host, graph, n, t, f)? else {
                    return Ok(None);
                };
                branches.push(Branch {
                    node: n,
                    cond,
                    true_target: t,
                    false_target: f,
                });
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(branches))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_gate_is_on_unless_explicitly_disabled() {
        if std::env::var_os("FISSION_DREAM").is_none() {
            assert!(dream_driver_enabled());
        }
    }

    #[test]
    fn a_retired_slot_contributes_no_edges() {
        // 0 -> 1 -> 2, fold {0,1}: slot 1 is retired and must not appear as a
        // node with successors, or the topological order would count it.
        let mut g = CollapseGraph::from_cfg(&[vec![1], vec![2], vec![]]);
        g.collapse(&[0, 1], 0, Vec::new()).expect("sequence folds");
        let dense = dense_successors(&g);
        assert_eq!(dense.len(), 3, "the index space is unchanged");
        assert!(dense[1].is_empty(), "the retired slot has no edges");
        assert_eq!(dense[0], vec![2]);
    }

    #[test]
    fn the_head_follows_block_zero_into_a_fold() {
        // Fold {0,1} with 1 as the entry: node 0 retires, and the head is the
        // node that now owns block 0 rather than block 0 itself.
        let mut g = CollapseGraph::from_cfg(&[vec![1], vec![0]]);
        g.collapse(&[0, 1], 1, Vec::new()).expect("loop folds");
        let head = live_node_owning_entry(&g).expect("some node owns block 0");
        assert_eq!(head, 1);
        assert!(!g.is_live(0), "block 0's original node is gone");
    }

    #[test]
    fn coverage_notices_a_lost_block() {
        let g = CollapseGraph::from_cfg(&[vec![1], vec![]]);
        assert!(covers_every_live_block(&g, &[0, 1]));
        // Block 2 was never in the graph, so nothing owns it.
        assert!(!covers_every_live_block(&g, &[0, 1, 2]));
    }
}
