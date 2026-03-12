//! Basic Block definitions for CFG
//!
//! A basic block is a maximal sequence of instructions with:
//! - Single entry point (only the first instruction can be jumped to)
//! - Single exit point (only the last instruction can cause a branch)

use fission_pcode::{PcodeOp, PcodeOpcode};

/// Represents a basic block in the CFG
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique block index
    pub index: usize,
    /// Start address of the block
    pub start_address: u64,
    /// End address of the block (address after last instruction)
    pub end_address: u64,
    /// Pcode operations in this block
    pub operations: Vec<PcodeOp>,
    /// Outgoing edges from this block
    pub successors: Vec<BlockEdge>,
    /// Incoming edges to this block
    pub predecessors: Vec<usize>,
    /// Whether this block is the entry block
    pub is_entry: bool,
    /// Whether this block is an exit block (contains return)
    pub is_exit: bool,
}

impl BasicBlock {
    /// Create a new basic block
    pub fn new(index: usize, start_address: u64) -> Self {
        BasicBlock {
            index,
            start_address,
            end_address: start_address,
            operations: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            is_entry: false,
            is_exit: false,
        }
    }

    /// Add an operation to this block
    pub fn add_operation(&mut self, op: PcodeOp) {
        if let Some(addr) = op.address.checked_add(1) {
            if addr > self.end_address {
                self.end_address = addr;
            }
        }
        self.operations.push(op);
    }

    /// Get the terminator instruction (last instruction that affects control flow)
    pub fn terminator(&self) -> Option<&PcodeOp> {
        self.operations
            .iter()
            .rev()
            .find(|op| op.opcode.is_control_flow())
    }

    /// Check if block ends with a conditional branch
    pub fn has_conditional_branch(&self) -> bool {
        self.operations
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::CBranch))
    }

    /// Check if block ends with an unconditional branch
    pub fn has_unconditional_branch(&self) -> bool {
        self.operations
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::Branch))
    }

    /// Check if block ends with a call
    pub fn has_call(&self) -> bool {
        self.operations
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd))
    }

    /// Check if block ends with a return
    pub fn has_return(&self) -> bool {
        self.operations
            .iter()
            .any(|op| matches!(op.opcode, PcodeOpcode::Return))
    }

    /// Get number of instructions in the block
    pub fn instruction_count(&self) -> usize {
        self.operations.len()
    }

    /// Get the block label (for visualization)
    pub fn label(&self) -> String {
        format!("BB{}", self.index)
    }

    /// Get detailed label with address
    pub fn detailed_label(&self) -> String {
        format!("BB{} @ 0x{:x}", self.index, self.start_address)
    }
}

/// Edge kind in the CFG
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Unconditional jump
    Unconditional,
    /// Conditional branch taken (true branch)
    ConditionalTrue,
    /// Conditional branch not taken (false branch / fallthrough)
    ConditionalFalse,
    /// Fallthrough to next block
    Fallthrough,
    /// Function call (interprocedural)
    Call,
    /// Return from function
    Return,
    /// Exception handling edge
    Exception,
    /// Back edge (loop)
    BackEdge,
}

impl EdgeKind {
    /// Check if this edge completes a loop
    pub fn is_back_edge(&self) -> bool {
        matches!(self, EdgeKind::BackEdge)
    }

    /// Get color for visualization
    pub fn color(&self) -> &'static str {
        match self {
            EdgeKind::Unconditional => "black",
            EdgeKind::ConditionalTrue => "green",
            EdgeKind::ConditionalFalse => "red",
            EdgeKind::Fallthrough => "gray",
            EdgeKind::Call => "blue",
            EdgeKind::Return => "purple",
            EdgeKind::Exception => "orange",
            EdgeKind::BackEdge => "brown",
        }
    }

    /// Get edge style for visualization
    pub fn style(&self) -> &'static str {
        match self {
            EdgeKind::BackEdge => "dashed",
            EdgeKind::Fallthrough => "dotted",
            _ => "solid",
        }
    }

    /// Get edge label for visualization
    pub fn label(&self) -> &'static str {
        match self {
            EdgeKind::Unconditional => "",
            EdgeKind::ConditionalTrue => "T",
            EdgeKind::ConditionalFalse => "F",
            EdgeKind::Fallthrough => "fall",
            EdgeKind::Call => "call",
            EdgeKind::Return => "ret",
            EdgeKind::Exception => "exc",
            EdgeKind::BackEdge => "back",
        }
    }
}

/// Represents an edge in the CFG
#[derive(Debug, Clone)]
pub struct BlockEdge {
    /// Target block index
    pub target: usize,
    /// Type of edge
    pub kind: EdgeKind,
    /// Optional target address (for indirect jumps)
    pub target_address: Option<u64>,
}

impl BlockEdge {
    /// Create a new edge
    pub fn new(target: usize, kind: EdgeKind) -> Self {
        BlockEdge {
            target,
            kind,
            target_address: None,
        }
    }

    /// Create a new edge with target address
    pub fn with_address(target: usize, kind: EdgeKind, address: u64) -> Self {
        BlockEdge {
            target,
            kind,
            target_address: Some(address),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_block_creation() {
        let block = BasicBlock::new(0, 0x1000);
        assert_eq!(block.index, 0);
        assert_eq!(block.start_address, 0x1000);
        assert!(!block.is_entry);
        assert!(!block.is_exit);
    }

    #[test]
    fn test_edge_kind_properties() {
        assert!(EdgeKind::BackEdge.is_back_edge());
        assert!(!EdgeKind::Unconditional.is_back_edge());
        assert_eq!(EdgeKind::ConditionalTrue.label(), "T");
        assert_eq!(EdgeKind::ConditionalFalse.label(), "F");
    }
}
