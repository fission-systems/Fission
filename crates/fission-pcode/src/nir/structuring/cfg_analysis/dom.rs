//! Dominator trees and dominance frontiers.

use super::util::{
    compute_dominator_sets, cooper_intersect, compute_rpo, nearest_common_from_sets,
    reachable_from,
};
use crate::fast_hash::FastMap as HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomTree {
    roots: Vec<usize>,
    dominators: HashMap<usize, HashSet<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImmDomTree {
    idom: Vec<usize>,
}

impl ImmDomTree {
    pub(crate) fn compute(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                idom: Vec::new(),
            };
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
        let super_root = if roots.len() > 1 { Some(node_count) } else { None };

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

    pub(crate) fn immediate_dominator(&self, n: usize) -> Option<usize> {
        let idom = self.idom.get(n).copied()?;
        if idom == n {
            None
        } else {
            Some(idom)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DominanceFrontier {
    frontiers: Vec<HashSet<usize>>,
}

impl DominanceFrontier {
    pub(crate) fn compute(predecessors: &[Vec<usize>], imm_dom: &ImmDomTree) -> Self {
        let node_count = predecessors.len();
        let mut frontiers = vec![HashSet::new(); node_count];
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

    pub(crate) fn contains(&self, from: usize, to: usize) -> bool {
        self.frontiers
            .get(from)
            .is_some_and(|nodes| nodes.contains(&to))
    }

    #[cfg(test)]
    pub(crate) fn of(&self, node: usize) -> Option<&HashSet<usize>> {
        self.frontiers.get(node)
    }
}

impl DomTree {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
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
            dominators.extend(local);
        }

        for idx in 0..node_count {
            if dominators.contains_key(&idx) {
                continue;
            }
            roots.push(idx);
            let component = reachable_from(idx, successors);
            if component.is_empty() {
                dominators.insert(idx, HashSet::from([idx]));
                continue;
            }
            let local = compute_dominator_sets(&component, predecessors, idx);
            dominators.extend(local);
        }

        Self { roots, dominators }
    }

    pub(crate) fn roots(&self) -> &[usize] {
        &self.roots
    }

    pub(crate) fn dominates(&self, dom: usize, node: usize) -> bool {
        self.dominators
            .get(&node)
            .is_some_and(|set| set.contains(&dom))
    }

    pub(crate) fn nearest_common_dominator(&self, nodes: &[usize]) -> Option<usize> {
        nearest_common_from_sets(&self.dominators, nodes)
    }
}
