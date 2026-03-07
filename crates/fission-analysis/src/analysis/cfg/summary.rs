//! CFG Summary structures for UI and CLI display

use super::{CfgAnalysis, CfgVisualizer, DotOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CfgSummary {
    pub function_address: String,
    pub block_count: usize,
    pub edge_count: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting_depth: usize,
    pub loops: Vec<LoopSummary>,
    pub blocks: Vec<BlockSummary>,
    pub dot_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSummary {
    pub header: usize,
    pub kind: String,
    pub body: Vec<usize>,
    pub back_edges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSummary {
    pub index: usize,
    pub address: String,
    pub is_entry: bool,
    pub is_exit: bool,
    pub successors: Vec<usize>,
    pub predecessors: Vec<usize>,
    pub instruction_count: usize,
}

impl CfgSummary {
    pub fn from_analysis(analysis: &CfgAnalysis, addr: Option<u64>, include_dot: bool) -> Self {
        let loops = analysis
            .loops
            .iter()
            .map(|l| LoopSummary {
                header: l.header,
                kind: format!("{:?}", l.kind),
                body: l.body.iter().copied().collect(),
                back_edges: l.back_edges.clone(),
            })
            .collect();

        let blocks = analysis
            .cfg
            .blocks
            .iter()
            .map(|b| BlockSummary {
                index: b.index,
                address: format!("0x{:x}", b.start_address),
                is_entry: b.is_entry,
                is_exit: b.is_exit,
                successors: b.successors.iter().map(|e| e.target).collect(),
                predecessors: b.predecessors.clone(),
                instruction_count: b.operations.len(),
            })
            .collect();

        let dot_content = if include_dot {
            let options = DotOptions {
                show_instructions: true,
                show_addresses: true,
                highlight_loops: true,
                ..Default::default()
            };
            Some(CfgVisualizer::to_dot(
                &analysis.cfg,
                &analysis.loops,
                &options,
            ))
        } else {
            None
        };

        Self {
            function_address: addr.map(|a| format!("0x{:x}", a)).unwrap_or_default(),
            block_count: analysis.cfg.block_count(),
            edge_count: analysis.cfg.edge_count(),
            cyclomatic_complexity: analysis.metrics.cyclomatic_complexity,
            max_nesting_depth: analysis.metrics.max_nesting_depth,
            loops,
            blocks,
            dot_content,
        }
    }
}
