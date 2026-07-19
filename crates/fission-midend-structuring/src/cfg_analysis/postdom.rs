//! Post-dominator sets and immediate postdominator tree (Cooper / reverse CFG).

use super::util::{
    compute_postdominator_sets_for_exit, compute_rpo, cooper_intersect, nearest_common_from_sets,
    reverse_reachable_from,
};
use fission_midend_core::fast_hash::FastMap as HashMap;
use crate::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostDomTree {
    exits: Vec<usize>,
    postdominators: HashMap<usize, HashSet<usize>>,
}

impl PostDomTree {
    pub fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                exits: Vec::new(),
                postdominators: HashMap::default(),
            };
        }

        let mut exits = successors
            .iter()
            .enumerate()
            .filter_map(|(idx, succs)| succs.is_empty().then_some(idx))
            .collect::<Vec<_>>();
        if exits.is_empty() {
            exits.push(node_count - 1);
        }

        let mut postdominators = HashMap::default();
        for exit in exits.iter().copied() {
            let component = reverse_reachable_from(exit, predecessors);
            if component.is_empty() {
                continue;
            }
            let local = compute_postdominator_sets_for_exit(&component, successors, exit);
            postdominators.extend(local);
        }

        for idx in 0..node_count {
            postdominators
                .entry(idx)
                .or_insert_with(|| [idx].into_iter().collect::<HashSet<_>>());
        }

        Self {
            exits,
            postdominators,
        }
    }

    pub fn analyze_window_with_exit(
        successors: &[Vec<usize>],
        window: &HashSet<usize>,
        exit: usize,
    ) -> Option<Self> {
        if !window.contains(&exit) {
            return None;
        }
        let sets = compute_postdominator_sets_for_exit(window, successors, exit);
        Some(Self {
            exits: vec![exit],
            postdominators: sets,
        })
    }

    pub fn exits(&self) -> &[usize] {
        &self.exits
    }

    pub fn postdominators(&self) -> &HashMap<usize, HashSet<usize>> {
        &self.postdominators
    }

    pub fn nearest_common_postdominator(&self, nodes: &[usize]) -> Option<usize> {
        nearest_common_from_sets(&self.postdominators, nodes)
    }
}

/// Immediate-postdominator tree computed via Cooper's algorithm on the reverse CFG.
///
/// For each node n, `idom[n]` is the unique node that immediately postdominates n
/// (i.e. the nearest strict postdominator of n on every path from n to any exit).
///
/// `nodes_by_rpo` stores indices ordered by reverse-postorder on the *reverse* CFG
/// (= postorder on the forward CFG), which is the traversal order required by
/// Cooper et al.'s "A Simple, Fast Dominance Algorithm" (2001).
///
/// Nodes unreachable from any exit (or disconnected loops) get `idom[n] = n`
/// (no strict postdominator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmPostDomTree {
    /// idom[n] = immediate postdominator of n, or n itself if none.
    idom: Vec<usize>,
    /// RPO numbers on the reverse CFG (used by Cooper's intersect).
    rpo_number: Vec<usize>,
    /// Sentinel: all nodes >= node_count are treated as virtual exits.
    node_count: usize,
}

impl ImmPostDomTree {
    /// Compute the immediate-postdominator tree using Cooper's algorithm.
    ///
    /// `successors[n]` = forward edges from n.
    /// `predecessors[n]` = reverse of successors (forward edges *into* n).
    pub fn compute(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                idom: Vec::new(),
                rpo_number: Vec::new(),
                node_count: 0,
            };
        }

        // Find exit nodes (no forward successors) in the original CFG.
        // For postdominators, these are the roots of the reverse CFG.
        let mut exits: Vec<usize> = successors
            .iter()
            .enumerate()
            .filter_map(|(i, succs)| succs.is_empty().then_some(i))
            .collect();
        if exits.is_empty() {
            exits.push(node_count.saturating_sub(1));
        }

        // If there are multiple exits, add a virtual super-exit node (index = node_count).
        // On the reverse CFG the virtual node has predecessors = exits (forward) and
        // no successors (reverse); on the *forward* CFG perspective the virtual node
        // succeeds all exits.
        let total_nodes = node_count + if exits.len() > 1 { 1 } else { 0 };
        let super_exit = if exits.len() > 1 {
            Some(node_count)
        } else {
            None
        };

        // Build the reverse CFG (edges go from successor to predecessor in the original CFG).
        // In the reverse CFG: reverse_succs[v] = predecessors[v] (forward preds),
        //                     reverse_preds[v] = successors[v]  (forward succs).
        // We also hook super_exit → each real exit in the reverse CFG.
        let mut rev_succs: Vec<Vec<usize>> = predecessors.to_vec();
        rev_succs.resize(total_nodes, Vec::new());
        let mut rev_preds: Vec<Vec<usize>> = successors.to_vec();
        rev_preds.resize(total_nodes, Vec::new());
        if let Some(se) = super_exit {
            for &exit in &exits {
                rev_succs[se].push(exit);
                rev_preds[exit].push(se);
            }
        }

        let start = super_exit.unwrap_or(exits[0]);

        // Compute RPO of the reverse CFG starting from `start` (= virtual/actual exit).
        let rpo_order = compute_rpo(start, &rev_succs, total_nodes);

        // Build RPO number map: rpo_number[n] = position in RPO traversal.
        let mut rpo_number = vec![usize::MAX; total_nodes];
        for (pos, &n) in rpo_order.iter().enumerate() {
            rpo_number[n] = pos;
        }

        // Cooper's algorithm: iteratively compute idom.
        // UNDEF sentinel: idom[n] = total_nodes means "not yet assigned".
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
                // Predecessors in the reverse CFG = rev_preds[n] = forward successors of n.
                let mut new_idom = UNDEF;
                for &p in &rev_preds[n] {
                    if idom[p] == UNDEF {
                        continue; // predecessor not yet processed
                    }
                    if new_idom == UNDEF {
                        new_idom = p;
                    } else {
                        new_idom = cooper_intersect(new_idom, p, &idom, &rpo_number);
                    }
                }
                if new_idom == UNDEF {
                    // Unreachable from any exit; self-loop sentinel.
                    new_idom = n;
                }
                if idom[n] != new_idom {
                    idom[n] = new_idom;
                    changed = true;
                }
            }
        }

        // For nodes unreachable from the reverse-CFG root, fall back to self.
        for i in 0..total_nodes {
            if idom[i] == UNDEF {
                idom[i] = i;
            }
        }

        // Trim back to node_count (remove virtual super-exit slot if present).
        idom.truncate(node_count);
        rpo_number.truncate(node_count);

        // Remap any idom pointing at the virtual super-exit to self.
        for i in 0..node_count {
            if idom[i] >= node_count {
                idom[i] = i;
            }
        }

        Self {
            idom,
            rpo_number,
            node_count,
        }
    }

    /// Immediate postdominator of `n`, or `None` if `n` has no strict postdominator
    /// (i.e. `n` is an exit node or part of a disconnected loop).
    pub fn immediate_postdominator(&self, n: usize) -> Option<usize> {
        let ipdom = self.idom.get(n).copied()?;
        if ipdom == n { None } else { Some(ipdom) }
    }

    /// Nearest common postdominator of a set of nodes (LCA in the idom tree).
    /// Returns `None` if the set is empty or no common postdominator exists.
    pub fn nearest_common_postdominator(&self, nodes: &[usize]) -> Option<usize> {
        let mut iter = nodes.iter().copied().filter(|&n| n < self.node_count);
        let mut result = iter.next()?;
        for n in iter {
            result = self.lca(result, n)?;
        }
        // LCA can return one of the input nodes themselves if one postdominates the other;
        // for "follow block" purposes we want a *strict* postdominator of the branch.
        // Return None if the LCA equals one of the original nodes and that node is a
        // branch arm (not a join block).  For simplicity we return the LCA as-is and
        // let callers filter based on index ordering.
        Some(result)
    }

    /// Merge point for two forward CFG arms (e.g. then/else) when both paths reconverge:
    /// nearest common postdominator in the Cooper immediate-postdominator tree.
    #[allow(dead_code)]
    pub fn merge_point_for_two_arms(&self, arm_a: usize, arm_b: usize) -> Option<usize> {
        self.nearest_common_postdominator(&[arm_a, arm_b])
    }

    /// LCA (= nearest common postdominator) of two nodes using Cooper's intersect.
    fn lca(&self, mut a: usize, mut b: usize) -> Option<usize> {
        if a >= self.node_count || b >= self.node_count {
            return None;
        }
        // Walk up the idom tree until both fingers meet.
        let max_iter = self.node_count + 2;
        let mut steps = 0usize;
        while a != b {
            while self.rpo_number.get(a).copied().unwrap_or(usize::MAX)
                > self.rpo_number.get(b).copied().unwrap_or(usize::MAX)
            {
                let parent = self.idom[a];
                if parent == a {
                    return None; // no common ancestor
                }
                a = parent;
            }
            while self.rpo_number.get(b).copied().unwrap_or(usize::MAX)
                > self.rpo_number.get(a).copied().unwrap_or(usize::MAX)
            {
                let parent = self.idom[b];
                if parent == b {
                    return None;
                }
                b = parent;
            }
            steps += 1;
            if steps > max_iter {
                return None;
            }
        }
        Some(a)
    }
}
