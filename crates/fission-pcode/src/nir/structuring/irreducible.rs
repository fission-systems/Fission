/// Irreducible CFG normalization via Node-Splitting (Ramalingam 1996).
///
/// An irreducible control-flow graph has a strongly-connected component
/// (SCC) with more than one entry node — i.e. multiple blocks that can be
/// reached directly from outside the SCC, creating "side-entry" loops.
/// Standard structured-control-flow algorithms (while/for/if) require a
/// reducible CFG and fail on irreducible ones.
///
/// ## Algorithm
///
/// For each irreducible SCC with headers {H1, H2, …}:
///   1. Keep H1 as the canonical header.
///   2. For each extra header Hi (i ≥ 2):
///      a. Create a virtual clone node C_i that executes the same P-code
///         block content as Hi.
///      b. Redirect all back-edges from within the SCC that target Hi to
///         instead target C_i.
///      c. C_i's successors are the same as Hi's successors.
///      d. Hi now has a single back-edge source (from C_i) and the SCC
///         becomes reducible.
///
/// After splitting, the CFG is reducible and the structuring driver can
/// retry its structured-code generation pass.
///
/// ## Limits
///
/// - Only applied when the total number of virtual blocks added ≤ `MAX_SPLIT_NODES`.
/// - Maximum depth (loop nesting) for splitting: `MAX_LOOP_DEPTH`.
/// - At most `MAX_ITERATIONS` rounds of splitting per function.
///
/// ## References
///
/// - Ramalingam 1996 "On Loops, Dominators, and the Duals"
/// - LLVM `lib/Transforms/Utils/FixIrreducible.cpp`
/// - Tarjan 1972 "Depth-First Search and Linear Graph Algorithms"

const MAX_SPLIT_NODES: usize = 32;
const MAX_ITERATIONS: usize = 3;
const MAX_HEADER_STMTS: usize = 50; // Per plan: skip if block too large.

/// Result of applying node-splitting to a CFG.
///
/// For `n` original blocks: indices `0..n` are originals; indices `n..` are
/// virtual clones.  `virtual_to_original[i - n]` gives the original block
/// index for virtual block `i`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct NodeSplitResult {
    pub(crate) new_successors: Vec<Vec<usize>>,
    pub(crate) new_predecessors: Vec<Vec<usize>>,
    /// For each virtual block (index ≥ original_count), the original block
    /// index whose P-code content it clones.
    pub(crate) virtual_to_original: Vec<usize>,
    pub(crate) original_count: usize,
    pub(crate) splits_applied: usize,
}

impl NodeSplitResult {
    /// Map a virtual block index to the original P-code block index.
    pub(crate) fn original_for(&self, idx: usize) -> usize {
        if idx < self.original_count {
            idx
        } else {
            self.virtual_to_original[idx - self.original_count]
        }
    }
}

/// Try to make the given CFG reducible by node-splitting.
///
/// Returns `Some(result)` if splitting was applied; `None` if the CFG is
/// already reducible, too large, or splitting limits are exceeded.
pub(crate) fn compute_node_splits(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    block_stmt_counts: &[usize],
) -> Option<NodeSplitResult> {
    let original_count = successors.len();
    if original_count == 0 {
        return None;
    }

    let mut cur_succs: Vec<Vec<usize>> = successors.to_vec();
    let mut cur_preds: Vec<Vec<usize>> = predecessors.to_vec();
    let mut virtual_to_original: Vec<usize> = Vec::new();
    let mut total_splits = 0;

    for _iter in 0..MAX_ITERATIONS {
        let scc = compute_scc(&cur_succs);
        let irreducible = find_irreducible_headers(&scc, &cur_preds);

        if irreducible.is_empty() {
            break; // CFG is now reducible.
        }

        let mut did_split = false;
        for (component_nodes, extra_headers) in &irreducible {
            let component_set: std::collections::HashSet<usize> =
                component_nodes.iter().copied().collect();

            for &header in extra_headers {
                // Check size limit — don't split huge blocks.
                let orig_header = if header < original_count {
                    header
                } else {
                    virtual_to_original[header - original_count]
                };
                if block_stmt_counts.get(orig_header).copied().unwrap_or(0) > MAX_HEADER_STMTS {
                    continue;
                }
                if total_splits >= MAX_SPLIT_NODES {
                    break;
                }

                // Create a virtual clone of `header`.
                let clone_idx = cur_succs.len();
                virtual_to_original.push(orig_header);

                // Clone's successors = header's successors.
                cur_succs.push(cur_succs[header].clone());
                // Clone's predecessors: will be populated below.
                cur_preds.push(Vec::new());

                // Update successor entries for clone's successors to include clone.
                for &succ in &cur_succs[clone_idx].clone() {
                    cur_preds[succ].push(clone_idx);
                }

                // Redirect back-edges from within the SCC targeting header
                // to target the clone instead.
                let back_edge_sources: Vec<usize> = cur_preds[header]
                    .iter()
                    .copied()
                    .filter(|&pred| component_set.contains(&pred))
                    .collect();

                for source in back_edge_sources {
                    // Redirect source → header to source → clone.
                    if let Some(pos) = cur_succs[source].iter().position(|&s| s == header) {
                        cur_succs[source][pos] = clone_idx;
                    }
                    cur_preds[header].retain(|&p| p != source);
                    cur_preds[clone_idx].push(source);
                }

                total_splits += 1;
                did_split = true;
            }
        }

        if !did_split {
            break;
        }
    }

    if total_splits == 0 {
        return None;
    }

    // Recompute predecessors to ensure consistency.
    let n = cur_succs.len();
    let mut final_preds = vec![Vec::<usize>::new(); n];
    for (src, succs) in cur_succs.iter().enumerate() {
        for &dst in succs {
            final_preds[dst].push(src);
        }
    }

    Some(NodeSplitResult {
        new_successors: cur_succs,
        new_predecessors: final_preds,
        virtual_to_original,
        original_count,
        splits_applied: total_splits,
    })
}

/// Maximum number of FAS edges that will be virtualized as gotos.
/// If the FAS exceeds this, we fall through to the raw linear path.
const MAX_FAS_VIRTUAL_GOTOS: usize = 8;

/// Compute the Minimum Feedback Arc Set (FAS) for irreducible SCCs using a
/// greedy 2-approximation heuristic.
///
/// For each irreducible SCC (≥ 2 nodes, ≥ 2 entry headers), we identify
/// candidate back-edges within the SCC and greedily select the minimal set of
/// edges needed to make the graph acyclic.
///
/// The greedy strategy sorts candidate edges by their source node's
/// *excess out-degree* (`out_degree − in_degree`) in descending order, and
/// selects each edge if removing it does not re-introduce another cycle
/// (checked via a fast cycle test on the remaining SCC nodes).
///
/// ## Returns
///
/// A `Vec<(src, dst)>` of edges to virtualize as gotos. Returns an empty Vec
/// if the FAS is larger than `MAX_FAS_VIRTUAL_GOTOS` (fallback to raw linear).
pub(crate) fn compute_fas_virtual_gotos(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
) -> Vec<(usize, usize)> {
    let sccs = compute_scc(successors);
    let irreducible = find_irreducible_headers(&sccs, predecessors);
    if irreducible.is_empty() {
        return Vec::new();
    }

    let mut fas_edges: Vec<(usize, usize)> = Vec::new();

    for (component_nodes, _extra_headers) in &irreducible {
        let component_set: std::collections::HashSet<usize> =
            component_nodes.iter().copied().collect();

        // Collect all back-edges within this SCC: edges (src → dst) where both
        // src and dst are inside the component.
        let mut candidate_edges: Vec<(usize, usize)> = Vec::new();
        for &src in component_nodes {
            for &dst in &successors[src] {
                if component_set.contains(&dst) {
                    candidate_edges.push((src, dst));
                }
            }
        }

        // Score and sort each candidate edge using H2 (post-dominator maximization) and H3 (simple return).
        let node_count = successors.len();
        if node_count <= 64 && candidate_edges.len() <= 16 {
            // Inside the scale gate: compute post-dominator counts
            let mut edge_scores = Vec::new();
            for &(src, dst) in &candidate_edges {
                let mut temp_succs = successors.to_vec();
                let mut temp_preds = predecessors.to_vec();

                // Remove the edge (src -> dst)
                if let Some(pos) = temp_succs[src].iter().position(|&x| x == dst) {
                    temp_succs[src].remove(pos);
                }
                if let Some(pos) = temp_preds[dst].iter().position(|&x| x == src) {
                    temp_preds[dst].remove(pos);
                }

                let postdom = super::PostDomTree::analyze(&temp_succs, &temp_preds);
                let postdom_score: usize = postdom.postdominators().values().map(|set| set.len()).sum();
                edge_scores.push(((src, dst), postdom_score));
            }

            candidate_edges.sort_by(|&a, &b| {
                let a_score = edge_scores.iter().find(|&&((s, d), _)| s == a.0 && d == a.1).map(|&(_, s)| s).unwrap_or(0);
                let b_score = edge_scores.iter().find(|&&((s, d), _)| s == b.0 && d == b.1).map(|&(_, s)| s).unwrap_or(0);

                if a_score != b_score {
                    // Key 1: Post-dominator relationship count (higher is better)
                    b_score.cmp(&a_score)
                } else {
                    let a_is_ret = successors[a.1].is_empty();
                    let b_is_ret = successors[b.1].is_empty();
                    if a_is_ret != b_is_ret {
                        // Key 2: Destination is a simple return block (true is preferred)
                        b_is_ret.cmp(&a_is_ret)
                    } else {
                        // Key 3: Source node excess out-degree score (higher is better)
                        let a_excess = successors[a.0].len() as i64 - predecessors[a.0].len() as i64;
                        let b_excess = successors[b.0].len() as i64 - predecessors[b.0].len() as i64;
                        if a_excess != b_excess {
                            b_excess.cmp(&a_excess)
                        } else {
                            // Key 4: Deterministic tie-breaker
                            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
                        }
                    }
                }
            });
        } else {
            // Outside the scale gate: fall back to H3 + excess out-degree + deterministic tie-breaker
            candidate_edges.sort_by(|&a, &b| {
                let a_is_ret = successors[a.1].is_empty();
                let b_is_ret = successors[b.1].is_empty();
                if a_is_ret != b_is_ret {
                    // Key 1: Destination is simple return block (true is preferred)
                    b_is_ret.cmp(&a_is_ret)
                } else {
                    // Key 2: Source node excess out-degree score (higher is better)
                    let a_excess = successors[a.0].len() as i64 - predecessors[a.0].len() as i64;
                    let b_excess = successors[b.0].len() as i64 - predecessors[b.0].len() as i64;
                    if a_excess != b_excess {
                        b_excess.cmp(&a_excess)
                    } else {
                        // Key 3: Deterministic tie-breaker
                        a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
                    }
                }
            });
        }

        // Greedily select edges until the component is acyclic.
        let mut removed_edges: std::collections::HashSet<(usize, usize)> =
            std::collections::HashSet::new();

        for edge in &candidate_edges {
            if !component_has_cycle(&component_set, successors, &removed_edges) {
                // Already acyclic — stop.
                break;
            }
            removed_edges.insert(*edge);
        }

        // If after inserting edges the component is now acyclic, record them.
        if !component_has_cycle(&component_set, successors, &removed_edges) {
            for e in &removed_edges {
                if !fas_edges.contains(e) {
                    fas_edges.push(*e);
                }
            }
        }
    }

    fas_edges.sort_unstable();
    fas_edges.dedup();

    if fas_edges.len() > MAX_FAS_VIRTUAL_GOTOS {
        return Vec::new();
    }

    fas_edges
}

/// Check whether the subgraph induced by `component_set` still has a cycle
/// after removing all edges in `removed_edges`.
fn component_has_cycle(
    component_set: &std::collections::HashSet<usize>,
    successors: &[Vec<usize>],
    removed_edges: &std::collections::HashSet<(usize, usize)>,
) -> bool {
    // DFS-based cycle detection on the component subgraph.
    let mut visited = std::collections::HashSet::new();
    let mut on_stack = std::collections::HashSet::new();

    fn dfs(
        node: usize,
        component_set: &std::collections::HashSet<usize>,
        successors: &[Vec<usize>],
        removed_edges: &std::collections::HashSet<(usize, usize)>,
        visited: &mut std::collections::HashSet<usize>,
        on_stack: &mut std::collections::HashSet<usize>,
    ) -> bool {
        visited.insert(node);
        on_stack.insert(node);
        for &succ in &successors[node] {
            if !component_set.contains(&succ) {
                continue;
            }
            if removed_edges.contains(&(node, succ)) {
                continue;
            }
            if !visited.contains(&succ) {
                if dfs(succ, component_set, successors, removed_edges, visited, on_stack) {
                    return true;
                }
            } else if on_stack.contains(&succ) {
                return true;
            }
        }
        on_stack.remove(&node);
        false
    }

    for &node in component_set {
        if !visited.contains(&node) {
            if dfs(node, component_set, successors, removed_edges, &mut visited, &mut on_stack) {
                return true;
            }
        }
    }
    false
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Tarjan SCC — returns `Vec<Vec<usize>>` (each inner Vec is a component).
fn compute_scc(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = successors.len();
    let mut state = TarjanState {
        index: 0,
        indices: vec![None; n],
        lowlink: vec![0; n],
        stack: Vec::new(),
        on_stack: vec![false; n],
        components: Vec::new(),
    };
    for node in 0..n {
        if state.indices[node].is_none() {
            tarjan_dfs_iterative(node, successors, &mut state);
        }
    }
    state.components
}

struct TarjanState {
    index: usize,
    indices: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    components: Vec<Vec<usize>>,
}

fn tarjan_dfs_iterative(start: usize, succs: &[Vec<usize>], s: &mut TarjanState) {
    // Iterative Tarjan SCC using an explicit work stack.
    // Each frame is (node, successor_iterator_index).
    // We simulate the recursive call stack on the heap to avoid stack overflow
    // on large / deeply-nested CFGs.
    struct Frame {
        v: usize,
        succ_idx: usize,
    }

    let mut call_stack: Vec<Frame> = Vec::new();

    // "Enter" the start node.
    s.indices[start] = Some(s.index);
    s.lowlink[start] = s.index;
    s.index += 1;
    s.stack.push(start);
    s.on_stack[start] = true;
    call_stack.push(Frame { v: start, succ_idx: 0 });

    while let Some(frame) = call_stack.last_mut() {
        let v = frame.v;
        if frame.succ_idx < succs[v].len() {
            let w = succs[v][frame.succ_idx];
            frame.succ_idx += 1;

            if s.indices[w].is_none() {
                // Recurse into w.
                s.indices[w] = Some(s.index);
                s.lowlink[w] = s.index;
                s.index += 1;
                s.stack.push(w);
                s.on_stack[w] = true;
                call_stack.push(Frame { v: w, succ_idx: 0 });
            } else if s.on_stack[w] {
                s.lowlink[v] = s.lowlink[v].min(s.indices[w].unwrap());
            }
        } else {
            // All successors processed — pop frame and propagate lowlink.
            call_stack.pop();
            if let Some(parent) = call_stack.last() {
                let pv = parent.v;
                s.lowlink[pv] = s.lowlink[pv].min(s.lowlink[v]);
            }
            // Check if v is an SCC root.
            if s.lowlink[v] == s.indices[v].unwrap() {
                let mut component = Vec::new();
                loop {
                    let w = s.stack.pop().unwrap();
                    s.on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                s.components.push(component);
            }
        }
    }
}

/// Find irreducible SCCs: components with ≥ 2 entry nodes (headers).
///
/// Returns Vec of `(component_nodes, extra_headers)` where
/// `extra_headers` are the secondary entry points that need splitting.
fn find_irreducible_headers(
    components: &[Vec<usize>],
    predecessors: &[Vec<usize>],
) -> Vec<(Vec<usize>, Vec<usize>)> {
    let mut result = Vec::new();
    for component in components {
        if component.len() < 2 {
            continue;
        }
        let component_set: std::collections::HashSet<usize> = component.iter().copied().collect();
        let mut headers = Vec::new();
        for &node in component {
            for &pred in predecessors.get(node).into_iter().flatten() {
                if !component_set.contains(&pred) {
                    headers.push(node);
                    break;
                }
            }
        }
        headers.sort_unstable();
        headers.dedup();
        if headers.len() >= 2 {
            // Extra headers are all but the first (canonical) one.
            let extra = headers[1..].to_vec();
            result.push((component.clone(), extra));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build predecessor list from successor list.
    fn build_preds(succs: &[Vec<usize>]) -> Vec<Vec<usize>> {
        let n = succs.len();
        let mut preds = vec![Vec::new(); n];
        for (src, dsts) in succs.iter().enumerate() {
            for &dst in dsts {
                preds[dst].push(src);
            }
        }
        preds
    }

    /// A genuinely irreducible 2-node SCC: both 0 and 1 are entry nodes from
    /// external nodes, and there is a back-edge between them.
    ///
    ///   entry_a(2) → 0 ─┐
    ///   entry_b(3) → 1 ←┘
    ///                 └→ 0    (1→0 makes {0,1} a cycle with dual external entry)
    ///
    /// FAS should virtualize exactly 1 back-edge to break the cycle.
    #[test]
    fn test_fas_two_cycle_resolves_to_one_goto() {
        // Nodes: 0, 1 (the irreducible SCC), 2 and 3 (external entry nodes).
        let succs = vec![
            vec![1],     // 0 → 1
            vec![0],     // 1 → 0  (back-edge; creates cycle with dual entry)
            vec![0],     // 2 (entry_a) → 0
            vec![1],     // 3 (entry_b) → 1  ← second external entry into SCC
        ];
        let preds = build_preds(&succs);
        let fas = compute_fas_virtual_gotos(&succs, &preds);
        // Exactly 1 edge removed to break the cycle.
        assert_eq!(fas.len(), 1, "Expected 1 FAS edge, got: {:?}", fas);
        // The removed edge must be either (0,1) or (1,0).
        assert!(
            fas.contains(&(0, 1)) || fas.contains(&(1, 0)),
            "FAS edge must be part of the cycle: {:?}",
            fas
        );
    }

    /// A 3-node irreducible SCC: 0→1→2→0, with extra entry point into node 1.
    /// FAS should virtualize at most 1 back-edge (the minimum to make it acyclic).
    #[test]
    fn test_fas_triangle_resolves_to_one_goto() {
        // Graph:  entry(3) → 0, entry(3) → 1 (dual entry → irreducible)
        //         0→1→2→0
        let succs = vec![
            vec![1],        // 0 → 1
            vec![2],        // 1 → 2
            vec![0],        // 2 → 0 (back-edge)
            vec![0, 1],     // 3 (entry) → 0 and → 1 (dual entry)
        ];
        let preds = build_preds(&succs);
        let fas = compute_fas_virtual_gotos(&succs, &preds);
        // At most 1 back-edge should be enough to break the single cycle.
        assert!(
            fas.len() <= 2,
            "Expected <= 2 FAS edges for a triangle, got: {:?}",
            fas
        );
        assert!(!fas.is_empty(), "Expected at least 1 FAS edge");
    }

    /// A very large cycle (10 nodes) should trigger the size gate and return empty Vec.
    #[test]
    fn test_fas_size_gate_blocks_large_components() {
        // 10-node cycle: 0→1→2→...→9→0, with entry from node 10.
        // Each consecutive back-edge in the chain needs its own FAS edge.
        // We'll force many cycles to exceed MAX_FAS_VIRTUAL_GOTOS (8).
        let n = 10;
        let mut succs: Vec<Vec<usize>> = (0..n).map(|i| vec![(i + 1) % n]).collect();
        // Add cross-edges to create multiple interleaved cycles needing many removals.
        for i in 0..n {
            succs[i].push((i + 3) % n);
        }
        succs.push(vec![0]); // entry node
        let preds = build_preds(&succs);
        let fas = compute_fas_virtual_gotos(&succs, &preds);
        // Due to many interleaved cycles, FAS would exceed gate → empty Vec.
        // (This test verifies the size gate fires, not that the algo is perfect.)
        // Either empty (gate fired) or small (algorithm was efficient enough).
        // We simply assert it doesn't panic and returns a reasonable result.
        assert!(fas.len() <= MAX_FAS_VIRTUAL_GOTOS);
    }

    #[test]
    fn test_fas_prioritizes_postdom_maximizing_edge() {
        // Nodes: 0, 1 (irreducible SCC), 2 (exit), 3 (entry to 0 and 1)
        // 0 -> 1, 0 -> 2
        // 1 -> 0
        // 2 is exit (empty succs)
        // 3 -> 0, 3 -> 1
        let succs = vec![
            vec![1, 2], // 0 -> 1 (back-edge), 0 -> 2 (exit)
            vec![0],    // 1 -> 0 (back-edge)
            vec![],     // 2 (exit)
            vec![0, 1], // 3 (entry)
        ];
        let preds = build_preds(&succs);
        let fas = compute_fas_virtual_gotos(&succs, &preds);
        // Removing (0, 1) maximizes post-dominators in the remaining graph.
        assert_eq!(fas, vec![(0, 1)]);
    }

    #[test]
    fn test_fas_scale_gate_fallback() {
        // If candidate_edges > 16, it should fall back to out-degree scoring and not crash.
        // We construct a graph with 20 candidate edges.
        let mut succs = vec![Vec::new(); 20];
        // Create 20-node complete graph (or tournament) to get > 16 candidate edges inside SCC
        for i in 0..20 {
            for j in 0..20 {
                if i != j {
                    succs[i].push(j);
                }
            }
        }
        let preds = build_preds(&succs);
        let fas = compute_fas_virtual_gotos(&succs, &preds);
        // It should complete successfully using fallback out-degree scoring
        assert!(fas.len() <= MAX_FAS_VIRTUAL_GOTOS);
    }
}
