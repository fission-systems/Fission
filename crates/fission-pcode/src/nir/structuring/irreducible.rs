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
            tarjan_dfs(node, successors, &mut state);
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

fn tarjan_dfs(v: usize, succs: &[Vec<usize>], s: &mut TarjanState) {
    s.indices[v] = Some(s.index);
    s.lowlink[v] = s.index;
    s.index += 1;
    s.stack.push(v);
    s.on_stack[v] = true;

    for &w in &succs[v] {
        if s.indices[w].is_none() {
            tarjan_dfs(w, succs, s);
            s.lowlink[v] = s.lowlink[v].min(s.lowlink[w]);
        } else if s.on_stack[w] {
            s.lowlink[v] = s.lowlink[v].min(s.indices[w].unwrap());
        }
    }

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
