//! Dominator Tree computation
//!
//! Implements the Lengauer-Tarjan algorithm for computing dominators.
//! A block B dominates block A if every path from the entry to A goes through B.

use super::{CfgError, CfgResult, ControlFlowGraph};
use std::collections::{HashMap, HashSet};

/// Dominator Tree structure
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Immediate dominator for each block (block_idx -> idom)
    pub idom: HashMap<usize, usize>,
    /// Children in the dominator tree (block_idx -> children)
    pub children: HashMap<usize, Vec<usize>>,
    /// Dominance frontier for each block
    pub dominance_frontier: HashMap<usize, HashSet<usize>>,
    /// Dominator tree depth for each block
    pub depth: HashMap<usize, usize>,
    /// Entry block (root of the tree)
    pub root: usize,
}

impl DominatorTree {
    /// Compute the dominator tree for a CFG using Cooper-Harvey-Kennedy algorithm
    /// (simpler iterative algorithm suitable for reducible graphs)
    pub fn compute(cfg: &ControlFlowGraph) -> CfgResult<Self> {
        if cfg.blocks.is_empty() {
            return Err(CfgError::NoEntryPoint);
        }

        let entry = cfg.entry_block;
        let rpo = cfg.reverse_postorder();

        // Map block index to RPO position for quick lookup
        let rpo_num: HashMap<usize, usize> = rpo
            .iter()
            .enumerate()
            .map(|(pos, &block_idx)| (block_idx, pos))
            .collect();

        // Initialize dominators
        let mut idom: HashMap<usize, usize> = HashMap::new();
        idom.insert(entry, entry); // Entry dominates itself

        // Iteratively compute dominators
        let mut changed = true;
        while changed {
            changed = false;

            for &block_idx in &rpo {
                if block_idx == entry {
                    continue;
                }

                let preds = cfg.predecessors(block_idx);
                if preds.is_empty() {
                    continue;
                }

                // Find first predecessor with computed dominator
                let mut new_idom: Option<usize> = None;
                for &pred in &preds {
                    if idom.contains_key(&pred) {
                        new_idom = Some(pred);
                        break;
                    }
                }

                if let Some(mut new_idom_val) = new_idom {
                    // Intersect with other predecessors
                    for &pred in &preds {
                        if idom.contains_key(&pred) && pred != new_idom_val {
                            new_idom_val = Self::intersect(&idom, &rpo_num, pred, new_idom_val);
                        }
                    }

                    // Check if changed
                    if idom.get(&block_idx) != Some(&new_idom_val) {
                        idom.insert(block_idx, new_idom_val);
                        changed = true;
                    }
                }
            }
        }

        // Build children map
        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        for (&block, &dom) in &idom {
            if block != dom {
                children.entry(dom).or_default().push(block);
            }
        }

        // Compute dominator tree depth
        let mut depth: HashMap<usize, usize> = HashMap::new();
        Self::compute_depth(entry, 0, &children, &mut depth);

        // Compute dominance frontier
        let dominance_frontier = Self::compute_dominance_frontier(cfg, &idom);

        Ok(DominatorTree {
            idom,
            children,
            dominance_frontier,
            depth,
            root: entry,
        })
    }

    /// Find intersection of two dominators in the dominator tree
    #[allow(clippy::while_immutable_condition)]
    fn intersect(
        idom: &HashMap<usize, usize>,
        rpo_num: &HashMap<usize, usize>,
        mut b1: usize,
        mut b2: usize,
    ) -> usize {
        while b1 != b2 {
            let rpo1 = rpo_num.get(&b1).copied().unwrap_or(usize::MAX);
            let rpo2 = rpo_num.get(&b2).copied().unwrap_or(usize::MAX);

            while rpo1 > rpo2 {
                if let Some(&dom) = idom.get(&b1) {
                    if b1 == dom {
                        break;
                    }
                    b1 = dom;
                } else {
                    break;
                }
                let new_rpo1 = rpo_num.get(&b1).copied().unwrap_or(usize::MAX);
                if new_rpo1 >= rpo1 {
                    break;
                }
            }

            let rpo1 = rpo_num.get(&b1).copied().unwrap_or(usize::MAX);
            while rpo2 > rpo1 {
                if let Some(&dom) = idom.get(&b2) {
                    if b2 == dom {
                        break;
                    }
                    b2 = dom;
                } else {
                    break;
                }
                let new_rpo2 = rpo_num.get(&b2).copied().unwrap_or(usize::MAX);
                if new_rpo2 >= rpo2 {
                    break;
                }
            }

            // Safety check to avoid infinite loop
            if rpo_num.get(&b1) == rpo_num.get(&b2) && b1 != b2 {
                break;
            }
        }
        b1
    }

    /// Compute depth in dominator tree
    fn compute_depth(
        block: usize,
        current_depth: usize,
        children: &HashMap<usize, Vec<usize>>,
        depth: &mut HashMap<usize, usize>,
    ) {
        depth.insert(block, current_depth);
        if let Some(kids) = children.get(&block) {
            for &child in kids {
                Self::compute_depth(child, current_depth + 1, children, depth);
            }
        }
    }

    /// Compute dominance frontier for all blocks
    fn compute_dominance_frontier(
        cfg: &ControlFlowGraph,
        idom: &HashMap<usize, usize>,
    ) -> HashMap<usize, HashSet<usize>> {
        let mut df: HashMap<usize, HashSet<usize>> = HashMap::new();

        for block_idx in 0..cfg.blocks.len() {
            df.insert(block_idx, HashSet::new());
        }

        for block_idx in 0..cfg.blocks.len() {
            let preds = cfg.predecessors(block_idx);
            if preds.len() >= 2 {
                // Join point - this block is in the DF of some predecessors
                for &pred in &preds {
                    let mut runner = pred;
                    while runner != *idom.get(&block_idx).unwrap_or(&block_idx) {
                        df.entry(runner).or_default().insert(block_idx);
                        if let Some(&next) = idom.get(&runner) {
                            if next == runner {
                                break;
                            }
                            runner = next;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        df
    }

    /// Check if block A dominates block B
    pub fn dominates(&self, a: usize, b: usize) -> bool {
        if a == b {
            return true;
        }

        let mut current = b;
        while let Some(&dom) = self.idom.get(&current) {
            if dom == current {
                return false;
            }
            if dom == a {
                return true;
            }
            current = dom;
        }
        false
    }

    /// Check if block A strictly dominates block B (dominates but not equal)
    pub fn strictly_dominates(&self, a: usize, b: usize) -> bool {
        a != b && self.dominates(a, b)
    }

    /// Get the immediate dominator of a block
    pub fn get_idom(&self, block: usize) -> Option<usize> {
        self.idom.get(&block).copied().filter(|&dom| dom != block)
    }

    /// Get all blocks dominated by a given block
    pub fn get_dominated(&self, block: usize) -> Vec<usize> {
        let mut dominated = Vec::new();
        self.collect_dominated(block, &mut dominated);
        dominated
    }

    fn collect_dominated(&self, block: usize, result: &mut Vec<usize>) {
        result.push(block);
        if let Some(kids) = self.children.get(&block) {
            for &child in kids {
                self.collect_dominated(child, result);
            }
        }
    }

    /// Get the dominance frontier of a block
    pub fn get_dominance_frontier(&self, block: usize) -> HashSet<usize> {
        self.dominance_frontier
            .get(&block)
            .cloned()
            .unwrap_or_default()
    }

    /// Get tree depth of a block
    pub fn get_depth(&self, block: usize) -> usize {
        self.depth.get(&block).copied().unwrap_or(0)
    }

    /// Find the lowest common dominator of two blocks
    pub fn lca(&self, a: usize, b: usize) -> usize {
        let depth_a = self.get_depth(a);
        let depth_b = self.get_depth(b);

        // Move deeper node up to same level
        let mut current_a = a;
        let mut current_b = b;

        for _ in 0..(depth_a.saturating_sub(depth_b)) {
            if let Some(&dom) = self.idom.get(&current_a) {
                current_a = dom;
            }
        }

        for _ in 0..(depth_b.saturating_sub(depth_a)) {
            if let Some(&dom) = self.idom.get(&current_b) {
                current_b = dom;
            }
        }

        // Move both up until they meet
        while current_a != current_b {
            if let Some(&dom_a) = self.idom.get(&current_a) {
                current_a = dom_a;
            }
            if let Some(&dom_b) = self.idom.get(&current_b) {
                current_b = dom_b;
            }
        }

        current_a
    }
}

impl std::fmt::Display for DominatorTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Dominator Tree (root: BB{}):", self.root)?;

        fn print_tree(
            f: &mut std::fmt::Formatter<'_>,
            tree: &DominatorTree,
            block: usize,
            indent: usize,
        ) -> std::fmt::Result {
            writeln!(f, "{}BB{}", "  ".repeat(indent), block)?;
            if let Some(children) = tree.children.get(&block) {
                for &child in children {
                    print_tree(f, tree, child, indent + 1)?;
                }
            }
            Ok(())
        }

        print_tree(f, self, self.root, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg::{BasicBlock, BlockEdge, EdgeKind};

    fn create_diamond_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();

        // Entry -> Branch -> Merge
        //       -> Other  ->
        let mut b0 = BasicBlock::new(0, 0x1000);
        b0.is_entry = true;
        b0.successors = vec![
            BlockEdge::new(1, EdgeKind::ConditionalTrue),
            BlockEdge::new(2, EdgeKind::ConditionalFalse),
        ];

        let mut b1 = BasicBlock::new(1, 0x1010);
        b1.successors = vec![BlockEdge::new(3, EdgeKind::Unconditional)];
        b1.predecessors = vec![0];

        let mut b2 = BasicBlock::new(2, 0x1020);
        b2.successors = vec![BlockEdge::new(3, EdgeKind::Unconditional)];
        b2.predecessors = vec![0];

        let mut b3 = BasicBlock::new(3, 0x1030);
        b3.is_exit = true;
        b3.predecessors = vec![1, 2];

        cfg.blocks = vec![b0, b1, b2, b3];
        cfg.entry_block = 0;
        cfg.exit_blocks = vec![3];

        cfg
    }

    #[test]
    fn test_dominator_computation() {
        let cfg = create_diamond_cfg();
        let Ok(dom_tree) = DominatorTree::compute(&cfg) else {
            panic!("dominator tree computation should succeed")
        };

        // Block 0 dominates all blocks
        assert!(dom_tree.dominates(0, 0));
        assert!(dom_tree.dominates(0, 1));
        assert!(dom_tree.dominates(0, 2));
        assert!(dom_tree.dominates(0, 3));

        // Block 1 doesn't dominate block 3 (can reach 3 via block 2)
        assert!(!dom_tree.dominates(1, 3));
        assert!(!dom_tree.dominates(2, 3));

        // Check immediate dominators
        assert_eq!(dom_tree.get_idom(1), Some(0));
        assert_eq!(dom_tree.get_idom(2), Some(0));
        assert_eq!(dom_tree.get_idom(3), Some(0));
    }

    #[test]
    fn test_dominance_frontier() {
        let cfg = create_diamond_cfg();
        let Ok(dom_tree) = DominatorTree::compute(&cfg) else {
            panic!("dominator tree computation should succeed")
        };

        // Block 3 should be in the dominance frontier of blocks 1 and 2
        let df1 = dom_tree.get_dominance_frontier(1);
        let df2 = dom_tree.get_dominance_frontier(2);

        assert!(df1.contains(&3));
        assert!(df2.contains(&3));
    }
}
