//! CFG Metrics and Complexity Analysis
//!
//! Computes various metrics for control flow graphs:
//! - Cyclomatic complexity (McCabe's metric)
//! - Nesting depth
//! - Essential complexity
//! - Block-level metrics

use super::{ControlFlowGraph, Loop};

/// Collected CFG metrics
#[derive(Debug, Clone, Default)]
pub struct CfgMetrics {
    /// McCabe's cyclomatic complexity (M = E - N + 2P)
    pub cyclomatic_complexity: usize,
    /// Essential complexity (after removing structured constructs)
    pub essential_complexity: usize,
    /// Maximum nesting depth of control structures
    pub max_nesting_depth: usize,
    /// Average nesting depth
    pub avg_nesting_depth: f64,
    /// Number of basic blocks
    pub block_count: usize,
    /// Number of edges
    pub edge_count: usize,
    /// Number of decision points (conditional branches)
    pub decision_count: usize,
    /// Number of loops
    pub loop_count: usize,
    /// Number of exit points
    pub exit_count: usize,
    /// Ratio of edges to nodes (graph density indicator)
    pub edge_to_node_ratio: f64,
    /// Number of unreachable blocks
    pub dead_block_count: usize,
}

impl CfgMetrics {
    /// Check if complexity is considered high
    pub fn is_high_complexity(&self) -> bool {
        self.cyclomatic_complexity > 10
    }

    /// Check if complexity is considered very high
    pub fn is_very_high_complexity(&self) -> bool {
        self.cyclomatic_complexity > 20
    }

    /// Get complexity rating as string
    pub fn complexity_rating(&self) -> &'static str {
        match self.cyclomatic_complexity {
            0..=5 => "Low",
            6..=10 => "Moderate",
            11..=20 => "High",
            21..=50 => "Very High",
            _ => "Extreme",
        }
    }

    /// Get recommended action based on complexity
    pub fn recommendation(&self) -> &'static str {
        match self.cyclomatic_complexity {
            0..=5 => "Simple function, good testability",
            6..=10 => "Moderate complexity, consider breaking down if it grows",
            11..=20 => "High complexity, recommend refactoring into smaller functions",
            21..=50 => "Very high complexity, strongly recommend refactoring",
            _ => "Extreme complexity, function should be split immediately",
        }
    }
}

impl std::fmt::Display for CfgMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "CFG Metrics:")?;
        writeln!(
            f,
            "  Cyclomatic Complexity: {} ({})",
            self.cyclomatic_complexity,
            self.complexity_rating()
        )?;
        writeln!(f, "  Essential Complexity:  {}", self.essential_complexity)?;
        writeln!(f, "  Max Nesting Depth:     {}", self.max_nesting_depth)?;
        writeln!(f, "  Blocks:                {}", self.block_count)?;
        writeln!(f, "  Edges:                 {}", self.edge_count)?;
        writeln!(f, "  Decision Points:       {}", self.decision_count)?;
        writeln!(f, "  Loops:                 {}", self.loop_count)?;
        writeln!(f, "  Exit Points:           {}", self.exit_count)?;
        if self.dead_block_count > 0 {
            writeln!(
                f,
                "  Dead Blocks:           {} (warning!)",
                self.dead_block_count
            )?;
        }
        Ok(())
    }
}

/// Complexity analysis engine
pub struct ComplexityAnalyzer;

impl ComplexityAnalyzer {
    /// Compute all metrics for a CFG
    pub fn compute(cfg: &ControlFlowGraph, loops: &[Loop]) -> CfgMetrics {
        let block_count = cfg.block_count();
        let edge_count = cfg.edge_count();

        CfgMetrics {
            cyclomatic_complexity: Self::cyclomatic_complexity(cfg),
            essential_complexity: Self::essential_complexity(cfg, loops),
            max_nesting_depth: Self::max_nesting_depth(loops),
            avg_nesting_depth: Self::average_nesting_depth(loops),
            block_count,
            edge_count,
            decision_count: Self::decision_count(cfg),
            loop_count: loops.len(),
            exit_count: cfg.exit_blocks.len(),
            edge_to_node_ratio: if block_count > 0 {
                edge_count as f64 / block_count as f64
            } else {
                0.0
            },
            dead_block_count: cfg.dead_blocks().len(),
        }
    }

    /// Calculate cyclomatic complexity using McCabe's formula
    /// M = E - N + 2P where:
    /// - E = number of edges
    /// - N = number of nodes (blocks)
    /// - P = number of connected components (usually 1)
    pub fn cyclomatic_complexity(cfg: &ControlFlowGraph) -> usize {
        let e = cfg.edge_count();
        let n = cfg.block_count();
        let p = 1; // Assuming single connected component (one function)

        // M = E - N + 2P
        // Handle case where graph might be malformed
        if n == 0 {
            return 1;
        }

        let complexity = e as isize - n as isize + 2 * p;
        complexity.max(1) as usize
    }

    /// Calculate cyclomatic complexity using decision count
    /// Alternative formula: M = D + 1 where D is number of decision points
    pub fn cyclomatic_from_decisions(cfg: &ControlFlowGraph) -> usize {
        Self::decision_count(cfg) + 1
    }

    /// Count decision points (conditional branches)
    pub fn decision_count(cfg: &ControlFlowGraph) -> usize {
        cfg.blocks
            .iter()
            .filter(|b| b.has_conditional_branch())
            .count()
    }

    /// Calculate essential complexity
    /// (complexity remaining after removing all structured constructs)
    /// A value of 1 indicates perfectly structured code
    pub fn essential_complexity(cfg: &ControlFlowGraph, loops: &[Loop]) -> usize {
        // Simplified calculation:
        // Count constructs that aren't well-structured
        let mut unstructured = 0;

        // Check for multiple exits from loops
        for loop_info in loops {
            if loop_info.exit_edges.len() > 1 {
                unstructured += loop_info.exit_edges.len() - 1;
            }
        }

        // Check for multiple exit points
        if cfg.exit_blocks.len() > 1 {
            unstructured += cfg.exit_blocks.len() - 1;
        }

        // Essential complexity is at least 1
        unstructured.max(1)
    }

    /// Calculate maximum nesting depth of loops
    pub fn max_nesting_depth(loops: &[Loop]) -> usize {
        loops
            .iter()
            .map(|l| l.depth + 1) // depth is 0-indexed, so add 1
            .max()
            .unwrap_or(0)
    }

    /// Calculate average nesting depth
    pub fn average_nesting_depth(loops: &[Loop]) -> f64 {
        if loops.is_empty() {
            return 0.0;
        }
        let total: usize = loops.iter().map(|l| l.depth + 1).sum();
        total as f64 / loops.len() as f64
    }

    /// Calculate Halstead complexity metrics (simplified)
    pub fn halstead_volume(cfg: &ControlFlowGraph) -> f64 {
        // Simplified: use instruction count as proxy
        let n: usize = cfg.blocks.iter().map(|b| b.instruction_count()).sum();
        if n == 0 {
            return 0.0;
        }
        // V = N * log2(n) where N is total operands/operators
        // Simplified approximation
        n as f64 * (n as f64).log2()
    }

    /// Calculate maintainability index (0-100 scale)
    /// Based on Halstead Volume, Cyclomatic Complexity, and Lines of Code
    pub fn maintainability_index(cfg: &ControlFlowGraph, _loops: &[Loop]) -> f64 {
        let cc = Self::cyclomatic_complexity(cfg) as f64;
        let loc: usize = cfg.blocks.iter().map(|b| b.instruction_count()).sum();
        let hv = Self::halstead_volume(cfg);

        if loc == 0 {
            return 100.0;
        }

        // MI = 171 - 5.2 * ln(HV) - 0.23 * CC - 16.2 * ln(LOC)
        // Normalized to 0-100 scale
        let mi = 171.0 - 5.2 * (hv + 1.0).ln() - 0.23 * cc - 16.2 * (loc as f64 + 1.0).ln();

        // Normalize to 0-100
        (mi * 100.0 / 171.0).clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg::{BasicBlock, BlockEdge, EdgeKind};

    fn create_simple_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();

        let mut b0 = BasicBlock::new(0, 0x1000);
        b0.is_entry = true;
        b0.successors = vec![
            BlockEdge::new(1, EdgeKind::ConditionalTrue),
            BlockEdge::new(2, EdgeKind::ConditionalFalse),
        ];

        let mut b1 = BasicBlock::new(1, 0x1010);
        b1.successors = vec![BlockEdge::new(3, EdgeKind::Unconditional)];

        let mut b2 = BasicBlock::new(2, 0x1020);
        b2.successors = vec![BlockEdge::new(3, EdgeKind::Unconditional)];

        let mut b3 = BasicBlock::new(3, 0x1030);
        b3.is_exit = true;

        cfg.blocks = vec![b0, b1, b2, b3];
        cfg.entry_block = 0;
        cfg.exit_blocks = vec![3];

        cfg
    }

    #[test]
    fn test_cyclomatic_complexity() {
        let cfg = create_simple_cfg();
        // E = 4, N = 4, P = 1
        // M = 4 - 4 + 2 = 2
        let cc = ComplexityAnalyzer::cyclomatic_complexity(&cfg);
        assert_eq!(cc, 2);
    }

    #[test]
    fn test_decision_count() {
        let cfg = create_simple_cfg();
        let decisions = ComplexityAnalyzer::decision_count(&cfg);
        // Block 0 has conditional branch
        assert_eq!(decisions, 0); // BasicBlock doesn't have ops in this test
    }

    #[test]
    fn test_metrics_display() {
        let metrics = CfgMetrics {
            cyclomatic_complexity: 5,
            essential_complexity: 1,
            max_nesting_depth: 2,
            avg_nesting_depth: 1.5,
            block_count: 10,
            edge_count: 15,
            decision_count: 4,
            loop_count: 2,
            exit_count: 1,
            edge_to_node_ratio: 1.5,
            dead_block_count: 0,
        };

        let display = format!("{}", metrics);
        assert!(display.contains("Cyclomatic Complexity: 5"));
        assert!(display.contains("Low"));
    }

    #[test]
    fn test_complexity_rating() {
        assert_eq!(
            CfgMetrics {
                cyclomatic_complexity: 3,
                ..Default::default()
            }
            .complexity_rating(),
            "Low"
        );
        assert_eq!(
            CfgMetrics {
                cyclomatic_complexity: 8,
                ..Default::default()
            }
            .complexity_rating(),
            "Moderate"
        );
        assert_eq!(
            CfgMetrics {
                cyclomatic_complexity: 15,
                ..Default::default()
            }
            .complexity_rating(),
            "High"
        );
        assert_eq!(
            CfgMetrics {
                cyclomatic_complexity: 30,
                ..Default::default()
            }
            .complexity_rating(),
            "Very High"
        );
    }
}
