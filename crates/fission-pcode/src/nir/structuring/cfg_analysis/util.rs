//! Shared helpers for dominators, postdominators, and graph walks.

use crate::fast_hash::FastMap as HashMap;
use std::collections::HashSet;

/// Cooper et al.'s `intersect(b1, b2)`: walk both fingers up the idom tree
/// (guided by RPO numbers) until they meet.  Returns the LCA node.
pub(super) fn cooper_intersect(
    mut b1: usize,
    mut b2: usize,
    idom: &[usize],
    rpo_number: &[usize],
) -> usize {
    let n = idom.len();
    let rpo = |x: usize| rpo_number.get(x).copied().unwrap_or(usize::MAX);
    let max_iter = n + 2;
    let mut steps = 0usize;
    while b1 != b2 {
        while rpo(b1) > rpo(b2) {
            let p = idom[b1];
            if p == b1 || p >= n {
                return b1;
            }
            b1 = p;
        }
        while rpo(b2) > rpo(b1) {
            let p = idom[b2];
            if p == b2 || p >= n {
                return b2;
            }
            b2 = p;
        }
        steps += 1;
        if steps > max_iter {
            break;
        }
    }
    b1
}

/// Compute RPO order of `start` in the graph defined by `succs`.
pub(crate) fn compute_rpo(start: usize, succs: &[Vec<usize>], node_count: usize) -> Vec<usize> {
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

pub(crate) fn dfs_postorder(
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

pub(super) fn nearest_common_from_sets(
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

pub(super) fn reachable_from(root: usize, successors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::new();
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

pub(super) fn reverse_reachable_from(exit: usize, predecessors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::new();
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

pub(super) fn compute_dominator_sets(
    nodes: &HashSet<usize>,
    predecessors: &[Vec<usize>],
    root: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut dom = HashMap::default();

    let mut sorted_nodes: Vec<usize> = nodes.iter().copied().collect();
    sorted_nodes.sort_unstable();

    for node in sorted_nodes.iter().copied() {
        if node == root {
            dom.insert(node, HashSet::from([root]));
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
                dom.insert(node, HashSet::from([node]));
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

pub(super) fn compute_postdominator_sets_for_exit(
    nodes: &HashSet<usize>,
    successors: &[Vec<usize>],
    exit: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut postdom = HashMap::default();

    let mut sorted_nodes: Vec<usize> = nodes.iter().copied().collect();
    sorted_nodes.sort_unstable();

    for node in sorted_nodes.iter().copied() {
        if node == exit {
            postdom.insert(node, HashSet::from([exit]));
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
                postdom.insert(node, HashSet::from([node]));
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
