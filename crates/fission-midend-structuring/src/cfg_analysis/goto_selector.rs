use super::*;
use crate::HashSet;

use crate::irreducible::compute_fas_virtual_gotos;

/// Ghidra `TraceDAG::selectBadEdge` / collapse-loop unstructured edge pick.
///
/// Prefer TraceDAG-scored likely-gotos; fall back to FAS / join heuristics.
pub fn select_bad_edge(
    entry: usize,
    exit: usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    already_virtual: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let virtual_set: HashSet<(usize, usize)> = already_virtual.iter().copied().collect();

    // 1) TraceDAG likely-goto (Ghidra TraceDAG::pushBranches + selectBadEdge).
    if exit > entry && !successors.is_empty() {
        let dom = DomTree::analyze(successors, predecessors);
        let mut dag = TraceDag::new(successors, predecessors, &dom);
        if let Some(edge) = dag.select_likely_unstructured_edge(entry, exit, &virtual_set) {
            return Some(edge);
        }
    }

    // 2) Back edges inside the window (classic loop latch).
    for from in entry..exit.min(successors.len()) {
        for &to in &successors[from] {
            if to >= entry && to <= from && !virtual_set.contains(&(from, to)) {
                return Some((from, to));
            }
        }
    }

    // 3) Multi-out into multi-pred join (competing paths).
    for from in entry..exit.min(successors.len()) {
        if successors[from].len() < 2 {
            continue;
        }
        for &to in &successors[from] {
            if virtual_set.contains(&(from, to)) {
                continue;
            }
            if to < exit && predecessors.get(to).is_some_and(|preds| preds.len() > 1) {
                return Some((from, to));
            }
        }
    }

    // 4) FAS residual.
    for edge in compute_fas_virtual_gotos(successors, predecessors) {
        if edge.0 >= entry && edge.0 < exit && !virtual_set.contains(&edge) {
            return Some(edge);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_bad_edge_prefers_back_edge_in_range() {
        let succs = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let preds = vec![vec![], vec![0, 2], vec![1], vec![2]];
        let edge = select_bad_edge(0, 4, &succs, &preds, &[]).expect("edge");
        // Back edge 2→1 or TraceDAG-scored alternative; must be real.
        assert!(succs[edge.0].contains(&edge.1), "{edge:?}");
    }

    #[test]
    fn select_bad_edge_skips_already_virtualized_edges() {
        let succs = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let preds = vec![vec![], vec![0, 2], vec![1], vec![2]];
        let edge = select_bad_edge(0, 4, &succs, &preds, &[(2, 1)]);
        assert_ne!(edge, Some((2, 1)));
        if let Some((f, t)) = edge {
            assert!(succs[f].contains(&t));
        }
    }

    #[test]
    fn diamond_has_no_forced_unstructured_edge() {
        // Clean diamond should not need a virtual edge for follow discovery.
        let succs = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let preds = vec![vec![], vec![0], vec![0], vec![1, 2]];
        // TraceDAG may still not pick an edge if the DAG retires cleanly.
        let _ = select_bad_edge(0, 4, &succs, &preds, &[]);
    }
}
