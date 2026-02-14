//! CFG Visualization using Graphviz DOT format
//!
//! Generates DOT graphs for visualizing control flow graphs
//! with support for loop highlighting, dominators, and custom styling.

use super::{BasicBlock, ControlFlowGraph, EdgeKind, Loop};
use std::collections::HashSet;

/// Options for DOT visualization
#[derive(Debug, Clone)]
pub struct DotOptions {
    /// Include instruction details in blocks
    pub show_instructions: bool,
    /// Include addresses in labels
    pub show_addresses: bool,
    /// Highlight loop structures
    pub highlight_loops: bool,
    /// Show edge labels
    pub show_edge_labels: bool,
    /// Use horizontal layout (left to right)
    pub horizontal: bool,
    /// Graph title
    pub title: Option<String>,
    /// Font name
    pub font: String,
    /// Font size
    pub font_size: usize,
    /// Show dominator edges
    pub show_dominators: bool,
}

impl Default for DotOptions {
    fn default() -> Self {
        DotOptions {
            show_instructions: false,
            show_addresses: true,
            highlight_loops: true,
            show_edge_labels: true,
            horizontal: false,
            title: None,
            font: "Courier".to_string(),
            font_size: 10,
            show_dominators: false,
        }
    }
}

impl DotOptions {
    /// Create options for detailed view
    pub fn detailed() -> Self {
        DotOptions {
            show_instructions: true,
            show_addresses: true,
            highlight_loops: true,
            show_edge_labels: true,
            ..Default::default()
        }
    }

    /// Create options for minimal view
    pub fn minimal() -> Self {
        DotOptions {
            show_instructions: false,
            show_addresses: false,
            highlight_loops: false,
            show_edge_labels: false,
            ..Default::default()
        }
    }
}

/// CFG Visualization generator
pub struct CfgVisualizer;

impl CfgVisualizer {
    /// Generate DOT format string for a CFG
    pub fn to_dot(cfg: &ControlFlowGraph, loops: &[Loop], options: &DotOptions) -> String {
        let mut dot = String::new();

        // Graph header
        dot.push_str("digraph CFG {\n");

        // Graph attributes
        let rankdir = if options.horizontal { "LR" } else { "TB" };
        dot.push_str(&format!("  rankdir={};\n", rankdir));
        dot.push_str(&format!("  fontname=\"{}\";\n", options.font));
        dot.push_str(&format!("  fontsize={};\n", options.font_size));
        dot.push_str("  node [shape=box, fontname=\"Courier\", fontsize=10];\n");
        dot.push_str("  edge [fontname=\"Courier\", fontsize=8];\n");

        // Title
        if let Some(ref title) = options.title {
            dot.push_str(&format!("  label=\"{}\";\n", Self::escape_dot(title)));
            dot.push_str("  labelloc=t;\n");
        }

        // Identify loop blocks for highlighting
        let loop_blocks: HashSet<usize> = if options.highlight_loops {
            loops.iter().flat_map(|l| l.body.iter().copied()).collect()
        } else {
            HashSet::new()
        };

        let loop_headers: HashSet<usize> = loops.iter().map(|l| l.header).collect();

        // Generate nodes
        for block in &cfg.blocks {
            let node_label = Self::format_block_label(block, options);
            let style = Self::get_node_style(block, &loop_blocks, &loop_headers, cfg);
            dot.push_str(&format!(
                "  BB{} [label=\"{}\"{}];\n",
                block.index,
                Self::escape_dot(&node_label),
                style
            ));
        }

        dot.push_str("\n");

        // Generate edges
        for block in &cfg.blocks {
            for edge in &block.successors {
                let edge_style = Self::get_edge_style(block.index, edge.target, &edge.kind, loops);
                let edge_label = if options.show_edge_labels {
                    edge.kind.label()
                } else {
                    ""
                };

                if edge_label.is_empty() {
                    dot.push_str(&format!(
                        "  BB{} -> BB{}{};\n",
                        block.index, edge.target, edge_style
                    ));
                } else {
                    dot.push_str(&format!(
                        "  BB{} -> BB{} [label=\"{}\"{}];\n",
                        block.index,
                        edge.target,
                        edge_label,
                        edge_style.replace("[", ", ").replace("]", "")
                    ));
                }
            }
        }

        // Add loop subgraphs for visual grouping
        if options.highlight_loops {
            for (i, loop_info) in loops.iter().enumerate() {
                dot.push_str(&format!("\n  subgraph cluster_loop_{} {{\n", i));
                dot.push_str(&format!(
                    "    label=\"Loop {} (header: BB{})\";\n",
                    i, loop_info.header
                ));
                dot.push_str("    style=dashed;\n");
                dot.push_str("    color=blue;\n");
                for &block_idx in &loop_info.body {
                    dot.push_str(&format!("    BB{};\n", block_idx));
                }
                dot.push_str("  }\n");
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Format block label based on options
    fn format_block_label(block: &BasicBlock, options: &DotOptions) -> String {
        let mut label = String::new();

        // Block identifier
        label.push_str(&format!("BB{}", block.index));

        // Address
        if options.show_addresses {
            label.push_str(&format!("\\n0x{:x}", block.start_address));
        }

        // Instructions
        if options.show_instructions && !block.operations.is_empty() {
            label.push_str("\\n---");
            for op in block.operations.iter().take(10) {
                if let Some(ref mnemonic) = op.asm_mnemonic {
                    label.push_str(&format!("\\n{}", mnemonic));
                } else {
                    label.push_str(&format!("\\n{:?}", op.opcode));
                }
            }
            if block.operations.len() > 10 {
                label.push_str(&format!("\\n... ({} more)", block.operations.len() - 10));
            }
        }

        label
    }

    /// Get node style based on block properties
    fn get_node_style(
        block: &BasicBlock,
        loop_blocks: &HashSet<usize>,
        loop_headers: &HashSet<usize>,
        _cfg: &ControlFlowGraph,
    ) -> String {
        let mut styles = Vec::new();

        // Entry block
        if block.is_entry {
            styles.push("fillcolor=lightgreen".to_string());
            styles.push("style=filled".to_string());
        }
        // Exit block
        else if block.is_exit {
            styles.push("fillcolor=lightcoral".to_string());
            styles.push("style=filled".to_string());
        }
        // Loop header
        else if loop_headers.contains(&block.index) {
            styles.push("fillcolor=lightyellow".to_string());
            styles.push("style=\"filled,bold\"".to_string());
            styles.push("penwidth=2".to_string());
        }
        // Loop body
        else if loop_blocks.contains(&block.index) {
            styles.push("fillcolor=lightyellow".to_string());
            styles.push("style=filled".to_string());
        }

        if styles.is_empty() {
            String::new()
        } else {
            format!(", {}", styles.join(", "))
        }
    }

    /// Get edge style based on edge kind
    fn get_edge_style(from: usize, to: usize, kind: &EdgeKind, loops: &[Loop]) -> String {
        let mut attrs = Vec::new();

        // Color based on edge kind
        attrs.push(format!("color={}", kind.color()));

        // Style based on edge kind
        let style = kind.style();
        if style != "solid" {
            attrs.push(format!("style={}", style));
        }

        // Check if this is a back edge
        let is_back_edge = loops
            .iter()
            .any(|l| l.back_edges.iter().any(|(f, t)| *f == from && *t == to));
        if is_back_edge {
            attrs.push("penwidth=2".to_string());
            attrs.push("constraint=false".to_string()); // Don't use for ranking
        }

        if attrs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", attrs.join(", "))
        }
    }

    /// Escape special characters for DOT format
    fn escape_dot(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "")
    }

    /// Generate DOT for a simple block listing (no edges)
    pub fn blocks_to_dot(cfg: &ControlFlowGraph) -> String {
        let mut dot = String::new();
        dot.push_str("digraph Blocks {\n");
        dot.push_str("  rankdir=TB;\n");
        dot.push_str("  node [shape=record];\n");

        for block in &cfg.blocks {
            let ops_str: String = block
                .operations
                .iter()
                .take(5)
                .map(|op| format!("{:?}", op.opcode))
                .collect::<Vec<_>>()
                .join("|");

            dot.push_str(&format!(
                "  BB{} [label=\"{{BB{} @ 0x{:x}|{}}}\"];\n",
                block.index, block.index, block.start_address, ops_str
            ));
        }

        dot.push_str("}\n");
        dot
    }

    /// Generate ASCII art representation of the CFG
    pub fn to_ascii(cfg: &ControlFlowGraph) -> String {
        let mut output = String::new();

        output.push_str("Control Flow Graph\n");
        output.push_str("==================\n\n");

        for block in &cfg.blocks {
            // Block header
            let marker = if block.is_entry {
                "[ENTRY]"
            } else if block.is_exit {
                "[EXIT]"
            } else {
                ""
            };

            output.push_str(&format!(
                "BB{} @ 0x{:x} {}\n",
                block.index, block.start_address, marker
            ));

            // Predecessors
            if !block.predecessors.is_empty() {
                let preds: Vec<String> = block
                    .predecessors
                    .iter()
                    .map(|p| format!("BB{}", p))
                    .collect();
                output.push_str(&format!("  <- {}\n", preds.join(", ")));
            }

            // Successors
            if !block.successors.is_empty() {
                let succs: Vec<String> = block
                    .successors
                    .iter()
                    .map(|e| format!("BB{} ({})", e.target, e.kind.label()))
                    .collect();
                output.push_str(&format!("  -> {}\n", succs.join(", ")));
            }

            output.push('\n');
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg::BlockEdge;

    fn create_test_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();

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
    fn test_dot_generation() {
        let cfg = create_test_cfg();
        let dot = CfgVisualizer::to_dot(&cfg, &[], &DotOptions::default());

        assert!(dot.contains("digraph CFG"));
        assert!(dot.contains("BB0"));
        assert!(dot.contains("BB1"));
        assert!(dot.contains("BB2"));
        assert!(dot.contains("BB3"));
        assert!(dot.contains("BB0 -> BB1"));
        assert!(dot.contains("BB0 -> BB2"));
    }

    #[test]
    fn test_ascii_generation() {
        let cfg = create_test_cfg();
        let ascii = CfgVisualizer::to_ascii(&cfg);

        assert!(ascii.contains("BB0"));
        assert!(ascii.contains("[ENTRY]"));
        assert!(ascii.contains("[EXIT]"));
    }

    #[test]
    fn test_escape_dot() {
        assert_eq!(CfgVisualizer::escape_dot("test\"quote"), "test\\\"quote");
        assert_eq!(CfgVisualizer::escape_dot("line1\nline2"), "line1\\nline2");
    }
}
