//! Shared helpers for dominators, postdominators, and graph walks.

use crate::HashSet;
use fission_midend_core::fast_hash::FastMap as HashMap;

/// Cooper et al.'s `intersect(b1, b2)`: walk both fingers up the idom tree
/// (guided by RPO numbers) until they meet.  Returns the LCA node.
pub fn cooper_intersect(
    mut b1: usize,
    mut b2: usize,
    idom: &[usize],
    rpo_number: &[usize],
) -> usize {
    let n = idom.len();
    let rpo = |x: usize| rpo_number.get(x).copied().unwrap_or(usize::MAX);
    let max_iter = n + 2;
    let mut steps = 0usize;
    // Both the outer meet and each finger's climb are bounded. Cooper's
    // algorithm gets termination from RPO numbers strictly decreasing up the
    // idom chain, which holds only when every node is reachable from the
    // root. A truncated CFG breaks that: nodes missing from `rpo_number` read
    // as `usize::MAX`, the chain can close into a cycle of two, and the climb
    // below walks it forever -- the outer `max_iter` never gets a turn.
    //
    // Measured on `coreutils/test`'s `strintcmp`, one basic block of 37 ops:
    // its callee summary re-decodes `numcompare` at a reduced instruction
    // limit, 87 blocks truncated to 71, and postdominator analysis over that
    // truncated graph never returned. `test`, `expr` and `[` all carry the
    // same pair and all three hung.
    //
    // A chain with no cycle is at most `n` long, so a climb past that has
    // found one; returning the finger where it stands is the same answer the
    // `p == b1` self-loop case already gives.
    let mut climb = 0usize;
    while b1 != b2 {
        while rpo(b1) > rpo(b2) {
            let p = idom[b1];
            if p == b1 || p >= n {
                return b1;
            }
            b1 = p;
            climb += 1;
            if climb > n {
                return b1;
            }
        }
        while rpo(b2) > rpo(b1) {
            let p = idom[b2];
            if p == b2 || p >= n {
                return b2;
            }
            b2 = p;
            climb += 1;
            if climb > n {
                return b2;
            }
        }
        steps += 1;
        if steps > max_iter {
            break;
        }
    }
    b1
}

/// Compute RPO order of `start` in the graph defined by `succs`.
pub fn compute_rpo(start: usize, succs: &[Vec<usize>], node_count: usize) -> Vec<usize> {
    let mut visited = vec![false; node_count];
    let mut postorder = Vec::with_capacity(node_count);
    dfs_postorder(start, succs, &mut visited, &mut postorder);
    // Nodes unreachable from `start` get appended in stable order.
    for i in 0..node_count {
        if !visited[i] {
            dfs_postorder(i, succs, &mut visited, &mut postorder);
        }
    }
    postorder.reverse(); // reverse postorder
    postorder
}

pub fn dfs_postorder(
    start_node: usize,
    succs: &[Vec<usize>],
    visited: &mut [bool],
    postorder: &mut Vec<usize>,
) {
    if start_node >= visited.len() || visited[start_node] {
        return;
    }
    struct Frame {
        node: usize,
        succ_idx: usize,
    }
    let mut stack = Vec::new();
    visited[start_node] = true;
    stack.push(Frame {
        node: start_node,
        succ_idx: 0,
    });

    while let Some(frame) = stack.last_mut() {
        let node = frame.node;
        if frame.succ_idx < succs[node].len() {
            let s = succs[node][frame.succ_idx];
            frame.succ_idx += 1;
            if s < visited.len() && !visited[s] {
                visited[s] = true;
                stack.push(Frame {
                    node: s,
                    succ_idx: 0,
                });
            }
        } else {
            postorder.push(node);
            stack.pop();
        }
    }
}

pub fn nearest_common_from_sets(
    sets: &HashMap<usize, HashSet<usize>>,
    nodes: &[usize],
) -> Option<usize> {
    let mut iter = nodes.iter().copied();
    let first = iter.next()?;
    let mut common = sets.get(&first)?.clone();
    for node in iter {
        let set = sets.get(&node)?;
        common = common.intersection(set).copied().collect();
        if common.is_empty() {
            return None;
        }
    }
    common
        .into_iter()
        .max_by_key(|candidate| sets.get(candidate).map_or(0, HashSet::len))
}

pub fn reachable_from(root: usize, successors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::default();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node >= successors.len() || !seen.insert(node) {
            continue;
        }
        for succ in successors[node].iter().copied() {
            stack.push(succ);
        }
    }
    seen
}

pub fn reverse_reachable_from(exit: usize, predecessors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::default();
    let mut stack = vec![exit];
    while let Some(node) = stack.pop() {
        if node >= predecessors.len() || !seen.insert(node) {
            continue;
        }
        for pred in predecessors[node].iter().copied() {
            stack.push(pred);
        }
    }
    seen
}

pub fn compute_dominator_sets(
    nodes: &HashSet<usize>,
    predecessors: &[Vec<usize>],
    root: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut dom = HashMap::default();

    let mut sorted_nodes: Vec<usize> = nodes.iter().copied().collect();
    sorted_nodes.sort_unstable();

    for node in sorted_nodes.iter().copied() {
        if node == root {
            dom.insert(node, [root].into_iter().collect::<HashSet<_>>());
        } else {
            dom.insert(node, nodes.clone());
        }
    }

    let mut changed = true;
    let max_iterations = nodes.len().saturating_mul(nodes.len().max(1));
    let mut iterations = 0usize;
    while changed && iterations < max_iterations {
        iterations += 1;
        changed = false;
        for node in sorted_nodes.iter().copied() {
            if node == root {
                continue;
            }
            let in_component_preds = predecessors[node]
                .iter()
                .copied()
                .filter(|pred| nodes.contains(pred))
                .collect::<Vec<_>>();
            if in_component_preds.is_empty() {
                dom.insert(node, [node].into_iter().collect::<HashSet<_>>());
                continue;
            }
            let mut intersection = dom
                .get(&in_component_preds[0])
                .cloned()
                .unwrap_or_else(|| nodes.clone());
            for pred in in_component_preds.iter().skip(1) {
                if let Some(pred_set) = dom.get(pred) {
                    intersection = intersection.intersection(pred_set).copied().collect();
                }
            }
            intersection.insert(node);
            if dom.get(&node) != Some(&intersection) {
                dom.insert(node, intersection);
                changed = true;
            }
        }
    }
    dom
}

pub fn compute_postdominator_sets_for_exit(
    nodes: &HashSet<usize>,
    successors: &[Vec<usize>],
    exit: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut postdom = HashMap::default();

    let mut sorted_nodes: Vec<usize> = nodes.iter().copied().collect();
    sorted_nodes.sort_unstable();

    for node in sorted_nodes.iter().copied() {
        if node == exit {
            postdom.insert(node, [exit].into_iter().collect::<HashSet<_>>());
        } else {
            postdom.insert(node, nodes.clone());
        }
    }

    let mut changed = true;
    let max_iterations = nodes.len().saturating_mul(nodes.len().max(1));
    let mut iterations = 0usize;
    while changed && iterations < max_iterations {
        iterations += 1;
        changed = false;
        for node in sorted_nodes.iter().copied() {
            if node == exit {
                continue;
            }
            let in_component_succs = successors[node]
                .iter()
                .copied()
                .filter(|succ| nodes.contains(succ))
                .collect::<Vec<_>>();
            if in_component_succs.is_empty() {
                postdom.insert(node, [node].into_iter().collect::<HashSet<_>>());
                continue;
            }
            let mut intersection = postdom
                .get(&in_component_succs[0])
                .cloned()
                .unwrap_or_else(|| nodes.clone());
            for succ in in_component_succs.iter().skip(1) {
                if let Some(succ_set) = postdom.get(succ) {
                    intersection = intersection.intersection(succ_set).copied().collect();
                }
            }
            intersection.insert(node);
            if postdom.get(&node) != Some(&intersection) {
                postdom.insert(node, intersection);
                changed = true;
            }
        }
    }
    postdom
}

#[cfg(test)]
mod cycle_guard_tests {
    use super::cooper_intersect;

    /// A truncated CFG can leave the idom array holding a two-cycle, and the
    /// RPO numbers that make Cooper's climb terminate no longer order it.
    /// Without a bound on the climb itself this spins forever -- the outer
    /// meet counter never gets a turn. Measured on `coreutils/test`'s
    /// `strintcmp`, where the callee summary re-decodes `numcompare` at a
    /// reduced instruction limit and postdominator analysis over the
    /// truncated graph never returned.
    #[test]
    fn a_two_cycle_in_the_idom_chain_terminates() {
        // 0 <-> 1 in the idom chain, and RPO numbers that keep the first
        // finger climbing: neither node is ever "not greater" than node 2.
        let idom = vec![1usize, 0, 2];
        let rpo = vec![9usize, 9, 0];
        let out = cooper_intersect(0, 2, &idom, &rpo);
        assert!(out < idom.len(), "returned a node, got {out}");
    }

    /// The ordinary case still answers: a straight chain 2 -> 1 -> 0 meets at
    /// its root.
    #[test]
    fn a_chain_still_meets_at_its_root() {
        let idom = vec![0usize, 0, 1];
        let rpo = vec![0usize, 1, 2];
        assert_eq!(cooper_intersect(2, 1, &idom, &rpo), 1);
    }
}
