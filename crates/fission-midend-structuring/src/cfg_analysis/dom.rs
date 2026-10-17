//! Dominator trees and dominance frontiers.

use super::util::{
    compute_dominator_sets, compute_rpo, cooper_intersect, nearest_common_from_sets, reachable_from,
};
use crate::HashSet;
use fission_midend_core::fast_hash::FastMap as HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomTree {
    roots: Vec<usize>,
    dominators: HashMap<usize, HashSet<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmDomTree {
    idom: Vec<usize>,
}

impl ImmDomTree {
    pub fn compute(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self { idom: Vec::new() };
        }

        let mut roots: Vec<usize> = predecessors
            .iter()
            .enumerate()
            .filter_map(|(i, preds)| preds.is_empty().then_some(i))
            .collect();
        if roots.is_empty() {
            roots.push(0);
        }

        let total_nodes = node_count + if roots.len() > 1 { 1 } else { 0 };
        let super_root = if roots.len() > 1 {
            Some(node_count)
        } else {
            None
        };

        let mut fwd_succs: Vec<Vec<usize>> = successors.to_vec();
        fwd_succs.resize(total_nodes, Vec::new());
        let mut fwd_preds: Vec<Vec<usize>> = predecessors.to_vec();
        fwd_preds.resize(total_nodes, Vec::new());
        if let Some(sr) = super_root {
            for &root in &roots {
                fwd_succs[sr].push(root);
                fwd_preds[root].push(sr);
            }
        }

        let start = super_root.unwrap_or(roots[0]);
        let rpo_order = compute_rpo(start, &fwd_succs, total_nodes);
        let mut rpo_number = vec![usize::MAX; total_nodes];
        for (pos, &n) in rpo_order.iter().enumerate() {
            rpo_number[n] = pos;
        }

        const UNDEF: usize = usize::MAX;
        let mut idom = vec![UNDEF; total_nodes];
        idom[start] = start;

        let mut changed = true;
        while changed {
            changed = false;
            for &n in &rpo_order {
                if n == start {
                    continue;
                }
                let mut new_idom = UNDEF;
                for &p in &fwd_preds[n] {
                    if idom[p] == UNDEF {
                        continue;
                    }
                    if new_idom == UNDEF {
                        new_idom = p;
                    } else {
                        new_idom = cooper_intersect(new_idom, p, &idom, &rpo_number);
                    }
                }
                if new_idom == UNDEF {
                    new_idom = n;
                }
                if idom[n] != new_idom {
                    idom[n] = new_idom;
                    changed = true;
                }
            }
        }

        for i in 0..total_nodes {
            if idom[i] == UNDEF {
                idom[i] = i;
            }
        }

        idom.truncate(node_count);
        rpo_number.truncate(node_count);
        for i in 0..node_count {
            if idom[i] >= node_count {
                idom[i] = i;
            }
        }

        Self { idom }
    }

    pub fn immediate_dominator(&self, n: usize) -> Option<usize> {
        let idom = self.idom.get(n).copied()?;
        if idom == n { None } else { Some(idom) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DominanceFrontier {
    frontiers: Vec<HashSet<usize>>,
}

impl DominanceFrontier {
    pub fn compute(predecessors: &[Vec<usize>], imm_dom: &ImmDomTree) -> Self {
        let node_count = predecessors.len();
        let mut frontiers = vec![HashSet::default(); node_count];
        for block in 0..node_count {
            if predecessors[block].len() < 2 {
                continue;
            }
            let Some(idom_block) = imm_dom.immediate_dominator(block) else {
                continue;
            };
            for mut runner in predecessors[block].iter().copied() {
                if runner >= node_count {
                    continue;
                }
                let mut hops = 0usize;
                while runner != idom_block {
                    frontiers[runner].insert(block);
                    let Some(parent) = imm_dom.immediate_dominator(runner) else {
                        break;
                    };
                    if parent == runner {
                        break;
                    }
                    runner = parent;
                    hops += 1;
                    if hops > node_count + 1 {
                        break;
                    }
                }
            }
        }
        Self { frontiers }
    }

    pub fn contains(&self, from: usize, to: usize) -> bool {
        self.frontiers
            .get(from)
            .is_some_and(|nodes| nodes.contains(&to))
    }

    #[cfg(test)]
    pub fn of(&self, node: usize) -> Option<&HashSet<usize>> {
        self.frontiers.get(node)
    }
}

impl DomTree {
    pub fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                roots: Vec::new(),
                dominators: HashMap::default(),
            };
        }

        let mut roots = predecessors
            .iter()
            .enumerate()
            .filter_map(|(idx, preds)| preds.is_empty().then_some(idx))
            .collect::<Vec<_>>();
        if roots.is_empty() {
            roots.push(0);
        }

        let mut dominators = HashMap::default();
        for root in roots.iter().copied() {
            let component = reachable_from(root, successors);
            if component.is_empty() {
                continue;
            }
            let local = compute_dominator_sets(&component, predecessors, root);
            Self::merge_root_dominators(&mut dominators, local);
        }

        for idx in 0..node_count {
            if dominators.contains_key(&idx) {
                continue;
            }
            roots.push(idx);
            let component = reachable_from(idx, successors);
            if component.is_empty() {
                dominators.insert(idx, [idx].into_iter().collect::<HashSet<_>>());
                continue;
            }
            let local = compute_dominator_sets(&component, predecessors, idx);
            Self::merge_root_dominators(&mut dominators, local);
        }

        Self { roots, dominators }
    }

    /// A shared descendant must be dominated along every entry path. Keeping
    /// only the last root's set admits definitions from paths that need not
    /// execute. Intersecting the root-local sets is equivalent to connecting
    /// all discovered entries to a virtual root, then removing that root.
    fn merge_root_dominators(
        dominators: &mut HashMap<usize, HashSet<usize>>,
        local: HashMap<usize, HashSet<usize>>,
    ) {
        for (node, incoming) in local {
            dominators
                .entry(node)
                .and_modify(|current| current.retain(|dom| incoming.contains(dom)))
                .or_insert(incoming);
        }
    }

    pub fn roots(&self) -> &[usize] {
        &self.roots
    }

    pub fn dominates(&self, dom: usize, node: usize) -> bool {
        self.dominators
            .get(&node)
            .is_some_and(|set| set.contains(&dom))
    }

    pub fn dominance_depth(&self, node: usize) -> usize {
        self.dominators.get(&node).map_or(0, HashSet::len)
    }

    pub fn nearest_common_dominator(&self, nodes: &[usize]) -> Option<usize> {
        nearest_common_from_sets(&self.dominators, nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_predecessor_index_map(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
        let mut predecessors = vec![Vec::new(); successors.len()];
        for (from, targets) in successors.iter().enumerate() {
            for &to in targets {
                predecessors[to].push(from);
            }
        }
        predecessors
    }

    #[test]
    fn dom_tree_shared_tail_intersects_all_entry_paths() {
        let successors = vec![vec![1], vec![3], vec![3], vec![4], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let dom = DomTree::analyze(&successors, &predecessors);

        assert_eq!(dom.roots(), &[0, 2]);
        assert!(dom.dominates(0, 1));
        for entry_path_node in [0, 1, 2] {
            assert!(!dom.dominates(entry_path_node, 3));
            assert!(!dom.dominates(entry_path_node, 4));
        }
        assert!(dom.dominates(3, 4));
        assert_eq!(dom.nearest_common_dominator(&[1, 2]), None);
        assert_eq!(dom.nearest_common_dominator(&[3, 4]), Some(3));
    }

    #[test]
    fn dom_tree_secondary_entry_into_loop_does_not_claim_primary_path() {
        let successors = vec![vec![1], vec![2, 3], vec![1], vec![], vec![2]];
        let predecessors = build_predecessor_index_map(&successors);
        let dom = DomTree::analyze(&successors, &predecessors);

        assert!(!dom.dominates(0, 1));
        assert!(!dom.dominates(4, 1));
        assert!(!dom.dominates(1, 2));
        assert!(!dom.dominates(2, 1));
        assert!(dom.dominates(1, 3));
    }

    #[test]
    fn dom_tree_keeps_disconnected_closed_component_root() {
        let successors = vec![vec![1], vec![], vec![3], vec![2]];
        let predecessors = build_predecessor_index_map(&successors);
        let dom = DomTree::analyze(&successors, &predecessors);

        assert_eq!(dom.roots(), &[0, 2]);
        assert!(dom.dominates(0, 1));
        assert!(dom.dominates(2, 3));
        assert!(!dom.dominates(3, 2));
        assert!(!dom.dominates(0, 3));
    }

    #[test]
    fn dom_tree_matches_path_exclusion_on_all_four_node_graphs() {
        const NODES: usize = 4;
        let edges: Vec<_> = (0..NODES)
            .flat_map(|from| {
                (0..NODES)
                    .filter(move |to| *to != from)
                    .map(move |to| (from, to))
            })
            .collect();
        for edge_mask in 0usize..(1 << edges.len()) {
            let mut successors = vec![Vec::new(); NODES];
            for (bit, &(from, to)) in edges.iter().enumerate() {
                if edge_mask & (1 << bit) != 0 {
                    successors[from].push(to);
                }
            }
            let predecessors = build_predecessor_index_map(&successors);
            let dom = DomTree::analyze(&successors, &predecessors);
            for excluded in 0..NODES {
                let mut reachable = [false; NODES];
                let mut pending = dom.roots().to_vec();
                while let Some(node) = pending.pop() {
                    if node == excluded || std::mem::replace(&mut reachable[node], true) {
                        continue;
                    }
                    pending.extend(successors[node].iter().copied());
                }
                for (node, &can_bypass) in reachable.iter().enumerate() {
                    assert_eq!(
                        dom.dominates(excluded, node),
                        node == excluded || !can_bypass,
                        "graph={successors:?} roots={:?} dominator={excluded} node={node}",
                        dom.roots(),
                    );
                }
            }
        }
    }
}
