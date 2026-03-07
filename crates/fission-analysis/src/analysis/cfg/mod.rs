//! Control Flow Graph (CFG) Analysis Module
//!
//! Provides comprehensive CFG analysis capabilities:
//! - Basic block extraction
//! - CFG graph structure with edges
//! - Dominator tree computation
//! - Loop detection (natural loops, irreducible loops)
//! - Cyclomatic complexity calculation
//! - CFG visualization (Graphviz DOT format)

mod basic_block;
mod dominator;
mod graph;
mod loops;
mod metrics;
mod summary;
mod visualization;

pub use basic_block::{BasicBlock, BlockEdge, EdgeKind};
pub use dominator::DominatorTree;
use fission_pcode::PcodeFunction;
pub use graph::{CfgBuilder, ControlFlowGraph};
pub use loops::{Loop, LoopAnalyzer, LoopKind};
pub use metrics::{CfgMetrics, ComplexityAnalyzer};
pub use summary::*;
pub use visualization::{CfgVisualizer, DotOptions};

/// Error types for CFG analysis
#[derive(Debug, Clone)]
pub enum CfgError {
    /// No entry point found in the function
    NoEntryPoint,
    /// Invalid block index
    InvalidBlockIndex(usize),
    /// Invalid edge reference
    InvalidEdge(usize, usize),
    /// Graph is not reducible
    IrreducibleGraph,
    /// Analysis failed
    AnalysisFailed(String),
}

impl std::fmt::Display for CfgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CfgError::NoEntryPoint => write!(f, "No entry point found in function"),
            CfgError::InvalidBlockIndex(idx) => write!(f, "Invalid block index: {}", idx),
            CfgError::InvalidEdge(from, to) => write!(f, "Invalid edge: {} -> {}", from, to),
            CfgError::IrreducibleGraph => write!(f, "Graph contains irreducible loops"),
            CfgError::AnalysisFailed(msg) => write!(f, "CFG analysis failed: {}", msg),
        }
    }
}

impl std::error::Error for CfgError {}

/// Result type for CFG operations
pub type CfgResult<T> = Result<T, CfgError>;

/// Complete CFG analysis result containing all computed information
#[derive(Debug, Clone)]
pub struct CfgAnalysis {
    /// The control flow graph
    pub cfg: ControlFlowGraph,
    /// Dominator tree (if computed)
    pub dominator_tree: Option<DominatorTree>,
    /// Detected loops
    pub loops: Vec<Loop>,
    /// Computed metrics
    pub metrics: CfgMetrics,
}

impl CfgAnalysis {
    /// Build complete CFG analysis from a Pcode function
    pub fn from_pcode(func: &PcodeFunction) -> CfgResult<Self> {
        // Build CFG
        let cfg = CfgBuilder::from_pcode(func)?;

        // Compute dominator tree
        let dominator_tree = DominatorTree::compute(&cfg).ok();

        // Detect loops
        let loops = if let Some(ref dom_tree) = dominator_tree {
            LoopAnalyzer::detect_loops(&cfg, dom_tree)
        } else {
            Vec::new()
        };

        // Compute metrics
        let metrics = ComplexityAnalyzer::compute(&cfg, &loops);

        Ok(CfgAnalysis {
            cfg,
            dominator_tree,
            loops,
            metrics,
        })
    }

    /// Generate DOT visualization
    pub fn to_dot(&self, options: &DotOptions) -> String {
        CfgVisualizer::to_dot(&self.cfg, &self.loops, options)
    }

    /// Generate summary report
    pub fn summary(&self) -> String {
        let mut report = String::new();

        report.push_str(&format!("=== CFG Analysis Summary ===\n"));
        report.push_str(&format!("Basic Blocks: {}\n", self.cfg.blocks.len()));
        report.push_str(&format!("Edges: {}\n", self.cfg.edge_count()));
        report.push_str(&format!("Entry Block: {}\n", self.cfg.entry_block));
        report.push_str(&format!("Exit Blocks: {:?}\n", self.cfg.exit_blocks));
        report.push_str(&format!("\n"));

        report.push_str(&format!("=== Metrics ===\n"));
        report.push_str(&format!(
            "Cyclomatic Complexity: {}\n",
            self.metrics.cyclomatic_complexity
        ));
        report.push_str(&format!(
            "Max Nesting Depth: {}\n",
            self.metrics.max_nesting_depth
        ));
        report.push_str(&format!("Number of Loops: {}\n", self.loops.len()));

        if !self.loops.is_empty() {
            report.push_str(&format!("\n=== Detected Loops ===\n"));
            for (i, loop_info) in self.loops.iter().enumerate() {
                report.push_str(&format!(
                    "Loop {}: Header={}, Kind={:?}, Blocks={:?}\n",
                    i, loop_info.header, loop_info.kind, loop_info.body
                ));
            }
        }

        if let Some(ref dom_tree) = self.dominator_tree {
            report.push_str(&format!("\n=== Dominator Tree ===\n"));
            report.push_str(&dom_tree.to_string());
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_yaml_snapshot;

    #[test]
    fn test_cfg_summary_snapshot() {
        let summary = CfgSummary {
            function_address: "0x1000".into(),
            block_count: 3,
            edge_count: 2,
            cyclomatic_complexity: 1,
            max_nesting_depth: 0,
            loops: vec![],
            blocks: vec![
                BlockSummary {
                    index: 0,
                    address: "0x1000".into(),
                    is_entry: true,
                    is_exit: false,
                    successors: vec![1],
                    predecessors: vec![],
                    instruction_count: 5,
                },
                BlockSummary {
                    index: 1,
                    address: "0x1010".into(),
                    is_entry: false,
                    is_exit: false,
                    successors: vec![2],
                    predecessors: vec![0],
                    instruction_count: 2,
                },
                BlockSummary {
                    index: 2,
                    address: "0x1020".into(),
                    is_entry: false,
                    is_exit: true,
                    successors: vec![],
                    predecessors: vec![1],
                    instruction_count: 1,
                },
            ],
            dot_content: Some("digraph { }".into()),
        };

        assert_yaml_snapshot!(summary);
    }

    #[test]
    fn test_cfg_error_display() {
        let err = CfgError::InvalidBlockIndex(5);
        assert!(err.to_string().contains("5"));
    }
}
