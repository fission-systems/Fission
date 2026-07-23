//! Blocking helpers for the GUI — binary load, decompile, and CFG extraction.
//!
//! All Fission core APIs are synchronous; these helpers wrap them so they can
//! be called from `tokio::task::spawn_blocking` without blocking the UI thread.

use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh_with_facts};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::Path;
use std::sync::Arc;

// ── Load ────────────────────────────────────────────────────────────────────

pub struct LoadResult {
    pub binary: Arc<LoadedBinary>,
    pub functions: Vec<FunctionInfo>,
    pub summary: String,
}

/// Load a binary from disk (blocking).
pub fn load_binary_blocking(path: &Path) -> Result<LoadResult, String> {
    let binary =
        LoadedBinary::from_file(path).map_err(|e| format!("Load failed: {e}"))?;

    let mut functions = binary.functions.clone();
    functions.sort_by_key(|f| f.address);

    let summary = format!(
        "{} | {} | {} functions | entry 0x{:x}",
        binary.format,
        if binary.is_64bit { "64-bit" } else { "32-bit" },
        functions.len(),
        binary.entry_point,
    );

    Ok(LoadResult { binary: Arc::new(binary), functions, summary })
}

// ── CFG data types ───────────────────────────────────────────────────────────

/// Lightweight GUI representation of a CFG edge kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgEdgeKind {
    Unconditional,
    ConditionalTrue,
    ConditionalFalse,
    Fallthrough,
    Return,
    Indirect,
}

impl CfgEdgeKind {
    /// Stroke colour for SVG rendering (matches design-token palette).
    pub fn svg_color(&self) -> &'static str {
        match self {
            Self::ConditionalTrue  => "#4ec97b",  // green
            Self::ConditionalFalse => "#f47067",  // red
            Self::Unconditional    => "#8d97a5",  // grey
            Self::Fallthrough      => "#6b7785",  // dimmer grey
            Self::Return           => "#c099ff",  // purple
            Self::Indirect         => "#ffb347",  // orange
        }
    }

    /// Short label shown on the edge.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ConditionalTrue  => "T",
            Self::ConditionalFalse => "F",
            Self::Unconditional    => "",
            Self::Fallthrough      => "",
            Self::Return           => "ret",
            Self::Indirect         => "ind",
        }
    }
}

/// A single basic block in the GUI CFG model.
#[derive(Debug, Clone)]
pub struct CfgNodeData {
    pub index:    usize,
    pub address:  u64,
    pub op_count: usize,
    pub is_entry: bool,
    pub is_exit:  bool,
}

impl CfgNodeData {
    /// Display label (short hex address).
    pub fn label(&self) -> String {
        format!("0x{:x}", self.address)
    }

    /// Node height in SVG units, proportional to op_count (min 36, max 72).
    pub fn node_height(&self) -> f64 {
        let h = 36.0 + (self.op_count as f64).min(8.0) * 4.5;
        h.min(72.0)
    }
}

/// A directed edge between two nodes.
#[derive(Debug, Clone)]
pub struct CfgEdgeData {
    pub from:    usize,
    pub to:      usize,
    pub kind:    CfgEdgeKind,
    pub is_back: bool,  // loop back-edge (rendered as curve)
}

/// Full GUI CFG model derived from decompile evidence.
#[derive(Debug, Clone, Default)]
pub struct CfgGraphData {
    pub nodes:               Vec<CfgNodeData>,
    pub edges:               Vec<CfgEdgeData>,
    pub cyclomatic:          usize,
    pub block_count:         usize,
    pub edge_count:          usize,
}

impl CfgGraphData {
    /// Build from the pipeline evidence captured during decompilation.
    pub fn from_evidence(
        evidence: &fission_decompiler::RustSleighPipelineEvidence,
    ) -> Option<Self> {
        let blocks = &evidence.raw_pcode_blocks;
        if blocks.is_empty() {
            return None;
        }

        let n = blocks.len();

        // ── DFS to find back edges ────────────────────────────────────────
        let mut visited   = vec![false; n];
        let mut in_stack  = vec![false; n];
        let mut back_set  = std::collections::HashSet::new();

        // adjacency from evidence
        let adj: Vec<Vec<usize>> = blocks
            .iter()
            .map(|b| b.successors.iter().map(|&s| s as usize).filter(|&s| s < n).collect())
            .collect();

        let mut stack: Vec<(usize, usize)> = vec![(0, 0)]; // (node, child_iter_idx)
        visited[0]   = true;
        in_stack[0]  = true;

        while let Some((u, ci)) = stack.last_mut() {
            let u = *u;
            if *ci < adj[u].len() {
                let v = adj[u][*ci];
                *ci += 1;
                if in_stack[v] {
                    back_set.insert((u, v));
                } else if !visited[v] {
                    visited[v]  = true;
                    in_stack[v] = true;
                    stack.push((v, 0));
                }
            } else {
                in_stack[u] = false;
                stack.pop();
            }
        }

        // ── Build nodes ───────────────────────────────────────────────────
        let nodes: Vec<CfgNodeData> = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let is_exit = b
                    .terminal_opcode
                    .as_deref()
                    .map_or(false, |op| op.contains("Return") || op.contains("BranchInd"));
                CfgNodeData {
                    index:    i,
                    address:  b.start_address,
                    op_count: b.op_count,
                    is_entry: i == 0,
                    is_exit,
                }
            })
            .collect();

        // ── Build edges ───────────────────────────────────────────────────
        let mut edges = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            let succs = &block.successors;
            let term  = block.terminal_opcode.as_deref();

            let is_cbranch = term.map_or(false, |t| t.contains("CBranch"));
            let is_branch  = term.map_or(false, |t| t.contains("Branch") && !t.contains("CBranch") && !t.contains("BranchInd"));
            let is_ret     = term.map_or(false, |t| t.contains("Return"));
            let is_ind     = term.map_or(false, |t| t.contains("BranchInd"));

            if is_ret { continue; }

            for (si, &raw_succ) in succs.iter().enumerate() {
                let to = raw_succ as usize;
                if to >= n { continue; }
                let is_back = back_set.contains(&(i, to));

                let kind = if is_cbranch {
                    if si == 0 { CfgEdgeKind::ConditionalTrue } else { CfgEdgeKind::ConditionalFalse }
                } else if is_branch {
                    CfgEdgeKind::Unconditional
                } else if is_ind {
                    CfgEdgeKind::Indirect
                } else {
                    CfgEdgeKind::Fallthrough
                };

                edges.push(CfgEdgeData { from: i, to, kind, is_back });
            }
        }

        let e = edges.len();
        let cyclomatic = e.saturating_sub(n) + 2;

        Some(CfgGraphData {
            nodes,
            edges,
            cyclomatic,
            block_count: n,
            edge_count:  e,
        })
    }
}

// ── Decompile ───────────────────────────────────────────────────────────────

pub struct DecompileOutput {
    pub code:            String,
    pub code_nir:        Option<String>,
    pub fell_back:       bool,
    pub fallback_reason: Option<String>,
    pub cfg:             Option<CfgGraphData>,
}

/// Decompile a single function (blocking).
pub fn decompile_blocking(
    binary: &Arc<LoadedBinary>,
    addr:   u64,
    name:   &str,
) -> Result<DecompileOutput, String> {
    let facts = FactStore::from_binary(binary.as_ref());

    let mut config = RustSleighDecompileConfig::cli_defaults();
    config.nir_timeout_ms = Some(10_000);

    let result = decompile_with_rust_sleigh_with_facts(
        binary.as_ref(),
        &facts,
        addr,
        name,
        &config,
        None,
        None,
    )?;

    let cfg = CfgGraphData::from_evidence(&result.evidence);

    Ok(DecompileOutput {
        code:            result.code,
        code_nir:        result.code_nir,
        fell_back:       result.fell_back,
        fallback_reason: result.fallback_reason,
        cfg,
    })
}
