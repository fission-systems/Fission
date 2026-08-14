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

use crate::cfg_analysis::{CommonPostdominator, ImmPostDomTree};
use crate::collapse_driver::{
    blocks_reachable_from_entry, condition_towards, covers_every_reachable_block,
    fold_accounts_for_every_internal_edge, lower_shape, node_statements,
};
use crate::collapse_graph::{CollapseGraph, NodeId};
use crate::collapse_shapes::{
    Shape, ShapeKind, find_shape, match_do_while, match_inf_loop, match_ring_loop,
    match_self_loop, match_while_do,
};
use crate::host::StructuringHost;
use crate::linear_types::LoweredTerminator;
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

/// On by default; opt out independently while comparing the multi-terminal
/// candidate against the established DREAM variants.
pub fn region_identifier_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("FISSION_REGION_IDENTIFIER").as_deref(),
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
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_isolated(|isolated| structure_acyclic_remainder(isolated, false, false))
}

/// Structure with internal terminal gotos that an earlier pass virtualized.
///
/// This is a separate candidate from the legacy reaching-condition path. The
/// caller compares both before committing either, so materializing a hidden
/// transfer cannot displace a candidate that already structured cleanly.
pub fn structure_by_reaching_conditions_with_virtual_gotos(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_isolated(|isolated| structure_acyclic_remainder(isolated, true, false))
}

/// Structure after first abstracting proven acyclic SESE subregions.
pub fn structure_by_hierarchical_reaching_conditions(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_isolated(|isolated| structure_acyclic_remainder(isolated, false, true))
}

/// Hierarchical reaching conditions plus explicit virtualized transfers.
pub fn structure_by_hierarchical_reaching_conditions_with_virtual_gotos(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_isolated(|isolated| structure_acyclic_remainder(isolated, true, true))
}

/// Price a reaching-condition candidate without committing minted identities.
///
/// The body is valid only for comparison. Once admitted, the caller reruns
/// [`structure_by_reaching_conditions`] so only the stable winner can affect
/// host state or become the emitted body.
pub fn preview_reaching_conditions(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_observed(|observed| structure_acyclic_remainder(observed, false, false))
}

/// Price the virtual-goto reaching-condition variant without committing it.
pub fn preview_reaching_conditions_with_virtual_gotos(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_observed(|observed| structure_acyclic_remainder(observed, true, false))
}

/// Price the hierarchical reaching-condition variant without committing it.
pub fn preview_hierarchical_reaching_conditions(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_observed(|observed| structure_acyclic_remainder(observed, false, true))
}

/// Price hierarchical reaching conditions with virtualized transfers.
pub fn preview_hierarchical_reaching_conditions_with_virtual_gotos(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_observed(|observed| structure_acyclic_remainder(observed, true, true))
}

/// Structure an identified multi-terminal acyclic region recursively.
///
/// Region discovery is pure. Statement lowering and every explicit edge
/// concession run only inside the caller's observed/isolated host.
pub fn structure_by_region_identifier(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_isolated(structure_multi_terminal_region)
}

/// Price the RegionIdentifier-style candidate without committing identities.
pub fn preview_region_identifier(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    host.lower_observed(structure_multi_terminal_region)
}

fn structure_multi_terminal_region(
    host: &mut impl StructuringHost,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    let successors = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(None);
    }
    let reachable = blocks_reachable_from_entry(&successors);
    let mut graph = CollapseGraph::from_cfg(&successors);
    materialize_virtual_gotos(host, &mut graph)?;
    fold_every_cycle(host, &mut graph)?;
    if graph_has_cycle(&graph) {
        return Ok(None);
    }

    let hierarchical_regions = fold_acyclic_sese_regions(host, &mut graph)?;
    let Some(root) = find_acyclic_region(&graph) else {
        return Ok(None);
    };
    let Some(entry) = live_node_owning_entry(&graph) else {
        return Ok(None);
    };
    if root.frontier != AcyclicRegionFrontier::VirtualExit
        || root.members.len() != graph.live_count()
        || root.head != entry
    {
        return Ok(None);
    }

    let budget = graph.node_capacity().saturating_mul(MAX_ROUNDS_PER_NODE);
    let mut concessions = 0usize;
    for _ in 0..budget {
        if let Some(sole) = graph.sole_live_node() {
            if !covers_every_live_block(&graph, &reachable) {
                return Ok(None);
            }
            let Some(node) = graph.node(sole) else {
                return Ok(None);
            };
            let body = node.body.clone().unwrap_or_default();
            if crate::linear_types::structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] RegionIdentifier offered: stmts={} subregions={} concessions={concessions}",
                    body.len(),
                    hierarchical_regions
                );
            }
            return Ok(Some(ReachingCandidate {
                body,
                used_hierarchical_regions: true,
            }));
        }

        let mut progressed = false;
        if let Some(shape) = find_shape(&graph) {
            if fold_accounts_for_every_internal_edge(&graph, &shape) {
                if let Some(body) = lower_shape(host, &graph, &shape)? {
                    let governing = governing_block_after(&graph, &shape);
                    if let Ok(folded) = graph.collapse(&shape.members, shape.entry, body) {
                        graph.set_governing_block(folded, governing);
                        progressed = true;
                    }
                }
            }
        }
        if progressed {
            continue;
        }
        if concessions >= MAX_CONCESSIONS || !concede_one_edge(host, &mut graph)? {
            return Ok(None);
        }
        concessions += 1;
    }
    Ok(None)
}

fn structure_acyclic_remainder(
    host: &mut impl StructuringHost,
    materialize_virtual_transfers: bool,
    hierarchical: bool,
) -> Result<Option<ReachingCandidate>, MlilPreviewError> {
    let outcome = describe_region(host, materialize_virtual_transfers, hierarchical)?;
    if crate::linear_types::structuring_diag_enabled() {
        let name = match (hierarchical, materialize_virtual_transfers) {
            (false, false) => "DREAM",
            (false, true) => "DREAM+virtual-gotos",
            (true, false) => "DREAM-hierarchical",
            (true, true) => "DREAM-hierarchical+virtual-gotos",
        };
        match &outcome {
            Ok(candidate) => eprintln!(
                "[DIAG] {name} offered: stmts={} hierarchical={}",
                candidate.body.len(),
                candidate.used_hierarchical_regions
            ),
            Err(reason) => eprintln!("[DIAG] {name} declined: {}", reason.as_str()),
        }
    }
    Ok(outcome.ok())
}

fn describe_region(
    host: &mut impl StructuringHost,
    materialize_virtual_transfers: bool,
    hierarchical: bool,
) -> Result<Result<ReachingCandidate, DeclineReason>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(Err(DeclineReason::EmptyGraph));
    }
    let mut graph = CollapseGraph::from_cfg(&successors);

    // Phase 0: one separately priced variant restores edges already
    // virtualised before structuring ran. Keeping this out of the legacy
    // candidate preserves functions that already structured more cleanly
    // without the extra explicit transfers.
    if materialize_virtual_transfers {
        materialize_virtual_gotos(host, &mut graph)?;
    }

    // Phase 1: cycles. DREAM applies to acyclic regions, so every loop has to
    // become a single node before the conditions mean anything.
    fold_every_cycle(host, &mut graph)?;

    // angr's RegionIdentifier does not hand the entire acyclic function to
    // one condition system. It abstracts the smallest closed regions first,
    // so decisions local to a diamond stop at its follow instead of becoming
    // terms in every later node's path formula. Do the same over the live
    // graph after loops have been folded.
    let hierarchical_regions = if hierarchical {
        fold_acyclic_sese_regions(host, &mut graph)?
    } else {
        0
    };

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
    Ok(Ok(ReachingCandidate {
        body,
        used_hierarchical_regions: hierarchical_regions > 0,
    }))
}

/// A reaching-condition body plus the admission fact introduced by recursive
/// acyclic region abstraction.
#[derive(Debug, Clone)]
pub struct ReachingCandidate {
    pub body: Vec<PreHirStmt>,
    pub used_hierarchical_regions: bool,
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
                    if crate::linear_types::structuring_diag_enabled() {
                        eprintln!(
                            "[DIAG] DREAM cycle fold stopped: reason=concession-unavailable concessions={concessions} live_nodes={}",
                            graph.live_count()
                        );
                    }
                    return Ok(());
                }
                concessions += 1;
                continue;
            }
        };
        if !fold_accounts_for_every_internal_edge(graph, &shape) {
            if crate::linear_types::structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] DREAM cycle fold stopped: reason=unaccounted-edge shape={:?} members={:?}",
                    shape.kind, shape.members
                );
            }
            return Ok(());
        }
        let Some(body) = lower_shape(host, graph, &shape)? else {
            if crate::linear_types::structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] DREAM cycle fold stopped: reason=unlowerable-shape shape={:?} members={:?}",
                    shape.kind, shape.members
                );
            }
            return Ok(());
        };
        // Read before folding: `collapse` retires the member nodes, so the
        // last member's governing block is unreachable afterwards.
        let governing = governing_block_after(graph, &shape);
        let Ok(folded) = graph.collapse(&shape.members, shape.entry, body) else {
            if crate::linear_types::structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] DREAM cycle fold stopped: reason=collapse-rejected shape={:?} members={:?}",
                    shape.kind, shape.members
                );
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct VirtualGotoFact {
    source: NodeId,
    target: NodeId,
    label: String,
}

fn apply_virtual_goto_facts(
    bodies: &mut crate::HashMap<NodeId, Vec<PreHirStmt>>,
    facts: &[VirtualGotoFact],
) -> bool {
    for fact in facts {
        let Some(source) = bodies.get_mut(&fact.source) else {
            return false;
        };
        source.push(PreHirStmt::Goto(fact.label.clone()));

        let Some(target) = bodies.get_mut(&fact.target) else {
            return false;
        };
        if !matches!(target.first(), Some(PreHirStmt::Label(label)) if *label == fact.label) {
            target.insert(0, PreHirStmt::Label(fact.label.clone()));
        }
    }
    true
}

/// Write out the jumps whose edges were removed before this driver ever saw
/// the graph.
///
/// A block can have no CFG successors while its terminator still names a
/// target: `IrreducibleReductionPass` virtualises edges to break irreducible
/// cycles, taking them out of `successors` and leaving the jump implicit. The
/// shapes then see a terminal node, and `node_statements` refuses it because a
/// `Goto` is control flow it cannot place -- which blocked `Sequence` and
/// `IfNoExit` regions and declined the whole function.
///
/// Measured: every one of the 30 such terminators resolves to a block in the
/// same function. They were never tail calls; the jump simply had nowhere to be
/// written down. This is the same goto/label materialisation
/// [`concede_one_edge`] does, minus removing an edge that is already gone.
fn materialize_virtual_gotos(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
) -> Result<(), MlilPreviewError> {
    let mut facts = Vec::new();
    for id in graph.live_nodes().collect::<Vec<_>>() {
        let Some(node) = graph.node(id) else { continue };
        if node.body.is_some() {
            continue;
        }
        let block = node.entry_block;
        if !host.successors()[block].is_empty() {
            continue;
        }
        let LoweredTerminator::Goto(target) = host.lower_block_terminator(block)? else {
            continue;
        };
        let Some(target_block) = host.find_block_index_by_address(target) else {
            continue;
        };
        let Some(target_node) = graph.live_nodes().find(|&n| {
            graph
                .node(n)
                .is_some_and(|x| x.members.contains(target_block))
        }) else {
            continue;
        };
        let label = crate::helpers::block_label(host.block_start_address(target_block));
        facts.push(VirtualGotoFact {
            source: id,
            target: target_node,
            label,
        });
    }
    if facts.is_empty() {
        return Ok(());
    }

    // Collect every fact before lowering any body. A target may itself be a
    // virtual-goto source; marking it materialized while walking the first
    // edge would otherwise hide its own outgoing transfer.
    let sources: crate::HashSet<NodeId> = facts.iter().map(|fact| fact.source).collect();
    let mut touched: Vec<NodeId> = facts
        .iter()
        .flat_map(|fact| [fact.source, fact.target])
        .collect();
    touched.sort_unstable();
    touched.dedup();

    let mut bodies: crate::HashMap<NodeId, Vec<PreHirStmt>> = crate::HashMap::default();
    let mut governing = crate::HashMap::default();
    for id in touched.iter().copied() {
        let Some(node) = graph.node(id) else {
            return Ok(());
        };
        let block = node.entry_block;
        let body = if let Some(body) = &node.body {
            body.clone()
        } else if sources.contains(&id) {
            // The source terminator was proved above to be the internal Goto
            // this routine places. Ordinary node lowering must reject that
            // terminator because it has no graph edge; lower only the source's
            // non-control statements here and append the transfer below.
            host.lower_block_stmts(block)?
        } else {
            let Some(body) = node_statements(host, graph, id)? else {
                return Ok(());
            };
            body
        };
        bodies.insert(id, body);
        governing.insert(id, node.governing_block);
    }

    if !apply_virtual_goto_facts(&mut bodies, &facts) {
        return Ok(());
    }
    for id in touched {
        let Some(body) = bodies.remove(&id) else {
            return Ok(());
        };
        graph.set_body(id, body);
        graph.set_governing_block(
            id,
            if sources.contains(&id) {
                None
            } else {
                governing.get(&id).copied().flatten()
            },
        );
    }
    Ok(())
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
    if !matches!(target.first(), Some(PreHirStmt::Label(existing)) if *existing == label) {
        target.insert(0, PreHirStmt::Label(label));
    }
    let target_governing = graph.node(to).and_then(|n| n.governing_block);

    graph.set_body(from, source);
    // The branch is now a statement, so there is no decision left to read off
    // this node's terminator.
    graph.set_governing_block(from, None);
    graph.set_body(to, target);
    graph.set_governing_block(to, target_governing);
    let removed = graph.virtualize_edge(from, to);
    if removed && crate::linear_types::structuring_diag_enabled() {
        eprintln!(
            "[DIAG] explicit edge concession: {from} -> {to} remaining_succ={:?}",
            graph.successors(from)
        );
    }
    Ok(removed)
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AcyclicRegion {
    head: NodeId,
    frontier: AcyclicRegionFrontier,
    members: Vec<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcyclicRegionFrontier {
    Block(NodeId),
    VirtualExit,
}

struct CompactLiveCfg {
    nodes: Vec<NodeId>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
}

fn compact_live_cfg(graph: &CollapseGraph) -> CompactLiveCfg {
    let nodes = graph.live_nodes().collect::<Vec<_>>();
    let dense_by_node: crate::HashMap<NodeId, usize> = nodes
        .iter()
        .copied()
        .enumerate()
        .map(|(dense, node)| (node, dense))
        .collect();
    let mut successors = vec![Vec::new(); nodes.len()];
    let mut predecessors = vec![Vec::new(); nodes.len()];
    for (dense, &node) in nodes.iter().enumerate() {
        for succ in graph.successors(node) {
            let Some(&succ_dense) = dense_by_node.get(succ) else {
                continue;
            };
            successors[dense].push(succ_dense);
            predecessors[succ_dense].push(dense);
        }
    }
    CompactLiveCfg {
        nodes,
        successors,
        predecessors,
    }
}

/// Find the smallest closed, single-entry acyclic region on a postdominator
/// chain. A real block frontier forms an ordinary SESE region; a synthetic
/// frontier proves that every terminal in the region reaches `VirtualExit`.
fn find_acyclic_region(graph: &CollapseGraph) -> Option<AcyclicRegion> {
    find_acyclic_region_filtered(graph, false)
}

/// Smallest closed single-entry acyclic region, optionally restricted to those
/// with a real block frontier.
///
/// The restriction matters because a `VirtualExit` frontier is not a region
/// this decomposition can fold -- it is the outer multi-terminal region, which
/// the schema loop handles. Without the filter the caller stops at the first
/// one it meets, and every foldable region elsewhere in the graph goes
/// unexamined. Measured, that left residual formulas of 26,000 to 458,000
/// nodes on the worst functions, against a viable bound of 8,000.
fn find_acyclic_region_filtered(
    graph: &CollapseGraph,
    block_frontier_only: bool,
) -> Option<AcyclicRegion> {
    let compact = compact_live_cfg(graph);
    let order = topological_order(&compact.successors)?;
    let postdom = ImmPostDomTree::compute(&compact.successors, &compact.predecessors);
    let mut best: Option<AcyclicRegion> = None;

    // Reverse topological order approximates angr's deterministic DFS
    // postorder and exposes the innermost region before its parents.
    for &head_dense in order.iter().rev() {
        if compact.successors[head_dense].is_empty() {
            continue;
        }
        let head = compact.nodes[head_dense];
        let mut frontier = postdom.immediate_postdominator_target(head_dense);
        while let Some(target) = frontier {
            let stop = match target {
                CommonPostdominator::Block(block) => Some(block),
                CommonPostdominator::VirtualExit => None,
            };
            let members_dense = reachable_without_dense(&compact.successors, head_dense, stop);
            if members_dense.len() >= 2 {
                let member_dense_set: crate::HashSet<usize> =
                    members_dense.iter().copied().collect();
                let members = members_dense
                    .iter()
                    .map(|&dense| compact.nodes[dense])
                    .collect::<Vec<_>>();
                let closed = members_dense.iter().all(|&node| {
                    compact.successors[node]
                        .iter()
                        .all(|succ| member_dense_set.contains(succ) || Some(*succ) == stop)
                });
                if closed && graph.check_single_entry(&members, head).is_ok() {
                    let frontier = match target {
                        CommonPostdominator::Block(block) => {
                            AcyclicRegionFrontier::Block(compact.nodes[block])
                        }
                        CommonPostdominator::VirtualExit => AcyclicRegionFrontier::VirtualExit,
                    };
                    // Not foldable here, and the postdominator chain ends at
                    // the virtual exit -- move to the next head rather than
                    // abandoning the whole search.
                    if block_frontier_only && frontier == AcyclicRegionFrontier::VirtualExit {
                        break;
                    }
                    let candidate = AcyclicRegion {
                        head,
                        frontier,
                        members,
                    };
                    let key = (candidate.members.len(), head_dense, head);
                    let better = best.as_ref().is_none_or(|current| {
                        let current_dense = compact
                            .nodes
                            .iter()
                            .position(|node| *node == current.head)
                            .unwrap_or(usize::MAX);
                        key < (current.members.len(), current_dense, current.head)
                    });
                    if better {
                        best = Some(candidate);
                    }
                    break;
                }
            }
            frontier = match target {
                CommonPostdominator::Block(block) => postdom.immediate_postdominator_target(block),
                CommonPostdominator::VirtualExit => None,
            };
        }
    }
    best
}

fn reachable_without_dense(
    successors: &[Vec<usize>],
    start: usize,
    stop: Option<usize>,
) -> Vec<usize> {
    let mut seen = vec![false; successors.len()];
    let mut members = Vec::new();
    let mut stack = vec![start];
    while let Some(n) = stack.pop() {
        if Some(n) == stop || n >= seen.len() || seen[n] {
            continue;
        }
        seen[n] = true;
        members.push(n);
        stack.extend(successors[n].iter().copied());
    }
    members
}

/// Abstract closed acyclic regions until no further proof succeeds.
fn fold_acyclic_sese_regions(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
) -> Result<usize, MlilPreviewError> {
    let budget = graph.node_capacity();
    let mut folded_count = 0usize;
    for _ in 0..budget {
        let Some(region) = find_acyclic_region_filtered(graph, true) else {
            break;
        };
        // Lowering is still effectful even after the graph proof. Run the
        // region on a nested isolated host and commit identities only when its
        // complete body is placeable and within the downstream cost bounds.
        let body = host.lower_isolated(|isolated| {
            lower_acyclic_members(isolated, graph, &region.members, region.head)
                .map(|outcome| outcome.ok())
        })?;
        let Some(body) = body else {
            // The smallest region is structurally valid but not lowerable.
            // Trying a containing region would necessarily touch the same
            // blocker, so preserve the existing whole-region fallback.
            break;
        };
        let folded = graph
            .collapse(&region.members, region.head, body)
            .expect("the pure single-entry proof is rechecked without graph mutation");
        graph.set_governing_block(folded, None);
        folded_count += 1;
    }
    Ok(folded_count)
}

fn lower_acyclic_members(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    members: &[NodeId],
    head: NodeId,
) -> Result<Result<Vec<PreHirStmt>, DeclineReason>, MlilPreviewError> {
    let member_set: crate::HashSet<NodeId> = members.iter().copied().collect();
    let mut dense = vec![Vec::new(); graph.node_capacity()];
    for &n in members {
        dense[n] = graph
            .successors(n)
            .iter()
            .copied()
            .filter(|s| member_set.contains(s))
            .collect();
    }
    let Some(order) = topological_order(&dense) else {
        return Ok(Err(DeclineReason::ReachingFailed));
    };
    let branches = match collect_branches_for(host, graph, members.iter().copied())? {
        Ok(branches) => branches,
        Err(reason) => return Ok(Err(reason)),
    };
    if branches.len() > MAX_DECISIONS {
        return Ok(Err(DeclineReason::TooManyDecisions));
    }

    let mut guards = Vec::new();
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
    if members.iter().any(|n| !reaching.contains_key(n)) {
        return Ok(Err(DeclineReason::NodeWithoutCondition));
    }

    let mut bodies = crate::HashMap::default();
    for &n in members {
        let Some(mut stmts) = node_statements(host, graph, n)? else {
            return Ok(Err(DeclineReason::UnplaceableTerminator));
        };
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
    if crate::structuring_quality::guard_formula_size(&body) > MAX_GUARD_FORMULA_SIZE {
        return Ok(Err(DeclineReason::GuardsTooLarge));
    }
    Ok(Ok(inline_single_use_guards(body, &is_guard)))
}

fn find_cyclic_shape(graph: &CollapseGraph) -> Option<Shape> {
    graph.live_nodes().find_map(|n| {
        match_inf_loop(graph, n)
            .or_else(|| match_self_loop(graph, n))
            .or_else(|| match_ring_loop(graph, n))
            .or_else(|| match_while_do(graph, n))
            .or_else(|| match_do_while(graph, n))
            .filter(|s| {
                matches!(
                    s.kind,
                    ShapeKind::InfLoop
                        | ShapeKind::RingLoop
                        | ShapeKind::SelfLoop
                        | ShapeKind::WhileDo
                        | ShapeKind::DoWhile
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
    collect_branches_for(host, graph, graph.live_nodes())
}

fn collect_branches_for(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    nodes: impl IntoIterator<Item = NodeId>,
) -> Result<Result<Vec<Branch>, DeclineReason>, MlilPreviewError> {
    let mut branches = Vec::new();
    for n in nodes {
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
    use fission_midend_prehir::PreHirExpr;

    fn expr(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(PreHirExpr::Var(name.to_string()))
    }

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

    #[test]
    fn a_closed_diamond_is_an_acyclic_sese_region() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![]]);
        assert_eq!(
            find_acyclic_region(&g),
            Some(AcyclicRegion {
                head: 0,
                frontier: AcyclicRegionFrontier::Block(3),
                members: vec![0, 2, 1],
            })
        );
    }

    #[test]
    fn the_smallest_nested_diamond_is_selected_first() {
        // Outer 0 -> {1,4} -> 5; inner 1 -> {2,3} -> 4. Folding the inner
        // decision first prevents its condition from entering the outer path
        // formulas.
        let g =
            CollapseGraph::from_cfg(&[vec![1, 4], vec![2, 3], vec![4], vec![4], vec![5], vec![]]);
        let region = find_acyclic_region(&g).expect("inner region");
        assert_eq!(
            (region.head, region.frontier),
            (1, AcyclicRegionFrontier::Block(4))
        );
        assert_eq!(region.members.len(), 3);
    }

    #[test]
    fn a_side_entry_prevents_acyclic_region_abstraction() {
        // Node 1 looks like a diamond head, but 0 enters one arm directly.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![2, 3], vec![4], vec![4], vec![]]);
        let region = find_acyclic_region(&g).expect("the enclosing region is still valid");
        assert_ne!(region.head, 1, "the side-entered inner region is rejected");
    }

    #[test]
    fn branches_with_distinct_exits_form_a_virtual_exit_region() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![], vec![]]);
        assert_eq!(
            find_acyclic_region(&g),
            Some(AcyclicRegion {
                head: 0,
                frontier: AcyclicRegionFrontier::VirtualExit,
                members: vec![0, 2, 1],
            })
        );
    }

    #[test]
    fn retired_slots_are_not_virtual_exit_terminals() {
        let mut g = CollapseGraph::from_cfg(&[vec![1], vec![2], vec![]]);
        g.collapse(&[0, 1], 0, Vec::new()).expect("fold sequence");
        let compact = compact_live_cfg(&g);
        assert_eq!(compact.nodes, vec![0, 2]);
        assert_eq!(
            compact
                .successors
                .iter()
                .filter(|successors| successors.is_empty())
                .count(),
            1
        );
    }

    #[test]
    fn virtual_goto_chains_keep_every_transfer_and_the_final_body() {
        let mut bodies = crate::HashMap::default();
        bodies.insert(0, vec![expr("source")]);
        bodies.insert(1, vec![expr("middle")]);
        bodies.insert(2, vec![PreHirStmt::Return(None)]);
        let facts = [
            VirtualGotoFact {
                source: 0,
                target: 1,
                label: "L1".to_string(),
            },
            VirtualGotoFact {
                source: 1,
                target: 2,
                label: "L2".to_string(),
            },
        ];

        assert!(apply_virtual_goto_facts(&mut bodies, &facts));
        assert_eq!(
            bodies[&0],
            vec![expr("source"), PreHirStmt::Goto("L1".to_string())]
        );
        assert_eq!(
            bodies[&1],
            vec![
                PreHirStmt::Label("L1".to_string()),
                expr("middle"),
                PreHirStmt::Goto("L2".to_string()),
            ]
        );
        assert_eq!(
            bodies[&2],
            vec![
                PreHirStmt::Label("L2".to_string()),
                PreHirStmt::Return(None),
            ]
        );
    }

    #[test]
    fn shared_virtual_goto_target_gets_one_label() {
        let mut bodies = crate::HashMap::default();
        bodies.insert(0, vec![expr("left")]);
        bodies.insert(1, vec![expr("right")]);
        bodies.insert(2, vec![expr("join")]);
        let facts = [
            VirtualGotoFact {
                source: 0,
                target: 2,
                label: "join".to_string(),
            },
            VirtualGotoFact {
                source: 1,
                target: 2,
                label: "join".to_string(),
            },
        ];

        assert!(apply_virtual_goto_facts(&mut bodies, &facts));
        assert_eq!(
            bodies[&2]
                .iter()
                .filter(|stmt| matches!(stmt, PreHirStmt::Label(label) if label == "join"))
                .count(),
            1
        );
    }
}
