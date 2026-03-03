//! Control Flow Graph structure and builder
//!
//! Implements the CFG data structure with blocks and edges,
//! and provides methods to build it from Pcode functions.

use super::{BasicBlock, BlockEdge, CfgError, CfgResult, EdgeKind};
use fission_pcode::{PcodeFunction, PcodeOp, PcodeOpcode};
use std::collections::{HashMap, HashSet, VecDeque};

/// Control Flow Graph representation
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// All basic blocks in the CFG
    pub blocks: Vec<BasicBlock>,
    /// Entry block index
    pub entry_block: usize,
    /// Exit block indices (blocks with return statements)
    pub exit_blocks: Vec<usize>,
    /// Function name (if available)
    pub function_name: Option<String>,
    /// Function start address
    pub function_address: u64,
}

impl ControlFlowGraph {
    /// Create a new empty CFG
    pub fn new() -> Self {
        ControlFlowGraph {
            blocks: Vec::new(),
            entry_block: 0,
            exit_blocks: Vec::new(),
            function_name: None,
            function_address: 0,
        }
    }

    /// Get a block by index
    pub fn get_block(&self, index: usize) -> Option<&BasicBlock> {
        self.blocks.get(index)
    }

    /// Get a mutable block by index
    pub fn get_block_mut(&mut self, index: usize) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(index)
    }

    /// Get the entry block
    pub fn entry(&self) -> Option<&BasicBlock> {
        self.blocks.get(self.entry_block)
    }

    /// Get number of blocks
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get total number of edges
    pub fn edge_count(&self) -> usize {
        self.blocks.iter().map(|b| b.successors.len()).sum()
    }

    /// Get all successors of a block
    pub fn successors(&self, block_idx: usize) -> Vec<usize> {
        self.blocks
            .get(block_idx)
            .map(|b| b.successors.iter().map(|e| e.target).collect())
            .unwrap_or_default()
    }

    /// Get all predecessors of a block
    pub fn predecessors(&self, block_idx: usize) -> Vec<usize> {
        self.blocks
            .get(block_idx)
            .map(|b| b.predecessors.clone())
            .unwrap_or_default()
    }

    /// Check if there's an edge from source to target
    pub fn has_edge(&self, source: usize, target: usize) -> bool {
        self.blocks
            .get(source)
            .map(|b| b.successors.iter().any(|e| e.target == target))
            .unwrap_or(false)
    }

    /// Get edge between two blocks
    pub fn get_edge(&self, source: usize, target: usize) -> Option<&BlockEdge> {
        self.blocks
            .get(source)
            .and_then(|b| b.successors.iter().find(|e| e.target == target))
    }

    /// Perform depth-first traversal
    pub fn dfs_preorder(&self) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        let mut stack = vec![self.entry_block];

        while let Some(block_idx) = stack.pop() {
            if visited.contains(&block_idx) {
                continue;
            }
            visited.insert(block_idx);
            order.push(block_idx);

            if let Some(block) = self.blocks.get(block_idx) {
                // Add successors in reverse order for correct DFS order
                for edge in block.successors.iter().rev() {
                    if !visited.contains(&edge.target) {
                        stack.push(edge.target);
                    }
                }
            }
        }

        order
    }

    /// Perform breadth-first traversal
    pub fn bfs(&self) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.entry_block);

        while let Some(block_idx) = queue.pop_front() {
            if visited.contains(&block_idx) {
                continue;
            }
            visited.insert(block_idx);
            order.push(block_idx);

            if let Some(block) = self.blocks.get(block_idx) {
                for edge in &block.successors {
                    if !visited.contains(&edge.target) {
                        queue.push_back(edge.target);
                    }
                }
            }
        }

        order
    }

    /// Compute reverse post-order (topological order for acyclic parts)
    pub fn reverse_postorder(&self) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut postorder = Vec::new();

        fn dfs_postorder(
            cfg: &ControlFlowGraph,
            block_idx: usize,
            visited: &mut HashSet<usize>,
            postorder: &mut Vec<usize>,
        ) {
            if visited.contains(&block_idx) {
                return;
            }
            visited.insert(block_idx);

            if let Some(block) = cfg.blocks.get(block_idx) {
                for edge in &block.successors {
                    dfs_postorder(cfg, edge.target, visited, postorder);
                }
            }
            postorder.push(block_idx);
        }

        dfs_postorder(self, self.entry_block, &mut visited, &mut postorder);
        postorder.reverse();
        postorder
    }

    /// Find all blocks reachable from the entry
    pub fn reachable_blocks(&self) -> HashSet<usize> {
        self.dfs_preorder().into_iter().collect()
    }

    /// Find unreachable (dead) blocks
    pub fn dead_blocks(&self) -> Vec<usize> {
        let reachable = self.reachable_blocks();
        (0..self.blocks.len())
            .filter(|idx| !reachable.contains(idx))
            .collect()
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing CFG from Pcode
pub struct CfgBuilder;

impl CfgBuilder {
    /// Build CFG from a Pcode function
    pub fn from_pcode(func: &PcodeFunction) -> CfgResult<ControlFlowGraph> {
        if func.blocks.is_empty() {
            return Err(CfgError::NoEntryPoint);
        }

        let mut cfg = ControlFlowGraph::new();

        // Build address to block index mapping
        let mut addr_to_block: HashMap<u64, usize> = HashMap::new();

        // Create basic blocks
        for (idx, pcode_block) in func.blocks.iter().enumerate() {
            let mut block = BasicBlock::new(idx, pcode_block.start_address);

            // Copy operations
            for op in &pcode_block.ops {
                block.add_operation(op.clone());
            }

            // Check if this is an exit block
            if block.has_return() {
                block.is_exit = true;
            }

            addr_to_block.insert(pcode_block.start_address, idx);
            cfg.blocks.push(block);
        }

        // Set entry block
        if !cfg.blocks.is_empty() {
            cfg.blocks[0].is_entry = true;
            cfg.entry_block = 0;

            // Get function address from first block
            cfg.function_address = cfg.blocks[0].start_address;
        }

        // Find exit blocks
        cfg.exit_blocks = cfg
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_exit)
            .map(|(idx, _)| idx)
            .collect();

        // Build edges based on control flow operations
        Self::build_edges(&mut cfg, &addr_to_block)?;

        // Build predecessor lists
        Self::build_predecessors(&mut cfg);

        Ok(cfg)
    }

    /// Build edges between blocks
    fn build_edges(
        cfg: &mut ControlFlowGraph,
        addr_to_block: &HashMap<u64, usize>,
    ) -> CfgResult<()> {
        let block_count = cfg.blocks.len();

        for idx in 0..block_count {
            let mut edges = Vec::new();

            // Analyze the last operations to determine edges
            let block = &cfg.blocks[idx];
            let has_branch = block.has_unconditional_branch();
            let has_cbranch = block.has_conditional_branch();
            let has_return = block.has_return();

            // Find branch targets from operations
            for op in &block.operations {
                match op.opcode {
                    PcodeOpcode::Branch => {
                        // Unconditional branch - target is first input
                        if let Some(target_addr) = Self::get_branch_target(op) {
                            if let Some(&target_idx) = addr_to_block.get(&target_addr) {
                                edges.push(BlockEdge::with_address(
                                    target_idx,
                                    EdgeKind::Unconditional,
                                    target_addr,
                                ));
                            }
                        }
                    }
                    PcodeOpcode::CBranch => {
                        // Conditional branch - has true and false paths
                        if let Some(target_addr) = Self::get_branch_target(op) {
                            if let Some(&target_idx) = addr_to_block.get(&target_addr) {
                                edges.push(BlockEdge::with_address(
                                    target_idx,
                                    EdgeKind::ConditionalTrue,
                                    target_addr,
                                ));
                            }
                        }
                        // Fallthrough for false case
                        if idx + 1 < block_count {
                            edges.push(BlockEdge::new(idx + 1, EdgeKind::ConditionalFalse));
                        }
                    }
                    PcodeOpcode::Call | PcodeOpcode::CallInd => {
                        // After call, fallthrough to next block (unless tail call)
                        if idx + 1 < block_count && !has_return {
                            // Only add fallthrough if not already added
                            let next_idx = idx + 1;
                            if !edges.iter().any(|e| e.target == next_idx) {
                                edges.push(BlockEdge::new(next_idx, EdgeKind::Fallthrough));
                            }
                        }
                    }
                    PcodeOpcode::Return | PcodeOpcode::BranchInd => {
                        // No successor edges for return or indirect branch (computed)
                    }
                    _ => {}
                }
            }

            // Add fallthrough edge if block doesn't end with unconditional control flow
            if !has_branch && !has_return && !has_cbranch {
                if idx + 1 < block_count {
                    let next_idx = idx + 1;
                    if !edges.iter().any(|e| e.target == next_idx) {
                        edges.push(BlockEdge::new(next_idx, EdgeKind::Fallthrough));
                    }
                }
            }

            cfg.blocks[idx].successors = edges;
        }

        Ok(())
    }

    /// Get branch target address from a branch operation
    fn get_branch_target(op: &PcodeOp) -> Option<u64> {
        // The branch target is typically the first input
        op.inputs.first().and_then(|input| {
            if input.is_constant {
                Some(input.offset)
            } else {
                None
            }
        })
    }

    /// Build predecessor lists from successor edges
    fn build_predecessors(cfg: &mut ControlFlowGraph) {
        // Clear existing predecessors
        for block in &mut cfg.blocks {
            block.predecessors.clear();
        }

        // Build predecessor lists
        for idx in 0..cfg.blocks.len() {
            let successors: Vec<usize> = cfg.blocks[idx]
                .successors
                .iter()
                .map(|e| e.target)
                .collect();

            for succ_idx in successors {
                if succ_idx < cfg.blocks.len() {
                    cfg.blocks[succ_idx].predecessors.push(idx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();

        // Block 0: entry
        let mut b0 = BasicBlock::new(0, 0x1000);
        b0.is_entry = true;
        b0.successors
            .push(BlockEdge::new(1, EdgeKind::ConditionalTrue));
        b0.successors
            .push(BlockEdge::new(2, EdgeKind::ConditionalFalse));

        // Block 1: true branch
        let mut b1 = BasicBlock::new(1, 0x1010);
        b1.successors
            .push(BlockEdge::new(3, EdgeKind::Unconditional));

        // Block 2: false branch
        let mut b2 = BasicBlock::new(2, 0x1020);
        b2.successors
            .push(BlockEdge::new(3, EdgeKind::Unconditional));

        // Block 3: merge/exit
        let mut b3 = BasicBlock::new(3, 0x1030);
        b3.is_exit = true;

        cfg.blocks = vec![b0, b1, b2, b3];
        cfg.entry_block = 0;
        cfg.exit_blocks = vec![3];

        // Build predecessors
        cfg.blocks[1].predecessors = vec![0];
        cfg.blocks[2].predecessors = vec![0];
        cfg.blocks[3].predecessors = vec![1, 2];

        cfg
    }

    #[test]
    fn test_cfg_creation() {
        let cfg = create_simple_cfg();
        assert_eq!(cfg.block_count(), 4);
        assert_eq!(cfg.edge_count(), 4);
        assert_eq!(cfg.entry_block, 0);
        assert_eq!(cfg.exit_blocks, vec![3]);
    }

    #[test]
    fn test_dfs_traversal() {
        let cfg = create_simple_cfg();
        let order = cfg.dfs_preorder();
        assert_eq!(order[0], 0); // Entry first
        assert!(order.contains(&1));
        assert!(order.contains(&2));
        assert!(order.contains(&3));
    }

    #[test]
    fn test_bfs_traversal() {
        let cfg = create_simple_cfg();
        let order = cfg.bfs();
        assert_eq!(order[0], 0); // Entry first
        // Blocks 1 and 2 should come before 3
        let Some(idx_1) = order.iter().position(|&x| x == 1) else {
            panic!("block 1 should be present in BFS order")
        };
        let Some(idx_2) = order.iter().position(|&x| x == 2) else {
            panic!("block 2 should be present in BFS order")
        };
        let Some(idx_3) = order.iter().position(|&x| x == 3) else {
            panic!("block 3 should be present in BFS order")
        };
        assert!(idx_1 < idx_3);
        assert!(idx_2 < idx_3);
    }

    #[test]
    fn test_successors_predecessors() {
        let cfg = create_simple_cfg();
        assert_eq!(cfg.successors(0), vec![1, 2]);
        assert_eq!(cfg.predecessors(3), vec![1, 2]);
        assert!(cfg.has_edge(0, 1));
        assert!(!cfg.has_edge(1, 0));
    }
}
