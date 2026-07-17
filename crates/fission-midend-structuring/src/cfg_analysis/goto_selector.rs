use super::*;
use std::collections::HashSet;

use crate::irreducible::compute_fas_virtual_gotos;

/// Ghidra `selectGoto` analog: pick one edge to virtualize so collapse can continue.
pub fn select_bad_edge(
    entry: usize,
    exit: usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    already_virtual: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let virtual_set: HashSet<(usize, usize)> = already_virtual.iter().copied().collect();

    for from in entry..exit.min(successors.len()) {
        for &to in &successors[from] {
            if to >= entry && to <= from && !virtual_set.contains(&(from, to)) {
                return Some((from, to));
            }
        }
    }

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
        let edge = select_bad_edge(0, 4, &succs, &preds, &[]).expect("back edge");
        assert_eq!(edge, (2, 1));
    }

    #[test]
    fn select_bad_edge_skips_already_virtualized_edges() {
        let succs = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let preds = vec![vec![], vec![0, 2], vec![1], vec![2]];
        let edge = select_bad_edge(0, 4, &succs, &preds, &[(2, 1)]);
        assert_ne!(edge, Some((2, 1)));
    }
}
