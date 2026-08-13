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
use crate::collapse_shapes::{
    Shape, ShapeKind, find_shape, match_do_while, match_self_loop, match_while_do,
};
use crate::host::StructuringHost;
use crate::reaching_conditions::{compute_reaching_conditions, topological_order};
use crate::reaching_emit::{
    Branch, emit_acyclic_region, inline_single_use_guards, materialize_branch_conditions,
};
use fission_midend_core::ir::{MlilPreviewError, NirType};
use fission_midend_prehir::PreHirStmt;

/// Why a region could not be described by reaching conditions.
///
/// Named rather than collapsed into a bare `None` because *which* refusal
/// fires is the only thing that says where the remaining work is. The driver
/// offers a candidate for a minority of functions, and guessing at why the
/// rest are refused has been wrong often enough in this work to be worth the
/// enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclineReason {
    /// No CFG at all.
    EmptyGraph,
    /// A cycle survived loop folding -- irreducible, or a loop shape
    /// `collapse_shapes` does not match.
    CycleSurvived,
    /// Nothing owns the function entry after folding.
    NoEntry,
    /// A node branches more than two ways: a switch dispatch.
    MultiwayBranch,
    /// A two-way branch whose condition could not be recovered.
    ConditionUnrecoverable,
    /// More decisions than `MAX_DECISIONS`.
    TooManyDecisions,
    /// The region is cyclic as far as the condition solver is concerned.
    ReachingFailed,
    /// A live node came out with no condition at all.
    NodeWithoutCondition,
    /// Folding lost a reachable block.
    CoverageLost,
    /// A terminator this driver cannot place.
    UnplaceableTerminator,
    /// Deeper than `MAX_NESTING_DEPTH`.
    TooDeep,
    /// Guards larger than `MAX_GUARD_FORMULA_SIZE`.
    GuardsTooLarge,
}

impl DeclineReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyGraph => "empty-graph",
            Self::CycleSurvived => "cycle-survived",
            Self::NoEntry => "no-entry",
            Self::MultiwayBranch => "multiway-branch",
            Self::ConditionUnrecoverable => "condition-unrecoverable",
            Self::TooManyDecisions => "too-many-decisions",
            Self::ReachingFailed => "reaching-failed",
            Self::NodeWithoutCondition => "node-without-condition",
            Self::CoverageLost => "coverage-lost",
            Self::UnplaceableTerminator => "unplaceable-terminator",
            Self::TooDeep => "too-deep",
            Self::GuardsTooLarge => "guards-too-large",
        }
    }
}

/// Ceiling on loop-folding rounds, proportional to graph size.
const MAX_ROUNDS_PER_NODE: usize = 8;

/// Backstop on how many edges may be conceded to a jump to break a cycle
/// nothing matches.
///
/// angr's `_last_resort_refinement`. Conceding is not free -- each one is a
/// `goto` -- but it is not measured against zero either: `structuring_quality`
/// compares against whatever the existing path produced for the same function,
/// and these are precisely the functions it leaves badly unstructured. Two
/// jumps beating ten is a win.
///
/// This started at 3 on the reasoning that a fold needing many concessions is
/// describing a graph the driver has no grip on. Measured, that reasoning was
/// protecting against something the comparator already handles:
///
/// | budget | 3 | 8 | 24 | 128 | none |
/// |---|---|---|---|---|---|
/// | corpus | -175 | -221 | -232 | -244 | -244 |
///
/// One file regresses at every setting, the same one, for a reason downstream
/// of structuring entirely -- so the extra concessions cost nothing and the
/// comparator turns down the folds that would. Runtime is flat throughout.
///
/// 128 is where the corpus stops changing, and removing the cap does not move
/// it. It stays finite as a bound on a graph unlike anything measured here;
/// the fold loop is separately bounded by `MAX_ROUNDS_PER_NODE`.
const MAX_CONCESSIONS: usize = 128;

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
/// It is a *proxy*, though, and a poor one: the region that caused the 45
/// seconds has only 39 decisions and nests 9 deep -- both unremarkable --
/// while its guards total 160,423 expression nodes. Capping decisions low
/// enough to exclude it also excluded healthy regions with twice the decisions
/// and a four-hundredth of the guard weight. `MAX_GUARD_FORMULA_SIZE` bounds
/// the real quantity, so this can sit high and only stop a region from being
/// described at all.
const MAX_DECISIONS: usize = 64;

/// How large the guards of an emitted body may total, in expression nodes.
///
/// The bound on what condition-based structuring actually costs, replacing two
/// proxies that could not see it. Measured across every candidate this driver
/// produced on the corpus: healthy ones run from 2 to 2,008 nodes, and the one
/// that cost 45 seconds downstream is **160,423** -- eighty times the largest
/// healthy case. There is no ambiguity to tune against, unlike the decision
/// count whose good and bad cases overlapped between 33 and 48.
///
/// 8,000 sits four times above the largest healthy candidate and twenty times
/// below the pathological one.
const MAX_GUARD_FORMULA_SIZE: usize = 8_000;

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
    let outcome = describe_region(host)?;
    if crate::linear_types::structuring_diag_enabled() {
        match &outcome {
            Ok(body) => eprintln!("[DIAG] DREAM offered: stmts={}", body.len()),
            Err(reason) => eprintln!("[DIAG] DREAM declined: {}", reason.as_str()),
        }
    }
    Ok(outcome.ok())
}

fn describe_region(
    host: &mut impl StructuringHost,
) -> Result<Result<Vec<PreHirStmt>, DeclineReason>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(Err(DeclineReason::EmptyGraph));
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
        return Ok(Err(DeclineReason::CycleSurvived));
    };
    let Some(head) = live_node_owning_entry(&graph) else {
        return Ok(Err(DeclineReason::NoEntry));
    };

    let branches = match collect_branches(host, &graph)? {
        Ok(b) => b,
        Err(reason) => return Ok(Err(reason)),
    };
    // Both the formula sizes and the nesting grow with the decision count,
    // and so does the downstream cost of the result.
    if branches.len() > MAX_DECISIONS {
        return Ok(Err(DeclineReason::TooManyDecisions));
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
        return Ok(Err(DeclineReason::ReachingFailed));
    };

    // Every live node must have a condition, or emission would silently drop
    // it -- the same class of loss that rewrote a `for` loop as a straight
    // line in the match-fold driver.
    if graph.live_nodes().any(|n| !reaching.contains_key(&n)) {
        return Ok(Err(DeclineReason::NodeWithoutCondition));
    }
    if !covers_every_live_block(&graph, &blocks_reachable_from_entry(&successors)) {
        return Ok(Err(DeclineReason::CoverageLost));
    }

    let mut bodies: crate::HashMap<NodeId, Vec<PreHirStmt>> = crate::HashMap::default();
    for n in graph.live_nodes().collect::<Vec<_>>() {
        let Some(mut stmts) = node_statements(host, &graph, n)? else {
            return Ok(Err(DeclineReason::UnplaceableTerminator));
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
        return Ok(Err(DeclineReason::TooDeep));
    }
    // The real cost bound: what the rest of the pipeline has to carry.
    if crate::structuring_quality::guard_formula_size(&body) > MAX_GUARD_FORMULA_SIZE {
        return Ok(Err(DeclineReason::GuardsTooLarge));
    }
    // Most guards are consumed by the `if` directly after them; folding those
    // away is what keeps a simple two-block `if` reading as `if (param_1)`
    // rather than through a variable. The rest stay, and are legitimate --
    // they carry a decision to a node that is not adjacent to it.
    let body = inline_single_use_guards(body, &is_guard);
    Ok(Ok(body))
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

/// Fold shapes until the graph has no cycle left.
///
/// Not just the cyclic shapes. Every cyclic shape `collapse_shapes` matches is
/// two nodes -- `WhileDo` is a test and a body, `DoWhile` a body and a latch --
/// so a loop whose body is more than one block matches none of them until that
/// body has itself been folded into a single node. Reducing the body is
/// acyclic work: a `Sequence`, an `IfThen`. Folding only cyclic shapes
/// therefore cannot structure any loop containing an `if`, which is close to
/// all of them.
///
/// Measured before this was fixed: **106 of the 144 functions the driver was
/// asked about were refused with a surviving cycle** -- three quarters of every
/// refusal, and more than four times every other cause combined.
///
/// Stopping as soon as the graph is acyclic is what keeps this from becoming
/// the match-fold driver: everything still standing at that point is handed to
/// the reaching conditions, which is the whole point of the strategy.
fn fold_every_cycle(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
) -> Result<(), MlilPreviewError> {
    let budget = graph.node_capacity().saturating_mul(MAX_ROUNDS_PER_NODE);
    let mut concessions = 0usize;
    for _ in 0..budget {
        if !graph_has_cycle(graph) {
            return Ok(());
        }
        // Prefer a loop shape when one matches, so a loop is closed as soon as
        // its body is reducible rather than after unrelated folding.
        let shape = match find_cyclic_shape(graph).or_else(|| find_shape(graph)) {
            Some(shape) => shape,
            None => {
                // Nothing matches and a cycle remains. Concede one edge to a
                // jump so the next round faces a different graph, rather than
                // abandoning a function the existing path will structure worse.
                if concessions >= MAX_CONCESSIONS || !concede_one_edge(host, graph)? {
                    return Ok(());
                }
                concessions += 1;
                continue;
            }
        };
        if !fold_accounts_for_every_internal_edge(graph, &shape) {
            return Ok(());
        }
        let Some(body) = lower_shape(host, graph, &shape)? else {
            return Ok(());
        };
        // Read before folding: `collapse` retires the member nodes, so the
        // last member's governing block is unreachable afterwards.
        let governing = governing_block_after(graph, &shape);
        let Ok(folded) = graph.collapse(&shape.members, shape.entry, body) else {
            return Ok(());
        };
        graph.set_governing_block(folded, governing);
    }
    Ok(())
}

/// Which block's terminator still decides where a folded region goes.
///
/// Only a `Sequence` keeps one. Its last member's branch was never turned
/// into a statement -- the graph edge carried it -- so the region still leaves
/// through that terminator and a later rule can recover the condition. Every
/// other shape ends with its exit expressed by the body it emitted, so there
/// is nothing left to ask, and `None` makes the next rule decline rather than
/// read a terminator whose decision has already been written down.
fn governing_block_after(graph: &CollapseGraph, shape: &Shape) -> Option<usize> {
    if shape.kind != ShapeKind::Sequence {
        return None;
    }
    let last = *shape.members.last()?;
    graph_member_governing(graph, last)
}

fn graph_member_governing(graph: &CollapseGraph, id: NodeId) -> Option<usize> {
    graph.node(id).and_then(|n| n.governing_block)
}

/// Turn one edge into a `goto`/`label` pair and remove it from the graph.
///
/// The edge stops being control flow the shapes have to account for and
/// becomes a statement instead, which is the only way a cycle nothing matches
/// can be broken. Prefers a back edge -- the classic unstructured jump, and the
/// one a reader least minds seeing.
///
/// Both ends are materialised: the source's body gains the jump (guarded by
/// its own condition when it was a two-way branch, so the remaining edge still
/// means what it meant), and the target's gains the label. After this the
/// source's decision is written down rather than pending, so it stops
/// governing anything.
fn concede_one_edge(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
) -> Result<bool, MlilPreviewError> {
    let Some((from, to)) = pick_conceded_edge(graph) else {
        return Ok(false);
    };
    let Some(entry_block) = graph.node(to).map(|n| n.entry_block) else {
        return Ok(false);
    };
    let label = format!("dream_L{entry_block}");

    let outs: Vec<NodeId> = graph.successors(from).to_vec();
    let Some(mut source) = node_statements(host, graph, from)? else {
        return Ok(false);
    };
    match outs.len() {
        1 => source.push(PreHirStmt::Goto(label.clone())),
        2 => {
            let Some(other) = outs.iter().copied().find(|s| *s != to) else {
                return Ok(false);
            };
            let Some(cond) = condition_towards(host, graph, from, to, other)? else {
                return Ok(false);
            };
            source.push(PreHirStmt::If {
                cond,
                then_body: std::rc::Rc::new(vec![PreHirStmt::Goto(label.clone())]),
                else_body: std::rc::Rc::new(Vec::new()),
            });
        }
        _ => return Ok(false),
    }

    let Some(mut target) = node_statements(host, graph, to)? else {
        return Ok(false);
    };
    target.insert(0, PreHirStmt::Label(label));
    let target_governing = graph.node(to).and_then(|n| n.governing_block);

    graph.set_body(from, source);
    // The branch is now a statement, so there is no decision left to read off
    // this node's terminator.
    graph.set_governing_block(from, None);
    graph.set_body(to, target);
    graph.set_governing_block(to, target_governing);
    Ok(graph.virtualize_edge(from, to))
}

/// The edge to give up.
///
/// Prefers a back edge -- the classic unstructured jump, and the one a reader
/// least minds seeing -- among edges whose removal leaves every live node
/// still reachable from the entry.
///
/// That second condition is not cosmetic. Reaching conditions are computed
/// from edges, so a node the graph can no longer reach gets no condition and
/// the whole region is declined; the jump still arrives there, but the
/// arithmetic cannot see it. Requiring the target to merely keep *a*
/// predecessor is not enough, because concessions compound: each one can be
/// legal on its own and the last of them still strips a node's only remaining
/// way in. Measured, that left this the largest refusal of all at 36 functions.
fn pick_conceded_edge(graph: &CollapseGraph) -> Option<(NodeId, NodeId)> {
    let head = live_node_owning_entry(graph)?;
    let live: Vec<NodeId> = graph.live_nodes().collect();
    let mut best: Option<(NodeId, NodeId)> = None;
    for &u in &live {
        for &v in graph.successors(u) {
            if graph.successors(u).len() > 2 || !survives_without(graph, head, &live, u, v) {
                continue;
            }
            // A back edge is preferred; anything else is a fallback.
            if v <= u {
                return Some((u, v));
            }
            best.get_or_insert((u, v));
        }
    }
    best
}

/// Whether every live node is still reachable from `head` once `(from, to)`
/// is gone.
fn survives_without(
    graph: &CollapseGraph,
    head: NodeId,
    live: &[NodeId],
    from: NodeId,
    to: NodeId,
) -> bool {
    let mut seen = vec![false; graph.node_capacity()];
    let mut stack = vec![head];
    if head >= seen.len() {
        return false;
    }
    seen[head] = true;
    while let Some(n) = stack.pop() {
        for &s in graph.successors(n) {
            if (n, s) == (from, to) || s >= seen.len() || seen[s] {
                continue;
            }
            seen[s] = true;
            stack.push(s);
        }
    }
    live.iter().all(|&n| seen[n])
}

fn graph_has_cycle(graph: &CollapseGraph) -> bool {
    topological_order(&dense_successors(graph)).is_none()
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
) -> Result<Result<Vec<Branch>, DeclineReason>, MlilPreviewError> {
    let mut branches = Vec::new();
    for n in graph.live_nodes().collect::<Vec<_>>() {
        let outs = graph.successors(n).to_vec();
        match outs.len() {
            0 | 1 => continue,
            2 => {
                let (t, f) = (outs[0], outs[1]);
                let Some(cond) = condition_towards(host, graph, n, t, f)? else {
                    return Ok(Err(DeclineReason::ConditionUnrecoverable));
                };
                branches.push(Branch {
                    node: n,
                    cond,
                    true_target: t,
                    false_target: f,
                });
            }
            _ => return Ok(Err(DeclineReason::MultiwayBranch)),
        }
    }
    Ok(Ok(branches))
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
