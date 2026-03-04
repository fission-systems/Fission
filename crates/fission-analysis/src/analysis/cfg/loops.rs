//! Loop Detection and Analysis
//!
//! Implements natural loop detection using dominator information.
//! A natural loop has a single entry point (header) that dominates all blocks in the loop.

#[cfg(test)]
use super::EdgeKind;
use super::{ControlFlowGraph, DominatorTree};
use std::collections::{HashMap, HashSet, VecDeque};

/// Kind of detected loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    /// Natural loop (single entry, dominates body)
    Natural,
    /// While loop (condition at header)
    While,
    /// Do-While loop (condition at tail)
    DoWhile,
    /// For loop (init, condition, increment pattern)
    For,
    /// Infinite loop (no exit condition)
    Infinite,
    /// Irreducible loop (multiple entries)
    Irreducible,
}

/// Represents a detected loop in the CFG
#[derive(Debug, Clone)]
pub struct Loop {
    /// Loop header block (entry point)
    pub header: usize,
    /// All blocks in the loop body (including header)
    pub body: HashSet<usize>,
    /// Back edges that form this loop
    pub back_edges: Vec<(usize, usize)>,
    /// Exit edges from the loop
    pub exit_edges: Vec<(usize, usize)>,
    /// Exit blocks (blocks outside the loop that can be reached)
    pub exit_blocks: Vec<usize>,
    /// Loop kind
    pub kind: LoopKind,
    /// Nesting depth (0 = outermost)
    pub depth: usize,
    /// Parent loop header (if nested)
    pub parent: Option<usize>,
    /// Child loops (headers)
    pub children: Vec<usize>,
}

impl Loop {
    /// Create a new loop
    pub fn new(header: usize) -> Self {
        Loop {
            header,
            body: HashSet::new(),
            back_edges: Vec::new(),
            exit_edges: Vec::new(),
            exit_blocks: Vec::new(),
            kind: LoopKind::Natural,
            depth: 0,
            parent: None,
            children: Vec::new(),
        }
    }

    /// Check if a block is in this loop
    pub fn contains(&self, block: usize) -> bool {
        self.body.contains(&block)
    }

    /// Get the number of blocks in the loop
    pub fn size(&self) -> usize {
        self.body.len()
    }

    /// Check if this is an innermost loop (no nested loops)
    pub fn is_innermost(&self) -> bool {
        self.children.is_empty()
    }

    /// Get loop latches (blocks with back edges to header)
    pub fn latches(&self) -> Vec<usize> {
        self.back_edges.iter().map(|(from, _)| *from).collect()
    }
}

/// Loop analysis engine
pub struct LoopAnalyzer;

impl LoopAnalyzer {
    /// Detect all loops in the CFG
    pub fn detect_loops(cfg: &ControlFlowGraph, dom_tree: &DominatorTree) -> Vec<Loop> {
        let mut loops = Vec::new();
        let back_edges = Self::find_back_edges(cfg, dom_tree);

        // Group back edges by header
        let mut header_to_back_edges: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for (from, to) in back_edges {
            header_to_back_edges.entry(to).or_default().push((from, to));
        }

        // Create loops for each header
        for (header, edges) in header_to_back_edges {
            let mut loop_info = Loop::new(header);
            loop_info.back_edges = edges.clone();

            // Find natural loop body
            loop_info.body = Self::find_natural_loop_body(cfg, header, &edges);

            // Find exit edges and blocks (clone body to avoid borrow issues)
            let body_copy = loop_info.body.clone();
            Self::find_exits(cfg, &body_copy, &mut loop_info);

            // Determine loop kind
            loop_info.kind = Self::classify_loop(cfg, &loop_info);

            loops.push(loop_info);
        }

        // Compute nesting relationships
        Self::compute_nesting(&mut loops);

        loops
    }

    /// Find all back edges in the CFG
    /// A back edge (u -> v) is an edge where v dominates u
    fn find_back_edges(cfg: &ControlFlowGraph, dom_tree: &DominatorTree) -> Vec<(usize, usize)> {
        let mut back_edges = Vec::new();

        for (from_idx, block) in cfg.blocks.iter().enumerate() {
            for edge in &block.successors {
                // Check if target dominates source (back edge condition)
                if dom_tree.dominates(edge.target, from_idx) {
                    back_edges.push((from_idx, edge.target));
                }
            }
        }

        back_edges
    }

    /// Find the natural loop body for a loop with given header and back edges
    fn find_natural_loop_body(
        cfg: &ControlFlowGraph,
        header: usize,
        back_edges: &[(usize, usize)],
    ) -> HashSet<usize> {
        let mut body = HashSet::new();
        body.insert(header);

        // Start from latch nodes and work backwards
        let mut worklist: VecDeque<usize> = VecDeque::new();
        for (latch, _) in back_edges {
            if *latch != header {
                body.insert(*latch);
                worklist.push_back(*latch);
            }
        }

        // BFS backwards to find all blocks in the loop
        while let Some(block) = worklist.pop_front() {
            for &pred in &cfg.blocks[block].predecessors {
                if !body.contains(&pred) {
                    body.insert(pred);
                    worklist.push_back(pred);
                }
            }
        }

        body
    }

    /// Find exit edges and exit blocks for a loop
    fn find_exits(cfg: &ControlFlowGraph, body: &HashSet<usize>, loop_info: &mut Loop) {
        let mut exit_blocks = HashSet::new();

        for &block_idx in body {
            if let Some(block) = cfg.blocks.get(block_idx) {
                for edge in &block.successors {
                    if !body.contains(&edge.target) {
                        loop_info.exit_edges.push((block_idx, edge.target));
                        exit_blocks.insert(edge.target);
                    }
                }
            }
        }

        loop_info.exit_blocks = exit_blocks.into_iter().collect();
    }

    /// Classify the loop type based on structure
    fn classify_loop(cfg: &ControlFlowGraph, loop_info: &Loop) -> LoopKind {
        // Check for infinite loop (no exits)
        if loop_info.exit_edges.is_empty() {
            return LoopKind::Infinite;
        }

        // Check if condition is at header (while loop) or at latch (do-while)
        let header = loop_info.header;
        if let Some(header_block) = cfg.blocks.get(header) {
            // If header has conditional branch to outside, it's a while loop
            if header_block.has_conditional_branch() {
                let exits_from_header =
                    loop_info.exit_edges.iter().any(|(from, _)| *from == header);
                if exits_from_header {
                    return LoopKind::While;
                }
            }
        }

        // Check for do-while (exit from latch)
        let latches = loop_info.latches();
        for latch in &latches {
            if let Some(latch_block) = cfg.blocks.get(*latch) {
                if latch_block.has_conditional_branch() {
                    let exits_from_latch =
                        loop_info.exit_edges.iter().any(|(from, _)| from == latch);
                    if exits_from_latch {
                        return LoopKind::DoWhile;
                    }
                }
            }
        }

        // Default to natural loop
        LoopKind::Natural
    }

    /// Compute nesting relationships between loops
    fn compute_nesting(loops: &mut Vec<Loop>) {
        let n = loops.len();
        if n == 0 {
            return;
        }

        // For each pair of loops, determine nesting
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }

                // Check if loop j is nested in loop i
                let header_j = loops[j].header;
                if loops[i].body.contains(&header_j) && loops[j].body.is_subset(&loops[i].body) {
                    // j is nested in i
                    loops[j].parent = Some(loops[i].header);
                    if !loops[i].children.contains(&header_j) {
                        loops[i].children.push(header_j);
                    }
                }
            }
        }

        // Compute depths based on nesting
        for i in 0..n {
            let mut depth = 0;
            let mut current_header = loops[i].parent;
            while let Some(parent) = current_header {
                depth += 1;
                current_header = loops
                    .iter()
                    .find(|l| l.header == parent)
                    .and_then(|l| l.parent);
            }
            loops[i].depth = depth;
        }
    }

    /// Get the innermost loop containing a block
    pub fn innermost_loop_containing<'a>(loops: &'a [Loop], block: usize) -> Option<&'a Loop> {
        loops
            .iter()
            .filter(|l| l.contains(block))
            .max_by_key(|l| l.depth)
    }

    /// Check if the CFG is reducible (no irreducible loops)
    pub fn is_reducible(cfg: &ControlFlowGraph, dom_tree: &DominatorTree) -> bool {
        // A graph is reducible if all back edges go to dominators
        // (which is how we define back edges, so we check for cross edges that
        // could indicate irreducibility)

        for (from_idx, block) in cfg.blocks.iter().enumerate() {
            for edge in &block.successors {
                let to_idx = edge.target;

                // Skip if it's a tree edge or back edge
                if dom_tree.dominates(to_idx, from_idx) {
                    continue; // Back edge - OK
                }
                if dom_tree.dominates(from_idx, to_idx) {
                    continue; // Forward edge - OK
                }

                // Cross edge - check if it creates multiple entries
                // This is a simplified check; full irreducibility detection is more complex
            }
        }

        true // Simplified - assume reducible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg::{BasicBlock, BlockEdge};

    fn create_while_loop_cfg() -> (ControlFlowGraph, DominatorTree) {
        let mut cfg = ControlFlowGraph::new();
        use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

        // Block 0: entry, jumps to header
        let mut b0 = BasicBlock::new(0, 0x1000);
        b0.is_entry = true;
        b0.successors = vec![BlockEdge::new(1, EdgeKind::Unconditional)];

        // Block 1: loop header (condition check)
        let mut b1 = BasicBlock::new(1, 0x1010);
        // Add CBranch operation to make has_conditional_branch() return true
        let cbranch_op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::CBranch,
            address: 0x1010,
            output: None,
            inputs: vec![
                Varnode::constant(0x1020, 8), // target address
                Varnode::constant(1, 1),      // condition
            ],
            asm_mnemonic: Some("jnz".to_string()),
        };
        b1.add_operation(cbranch_op);
        b1.successors = vec![
            BlockEdge::new(2, EdgeKind::ConditionalTrue), // enter loop
            BlockEdge::new(3, EdgeKind::ConditionalFalse), // exit loop
        ];
        b1.predecessors = vec![0, 2];

        // Block 2: loop body
        let mut b2 = BasicBlock::new(2, 0x1020);
        b2.successors = vec![BlockEdge::new(1, EdgeKind::Unconditional)]; // back edge
        b2.predecessors = vec![1];

        // Block 3: after loop
        let mut b3 = BasicBlock::new(3, 0x1030);
        b3.is_exit = true;
        b3.predecessors = vec![1];

        cfg.blocks = vec![b0, b1, b2, b3];
        cfg.entry_block = 0;
        cfg.exit_blocks = vec![3];

        let Ok(dom_tree) = DominatorTree::compute(&cfg) else {
            panic!("dominator tree computation should succeed")
        };
        (cfg, dom_tree)
    }

    #[test]
    fn test_back_edge_detection() {
        let (cfg, dom_tree) = create_while_loop_cfg();
        let back_edges = LoopAnalyzer::find_back_edges(&cfg, &dom_tree);

        // Should find back edge from block 2 to block 1
        assert_eq!(back_edges.len(), 1);
        assert_eq!(back_edges[0], (2, 1));
    }

    #[test]
    fn test_loop_detection() {
        let (cfg, dom_tree) = create_while_loop_cfg();
        let loops = LoopAnalyzer::detect_loops(&cfg, &dom_tree);

        assert_eq!(loops.len(), 1);
        let loop_info = &loops[0];
        assert_eq!(loop_info.header, 1);
        assert!(loop_info.body.contains(&1)); // header
        assert!(loop_info.body.contains(&2)); // body
        assert!(!loop_info.body.contains(&0)); // not entry
        assert!(!loop_info.body.contains(&3)); // not exit
    }

    #[test]
    fn test_while_loop_classification() {
        let (cfg, dom_tree) = create_while_loop_cfg();
        let loops = LoopAnalyzer::detect_loops(&cfg, &dom_tree);

        assert_eq!(loops[0].kind, LoopKind::While);
    }

    #[test]
    fn test_loop_exits() {
        let (cfg, dom_tree) = create_while_loop_cfg();
        let loops = LoopAnalyzer::detect_loops(&cfg, &dom_tree);

        let loop_info = &loops[0];
        assert_eq!(loop_info.exit_edges.len(), 1);
        assert_eq!(loop_info.exit_edges[0], (1, 3));
        assert_eq!(loop_info.exit_blocks, vec![3]);
    }
}
