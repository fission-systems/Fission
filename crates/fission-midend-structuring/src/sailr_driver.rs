//! SAILR structuring: Phoenix schemas, compiler-aware concessions.
//!
//! Opt-in (`FISSION_SAILR=1`); see [`sailr_driver_enabled`] for why.
//!
//! The fourth strategy, alongside Ghidra's rules ([`crate::collapse_structure`]),
//! the match-fold schemas ([`crate::collapse_driver`]) and DREAM
//! ([`crate::reaching_driver`]). It follows "Ahoy SAILR! There is No Need to
//! DREAM of C" (Basque et al., USENIX Security 2024) -- the paper, not any
//! implementation of it.
//!
//! Match-fold folds a function only when no edge has to become a jump
//! (`MAX_CONCESSIONS = 0` there), because conceding the *first* edge that
//! comes to hand cost more gotos than it saved. SAILR's argument is that most
//! of those gotos are not inherent: compilers create them, by merging
//! returns and tails and by threading jumps, and a structurer that knows this
//! can (a) undo the merge instead of emitting a jump, and (b) when a jump is
//! unavoidable, choose the one that leaves the rest of the function most
//! structured. This driver does both, on the same shrinking
//! [`CollapseGraph`] match-fold uses:
//!
//! 1. **Fold** a matched region whenever one is expressible (Phoenix).
//! 2. Otherwise **split a merged return**: an exit node that only returns a
//!    side-effect-free value, reached from several places, is given one copy
//!    per predecessor. The compiler merged identical `return`s into one block;
//!    the source had one per path. This adds no jump and removes a join.
//!    Before the first round, every edge into a *return join* -- a merged
//!    `ret` whose value depends on the way in -- is given its own `return` of
//!    the value the builder says that edge carries
//!    (`collapse_driver::split_return_joins`), the same per-edge
//!    answer the linear and SESE paths use.
//! 3. Otherwise **concede one edge**, chosen by the paper's heuristics in
//!    order: H1, the edge whose target has the fewest other incoming edges;
//!    H2, the edge whose removal leaves the most nodes with a real immediate
//!    postdominator (fewer, flatter scopes); H3, an edge into a plain return.
//!    Remaining ties fall back to Phoenix's order -- the target closest to
//!    the head first.
//!
//! Candidate edges are grouped as Phoenix groups them: edges between nodes
//! with no dominance relation first, then edges back to a dominator, then
//! edges from a dominator to a join. Edges into a node with a single
//! predecessor are never candidates, and an edge whose removal would strand
//! a node from the entry is skipped.
//!
//! # What it declines
//!
//! The same all-or-nothing contract as the other drivers: either the graph
//! folds to one node that owns every reachable block, or the driver returns
//! `None` and the caller keeps its existing path. Besides the per-round caps
//! (rounds, concessions, return copies), an aggregate work meter
//! (`MAX_WORK_UNITS`, nodes plus edges visited) bounds the whole attempt; a
//! function that spends it is declined the same way, deterministically. Whether the result is
//! *better* is not this driver's call -- the caller's quality comparison
//! admits it only when it strictly improves on the current best, so a
//! function this driver structures worse is simply not changed.

use crate::HashMap;
use crate::cfg_analysis::{ImmDomTree, ImmPostDomTree};
use crate::collapse_driver::{
    blocks_reachable_from_entry, covers_every_reachable_block,
    fold_accounts_for_every_internal_edge, lower_shape, node_statements, split_return_joins,
};
use crate::collapse_graph::{CollapseGraph, NodeId};
use crate::collapse_shapes::{Shape, find_shape};
use crate::host::StructuringHost;
use crate::reaching_driver::{
    governing_block_after, live_node_owning_entry, materialize_concession,
};
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::util::expr_has_side_effecting_call;
use fission_midend_prehir::{PreHirLValue, PreHirStmt};

/// Ceiling on fold/split/concession rounds, proportional to graph size.
const MAX_ROUNDS_PER_NODE: usize = 8;

/// How many edges may become jumps before the driver gives up.
///
/// Not a quality bound -- the caller's comparison is -- but a cost bound: a
/// function that needs this many jumps is one the other drivers already
/// express with jumps, and folding it again only spends time.
const MAX_CONCESSIONS: usize = 32;

/// How many return copies one function may receive in total.
const MAX_RETURN_COPIES: usize = 16;

/// Largest return node copied: at most this many statements, the `return`
/// included.
const MAX_RETURN_STMTS: usize = 3;

/// Predecessor bound for splitting one return node.
const MAX_RETURN_PREDS: usize = 8;

/// H2 is quadratic in graph size; apply it only to small choices.
const H2_MAX_EDGES: usize = 10;
const H2_MAX_NODES: usize = 50;

/// Aggregate graph work, in nodes plus edges visited, before the driver
/// declines the function. See [`WorkMeter`].
const MAX_WORK_UNITS: u64 = 2_000_000;

/// Whether the SAILR driver is offered. **Off unless `FISSION_SAILR=1`.**
///
/// Measured on the DecBench sample set (250 functions, same binary, only this
/// switch flipped): GED perfect 68 -> 65, total distance 7,406 -> 7,394, seven
/// rows changed. The losses are loops: the shared `WhileDo` lowering rotates
/// the test into a preheader and latch (`for (zf = x == 0; x; ...)`) where the
/// existing path keeps the source's single head (`while (1) { ...; if (!x)
/// break; ... }`). Until that is resolved the driver stays opt-in.
pub fn sailr_driver_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("FISSION_SAILR").as_deref(),
            Ok("1" | "true" | "on" | "yes")
        )
    })
}

/// Structure a whole function with SAILR, committing only a complete fold.
pub fn structure_by_sailr(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    host.lower_isolated(fold_sailr)
}

/// Price a SAILR candidate without committing it; the caller reruns
/// [`structure_by_sailr`] for a winner and never emits this body directly.
pub fn preview_sailr(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    host.lower_observed(fold_sailr)
}

fn fold_sailr(
    host: &mut impl StructuringHost,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    let mut work = WorkMeter::new(MAX_WORK_UNITS);
    let out = fold_sailr_inner(host, &mut work)?;
    if crate::linear_types::structuring_diag_enabled() {
        match &out {
            Ok(_) => eprintln!("[DIAG] SAILR folded: work {}", work.used),
            Err(reason) => eprintln!("[DIAG] SAILR declined: {reason}; work {}", work.used),
        }
    }
    Ok(out.ok())
}

fn fold_sailr_inner(
    host: &mut impl StructuringHost,
    work: &mut WorkMeter,
) -> Result<Result<Vec<PreHirStmt>, String>, MlilPreviewError> {
    let successors: Vec<Vec<usize>> = host.successors().to_vec();
    if successors.is_empty() {
        return Ok(Err("empty cfg".into()));
    }
    let mut graph = CollapseGraph::from_cfg(&successors);
    let reachable = blocks_reachable_from_entry(&successors);
    // Before anything folds or copies a merged return, every edge into one
    // whose value depends on the way in gets its own `return` of that value.
    split_return_joins(host, &mut graph)?;
    let budget = successors.len().saturating_mul(MAX_ROUNDS_PER_NODE);
    let mut concessions = 0usize;
    let mut copies = 0usize;

    for _ in 0..budget {
        // Finding a shape and looking for a return to split each scan the
        // live nodes.
        work.charge(graph.live_count());
        if work.exhausted() {
            return Ok(Err(format!(
                "work budget {} spent, live {}",
                work.limit,
                graph.live_count()
            )));
        }
        if let Some(sole) = graph.sole_live_node() {
            let Some(node) = graph.node(sole) else {
                return Ok(Err("sole node missing".into()));
            };
            if !covers_every_reachable_block(node, &reachable) {
                return Ok(Err("final node does not own every reachable block".into()));
            }
            return Ok(Ok(node.body.clone().unwrap_or_default()));
        }
        let mut failed: Option<Shape> = None;
        if let Some(shape) = find_shape(&graph) {
            let lowered = if fold_accounts_for_every_internal_edge(&graph, &shape) {
                lower_shape(host, &graph, &shape)?
            } else {
                None
            };
            if let Some(body) = lowered {
                // Read before folding: `collapse` retires the members, and a
                // folded `Sequence` still leaves through its last member's
                // branch, which the next shape must be able to read.
                let governing = governing_block_after(&graph, &shape);
                match graph.collapse(&shape.members, shape.entry, body) {
                    Ok(folded) => graph.set_governing_block(folded, governing),
                    Err(e) => return Ok(Err(format!("collapse refused: {e:?}"))),
                }
                continue;
            }
            if crate::linear_types::structuring_diag_enabled() {
                let why = explain_unlowerable(host, &graph, &shape)?;
                eprintln!("[DIAG] SAILR unlowerable {:?}: {why}", shape.kind);
            }
            failed = Some(shape);
        }
        if copies < MAX_RETURN_COPIES {
            let split = split_merged_return(
                host,
                &mut graph,
                failed.as_ref(),
                MAX_RETURN_COPIES - copies,
            )?;
            if split > 0 {
                copies += split;
                continue;
            }
        }
        if concessions >= MAX_CONCESSIONS {
            return Ok(Err(format!(
                "concession cap {MAX_CONCESSIONS}, live {}",
                graph.live_count()
            )));
        }
        let Some((from, to)) = pick_edge(&graph, failed.as_ref(), work, |n| {
            plain_return_node(host, &graph, n)
        }) else {
            return Ok(Err(format!(
                "no concedable edge (live {}, failed shape {:?})",
                graph.live_count(),
                failed.as_ref().map(|s| s.kind)
            )));
        };
        let Some(entry_block) = graph.node(to).map(|n| n.entry_block) else {
            return Ok(Err("conceded target missing".into()));
        };
        if !materialize_concession(host, &mut graph, from, to, format!("sailr_L{entry_block}"))? {
            return Ok(Err(format!(
                "edge {from}->{to} not materializable (outs {:?})",
                graph.successors(from)
            )));
        }
        concessions += 1;
    }
    Ok(Err(format!("round budget, live {}", graph.live_count())))
}

/// Diagnostics only: why a matched shape could not be lowered.
fn explain_unlowerable(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    shape: &Shape,
) -> Result<String, MlilPreviewError> {
    if !fold_accounts_for_every_internal_edge(graph, shape) {
        return Ok("internal edges not accounted".into());
    }
    let mut parts = Vec::new();
    for &m in &shape.members {
        let Some(node) = graph.node(m) else { continue };
        if node.body.is_none() && node_statements(host, graph, m)?.is_none() {
            let term = host.lower_block_terminator(node.entry_block)?;
            parts.push(format!(
                "member {m} (block {}, succ {:?}) unlowerable terminator {}",
                node.entry_block,
                host.successors()[node.entry_block],
                match term {
                    crate::linear_types::LoweredTerminator::Switch { .. } => "Switch",
                    crate::linear_types::LoweredTerminator::Unsupported { .. } => "Unsupported",
                    crate::linear_types::LoweredTerminator::Goto(_) => "Goto",
                    crate::linear_types::LoweredTerminator::Cond { .. } => "Cond",
                    crate::linear_types::LoweredTerminator::Return(_) => "Return",
                    crate::linear_types::LoweredTerminator::Fallthrough(_) => "Fallthrough",
                    #[allow(unreachable_patterns)]
                    _ => "other",
                }
            ));
        }
    }
    if let (Some(&entry), Some(follow)) = (shape.members.first(), shape.follow) {
        let gov = graph.node(entry).and_then(|n| n.governing_block);
        if gov.is_none() {
            parts.push(format!("entry {entry} has no governing block"));
        } else if let Some(&clause) = shape.members.get(1) {
            if crate::collapse_driver::condition_towards(host, graph, entry, clause, follow)?
                .is_none()
            {
                parts.push(format!(
                    "condition {entry}->{clause} vs {follow} not recoverable (gov {gov:?})"
                ));
            }
        }
    }
    if parts.is_empty() {
        parts.push("lower_shape declined for its own reason".into());
    }
    Ok(parts.join("; "))
}

/// The statements of `id` when it is an exit that only returns: at most
/// [`MAX_RETURN_STMTS`] statements, ending in `return`, with no call that
/// could have an effect and no control flow. Copying such a node changes
/// nothing a path does -- each path still runs exactly those statements once.
///
/// Why that holds, including for values that differ per predecessor: a
/// node's statements are a function of the node alone (a block is lowered
/// once and cached by block index), and they are what every path through it
/// already ran. A value that depends on the incoming edge reaches the block
/// through copies placed at the end of each predecessor, or through a merge
/// binding the block itself evaluates; a copy keeps both, in the same order
/// relative to every load and store. The one value the block's own lowering
/// does *not* get right for every way in is a return join's (a shared `ret`
/// block whose return register each predecessor sets differently); those
/// edges were already given their own `return` by
/// [`split_return_joins`] before anything folded or was copied. A node with
/// no successors has nothing after it to reach twice. The size and effect
/// bounds are a code-growth policy, not what makes the copy sound. A `goto`
/// target is never copied: its body carries the label, and only variable
/// assignments and the `return` are admitted, so no label is ever defined
/// twice.
fn plain_return_body(
    host: &mut impl StructuringHost,
    graph: &CollapseGraph,
    id: NodeId,
) -> Result<Option<Vec<PreHirStmt>>, MlilPreviewError> {
    if !graph.successors(id).is_empty() {
        return Ok(None);
    }
    let Some(body) = node_statements(host, graph, id)? else {
        return Ok(None);
    };
    Ok(is_plain_return(&body).then_some(body))
}

fn plain_return_node(host: &mut impl StructuringHost, graph: &CollapseGraph, id: NodeId) -> bool {
    matches!(plain_return_body(host, graph, id), Ok(Some(_)))
}

/// Whether `body` is a short, effect-free run ending in `return`.
pub(crate) fn is_plain_return(body: &[PreHirStmt]) -> bool {
    let Some((last, rest)) = body.split_last() else {
        return false;
    };
    if body.len() > MAX_RETURN_STMTS {
        return false;
    }
    let PreHirStmt::Return(value) = last else {
        return false;
    };
    if value.as_ref().is_some_and(expr_has_side_effecting_call) {
        return false;
    }
    rest.iter().all(|stmt| match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            matches!(lhs, PreHirLValue::Var(_)) && !expr_has_side_effecting_call(rhs)
        }
        _ => false,
    })
}

/// Give every predecessor of one merged return node its own copy.
///
/// Picks a node next to the region that just failed to fold when there is
/// one, otherwise the merged return with the lowest entry block. Returns how
/// many copies were made (0: nothing to split).
fn split_merged_return(
    host: &mut impl StructuringHost,
    graph: &mut CollapseGraph,
    failed: Option<&Shape>,
    allowance: usize,
) -> Result<usize, MlilPreviewError> {
    let mut candidates: Vec<NodeId> = Vec::new();
    let mut listed = vec![false; graph.node_capacity()];
    if let Some(shape) = failed {
        for &m in &shape.members {
            for &s in graph.successors(m) {
                if !listed[s] {
                    listed[s] = true;
                    candidates.push(s);
                }
            }
        }
    }
    let mut rest: Vec<NodeId> = graph.live_nodes().filter(|&n| !listed[n]).collect();
    rest.sort_by_key(|&n| graph.node(n).map_or(usize::MAX, |x| x.entry_block));
    candidates.extend(rest);

    for id in candidates {
        let preds: Vec<NodeId> = graph.predecessors(id).to_vec();
        if preds.len() < 2 || preds.len() > MAX_RETURN_PREDS || preds.len() - 1 > allowance {
            continue;
        }
        let Some(body) = plain_return_body(host, graph, id)? else {
            continue;
        };
        // The original keeps the first predecessor; every other gets a copy.
        let mut made = 0;
        for &p in &preds[1..] {
            if graph.split_exit_for(id, p, body.clone()).is_some() {
                made += 1;
            }
        }
        if made > 0 {
            graph.set_body(id, body);
            graph.set_governing_block(id, None);
            return Ok(made);
        }
    }
    Ok(0)
}

/// The edge to concede, by Phoenix's candidate classes and SAILR's H1-H3.
///
/// Dominance is answered from one immediate-dominator tree (Cooper) with
/// pre/post-order intervals, and "removing this edge strands no node" is
/// answered from the same tree in O(in-degree) per candidate instead of a
/// graph walk per candidate; see [`Dominance::survives_without`] for why the
/// two questions are the same. The choice is identical to the walk-based one
/// (`differential_tests::pick_edge_matches_the_reference_implementation`).
fn pick_edge(
    graph: &CollapseGraph,
    failed: Option<&Shape>,
    work: &mut WorkMeter,
    mut is_plain_return: impl FnMut(NodeId) -> bool,
) -> Option<(NodeId, NodeId)> {
    let head = live_node_owning_entry(graph)?;
    let live: Vec<NodeId> = graph.live_nodes().collect();
    let dense = Dense::new(graph, head, &live);
    work.charge(dense.size());
    // Removing an edge only ever loses reachability: when some live node is
    // already cut off from the head, no removal leaves every node reachable,
    // so no edge can be conceded.
    if !dense.all_reachable() {
        return None;
    }
    let dom = Dominance::new(&dense);

    // Phoenix's three classes; within each, SAILR's order.
    let mut unrelated = Vec::new();
    let mut to_dominator = Vec::new();
    let mut from_dominator = Vec::new();
    for &u in &live {
        if graph.successors(u).len() > 2 {
            continue;
        }
        for &v in graph.successors(u) {
            if u == v || (v != head && graph.predecessors(v).len() < 2) {
                continue;
            }
            let (du, dv) = (dense.index[&u], dense.index[&v]);
            let u_dom_v = dom.dominates(du, dv);
            let v_dom_u = dom.dominates(dv, du);
            let class = if !u_dom_v && !v_dom_u {
                &mut unrelated
            } else if !u_dom_v {
                &mut to_dominator
            } else {
                &mut from_dominator
            };
            class.push((u, v));
        }
    }
    let near_failure = |e: &(NodeId, NodeId)| {
        failed.is_some_and(|s| s.members.contains(&e.0) || s.members.contains(&e.1))
    };
    for class in [unrelated, to_dominator, from_dominator] {
        if class.is_empty() {
            continue;
        }
        // Edges touching the region that just failed are the ones whose
        // concession lets it fold next round.
        let focused: Vec<_> = class.iter().copied().filter(near_failure).collect();
        let edges = if focused.is_empty() { class } else { focused };
        for edge in order_edges(graph, &dense, edges, work, &mut is_plain_return) {
            let (du, dv) = (dense.index[&edge.0], dense.index[&edge.1]);
            work.charge(dense.pred[dv].len());
            if dom.survives_without(&dense, du, dv) {
                return Some(edge);
            }
        }
    }
    None
}

/// SAILR's ordering: H1, then H2 on small graphs, then H3, then Phoenix's
/// default. Every step only narrows the tie of the previous one, and the
/// order is total, so the result is deterministic.
fn order_edges(
    graph: &CollapseGraph,
    dense: &Dense,
    mut edges: Vec<(NodeId, NodeId)>,
    work: &mut WorkMeter,
    is_plain_return: &mut impl FnMut(NodeId) -> bool,
) -> Vec<(NodeId, NodeId)> {
    let block = |n: NodeId| graph.node(n).map_or(usize::MAX, |x| x.entry_block);
    // Phoenix's default: the target closest to the head first, then the
    // target with the fewest incoming candidate edges, then by address.
    let mut cand_in: HashMap<NodeId, usize> = HashMap::default();
    for &(_, v) in &edges {
        *cand_in.entry(v).or_default() += 1;
    }
    edges.sort_by_key(|&(u, v)| (dense.rpo[&v], cand_in[&v], block(u), block(v)));

    // H1: fewest sibling edges into the target.
    let siblings = |v: NodeId| graph.predecessors(v).len().saturating_sub(1);
    let h1 = edges
        .iter()
        .map(|&(_, v)| siblings(v))
        .filter(|&c| c > 0)
        .min();
    let mut best: Vec<(NodeId, NodeId)> = match h1 {
        Some(min) => edges
            .iter()
            .copied()
            .filter(|&(_, v)| siblings(v) == min)
            .collect(),
        None => edges.clone(),
    };

    // H2: removal that leaves the most real immediate postdominators.
    if best.len() > 1 && edges.len() <= H2_MAX_EDGES && dense.succ.len() <= H2_MAX_NODES {
        // One scratch copy of the adjacency for every candidate, each edge
        // taken out and put back in place, rather than a copy per candidate.
        let mut scratch = (dense.succ.clone(), dense.pred.clone());
        let scored: Vec<((NodeId, NodeId), usize)> = best
            .iter()
            .map(|&(u, v)| {
                work.charge(dense.size());
                (
                    (u, v),
                    postdominated_without(&mut scratch, dense.index[&u], dense.index[&v]),
                )
            })
            .collect();
        let max = scored.iter().map(|(_, c)| *c).max().unwrap_or(0);
        best = scored
            .into_iter()
            .filter(|(_, c)| *c == max)
            .map(|(e, _)| e)
            .collect();
    }

    // H3: an edge into a plain return can be undone later by a copy.
    if best.len() > 1 {
        let to_return: Vec<_> = best
            .iter()
            .copied()
            .filter(|&(_, v)| is_plain_return(v))
            .collect();
        if !to_return.is_empty() {
            best = to_return;
        }
    }

    // Best first, then the rest in default order as fallbacks.
    let chosen: crate::HashSet<(NodeId, NodeId)> = best.iter().copied().collect();
    let mut out = best;
    out.extend(edges.into_iter().filter(|e| !chosen.contains(e)));
    out
}

/// The live graph renumbered densely with the head at 0, for the dominance
/// analyses, plus a reverse-postorder rank of every node.
struct Dense {
    index: HashMap<NodeId, usize>,
    succ: Vec<Vec<usize>>,
    pred: Vec<Vec<usize>>,
    rpo: HashMap<NodeId, usize>,
    /// How many nodes the walk from the head reached.
    reached: usize,
}

impl Dense {
    fn new(graph: &CollapseGraph, head: NodeId, live: &[NodeId]) -> Self {
        let mut order = vec![head];
        order.extend(live.iter().copied().filter(|&n| n != head));
        let index: HashMap<NodeId, usize> =
            order.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let mut succ = vec![Vec::new(); order.len()];
        let mut pred = vec![Vec::new(); order.len()];
        for (i, &n) in order.iter().enumerate() {
            for s in graph.successors(n) {
                if let Some(&j) = index.get(s) {
                    succ[i].push(j);
                    pred[j].push(i);
                }
            }
        }
        // Reverse postorder from the head; unreachable nodes rank last, by id.
        let mut seen = vec![false; order.len()];
        let mut post = Vec::with_capacity(order.len());
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        seen[0] = true;
        while let Some((n, i)) = stack.pop() {
            if i < succ[n].len() {
                stack.push((n, i + 1));
                let s = succ[n][i];
                if !seen[s] {
                    seen[s] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(n);
            }
        }
        let mut rpo: HashMap<NodeId, usize> = HashMap::default();
        for (rank, &n) in post.iter().rev().enumerate() {
            rpo.insert(order[n], rank);
        }
        let reached = post.len();
        let mut next = post.len();
        for &n in &order {
            if !rpo.contains_key(&n) {
                rpo.insert(n, next);
                next += 1;
            }
        }
        Self {
            index,
            succ,
            pred,
            rpo,
            reached,
        }
    }

    /// Nodes plus edges: the unit every pass over this graph is charged in.
    fn size(&self) -> usize {
        self.succ.len() + self.succ.iter().map(Vec::len).sum::<usize>()
    }

    /// Whether every node is reachable from the head. Unreachable nodes are
    /// exactly the ones ranked after the reverse postorder walk.
    fn all_reachable(&self) -> bool {
        self.reached == self.succ.len()
    }
}

/// How many nodes keep a real immediate postdominator once `u -> v` is gone.
///
/// `scratch` holds a copy of the dense adjacency; the edge is removed for the
/// computation and restored at its original positions afterwards, so the
/// copy can serve every candidate.
fn postdominated_without(
    scratch: &mut (Vec<Vec<usize>>, Vec<Vec<usize>>),
    u: usize,
    v: usize,
) -> usize {
    let (succ, pred) = scratch;
    let succ_at: Vec<usize> = positions(&succ[u], v);
    let pred_at: Vec<usize> = positions(&pred[v], u);
    succ[u].retain(|&x| x != v);
    pred[v].retain(|&x| x != u);
    let ipdom = ImmPostDomTree::compute(succ, pred);
    let count = (0..succ.len())
        .filter(|&n| ipdom.immediate_postdominator(n).is_some())
        .count();
    for at in succ_at {
        succ[u].insert(at, v);
    }
    for at in pred_at {
        pred[v].insert(at, u);
    }
    count
}

fn positions(list: &[usize], value: usize) -> Vec<usize> {
    list.iter()
        .enumerate()
        .filter_map(|(i, &x)| (x == value).then_some(i))
        .collect()
}

/// Dominance on a [`Dense`] graph every node of which is reachable from the
/// head: the immediate-dominator tree, numbered by one depth-first walk so
/// that "`a` dominates `b`" is an interval test.
struct Dominance {
    pre: Vec<usize>,
    post: Vec<usize>,
}

impl Dominance {
    fn new(dense: &Dense) -> Self {
        let n = dense.succ.len();
        let idom = ImmDomTree::compute(&dense.succ, &dense.pred);
        let mut children = vec![Vec::new(); n];
        for v in 1..n {
            if let Some(p) = idom.immediate_dominator(v) {
                children[p].push(v);
            }
        }
        let mut pre = vec![usize::MAX; n];
        let mut post = vec![usize::MAX; n];
        let (mut pre_clock, mut post_clock) = (0usize, 0usize);
        let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
        pre[0] = pre_clock;
        pre_clock += 1;
        while let Some((node, next)) = stack.pop() {
            if let Some(&child) = children[node].get(next) {
                stack.push((node, next + 1));
                pre[child] = pre_clock;
                pre_clock += 1;
                stack.push((child, 0));
            } else {
                post[node] = post_clock;
                post_clock += 1;
            }
        }
        Self { pre, post }
    }

    /// Reflexive: every node dominates itself.
    fn dominates(&self, a: usize, b: usize) -> bool {
        self.pre[a] <= self.pre[b] && self.post[b] <= self.post[a]
    }

    /// Whether every node stays reachable from the head once `u -> v` is
    /// removed, given that every node is reachable now.
    ///
    /// Removing an edge strands something only if it strands `v`: any path
    /// that used the edge can be re-routed through whatever still reaches `v`.
    /// `v` stays reachable iff it is the head, or another predecessor `p`
    /// reaches it without passing through `v` first -- that is, `v` does not
    /// dominate `p`. If `v` dominates every other predecessor, each of them is
    /// reached only through `v`, so nothing but `u -> v` can bring control to
    /// `v`.
    fn survives_without(&self, dense: &Dense, u: usize, v: usize) -> bool {
        v == 0
            || dense.pred[v]
                .iter()
                .any(|&p| p != u && !self.dominates(v, p))
    }
}

/// An aggregate bound on the driver's graph work, in nodes-plus-edges
/// visited. Per-round caps bound each step; this bounds their sum, so a
/// function that keeps the driver busy is declined -- deterministically,
/// since the count does not depend on time -- and the caller keeps its
/// existing structuring.
#[derive(Debug, Clone, Copy)]
struct WorkMeter {
    used: u64,
    limit: u64,
}

impl WorkMeter {
    fn new(limit: u64) -> Self {
        Self { used: 0, limit }
    }

    fn charge(&mut self, units: usize) {
        self.used = self.used.saturating_add(units as u64);
    }

    fn exhausted(&self) -> bool {
        self.used > self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::PreHirExpr;

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    #[test]
    fn a_short_effect_free_return_is_plain() {
        let body = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("r".to_string()),
                rhs: var("x"),
            },
            PreHirStmt::Return(Some(var("r"))),
        ];
        assert!(is_plain_return(&body));
        assert!(is_plain_return(&[PreHirStmt::Return(None)]));
    }

    #[test]
    fn a_return_with_more_work_or_no_return_is_not_plain() {
        assert!(!is_plain_return(&[]));
        assert!(!is_plain_return(&[PreHirStmt::Expr(var("x"))]));
        let long = vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("a".to_string()),
                rhs: var("x"),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("b".to_string()),
                rhs: var("y"),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("c".to_string()),
                rhs: var("z"),
            },
            PreHirStmt::Return(Some(var("c"))),
        ];
        assert!(!is_plain_return(&long), "longer than MAX_RETURN_STMTS");
        let store = vec![PreHirStmt::Expr(var("side")), PreHirStmt::Return(None)];
        assert!(
            !is_plain_return(&store),
            "an expression statement is not an assignment"
        );
    }

    /// 0 -> {1, 2}; 1 -> 3; 2 -> 3; 3 exit. The diamond's join is the only
    /// node with two predecessors, so splitting it leaves two exits.
    #[test]
    fn splitting_an_exit_gives_each_predecessor_its_own_copy() {
        let mut g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3], vec![3], vec![]]);
        let body = vec![PreHirStmt::Return(None)];
        let copy = g.split_exit_for(3, 2, body.clone()).expect("split");
        assert_eq!(g.successors(2), &[copy]);
        assert_eq!(g.predecessors(3), &[1]);
        assert_eq!(g.predecessors(copy), &[2]);
        assert_eq!(g.node(copy).map(|n| n.entry_block), Some(3));
        assert!(
            g.split_exit_for(1, 0, body).is_none(),
            "a node with successors is not an exit"
        );
    }

    /// 0 -> {1, 2}; 1 -> {3, 4}; 2 -> 4; 3 -> 4. Candidate edges into the join
    /// 4 all have siblings; H1 cannot split them, so the order is total and
    /// stable across calls.
    #[test]
    fn edge_order_is_deterministic() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3, 4], vec![4], vec![4], vec![]]);
        let a = pick_edge(&g, None, &mut WorkMeter::new(u64::MAX), |_| false);
        let b = pick_edge(&g, None, &mut WorkMeter::new(u64::MAX), |_| false);
        assert!(a.is_some());
        assert_eq!(a, b);
        let (_, to) = a.unwrap();
        assert_eq!(to, 4, "only the join has more than one predecessor");
    }

    /// H3: between otherwise equal edges, the one into a plain return goes
    /// first. 0 -> {1, 2}; 1 -> {3, 4}; 2 -> {3, 4}; 3, 4 exits. Edges into 3
    /// and 4 tie on H1 (one sibling each) and on class.
    #[test]
    fn h3_prefers_an_edge_into_a_plain_return() {
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![3, 4], vec![3, 4], vec![], vec![]]);
        let picked = pick_edge(&g, None, &mut WorkMeter::new(u64::MAX), |n| n == 4);
        assert_eq!(picked.map(|(_, v)| v), Some(4));
        let picked = pick_edge(&g, None, &mut WorkMeter::new(u64::MAX), |n| n == 3);
        assert_eq!(picked.map(|(_, v)| v), Some(3));
    }
}

#[cfg(test)]
#[path = "sailr_driver_differential_tests.rs"]
mod differential_tests;
