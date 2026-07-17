//! Host trait for CFG structuring free functions.
//!
//! Structuring algorithms that need builder state take `&mut impl StructuringHost`
//! instead of living as `PreviewBuilder` methods. The production host is
//! `PreviewBuilder` in `fission-pcode`.
//!
//! # Layers
//!
//! | Layer | Examples |
//! |-------|----------|
//! | CFG facts | successors, predecessors, `CfgFactCache` |
//! | Identity | block target keys, addresses |
//! | Lowering hooks | `lower_block_stmts` (HIR only, no p-code types) |
//! | Telemetry | bump helpers |
//! | Diagnostics | optional traces |
//!
//! Pure helpers that only touch `HirStmt`/`HirExpr` do **not** use this trait.

use crate::cfg_analysis::{CfgFactCache, DomTree, SccAnalysis};
use crate::loop_analysis::LoopBody;
use fission_midend_core::ir::{HirExpr, HirStmt, MlilPreviewError, MlilPreviewOptions};
use std::collections::HashSet;

/// Context required by free-function structuring algorithms.
pub trait StructuringHost {
    // ── CFG graph ──────────────────────────────────────────────────────────
    fn successors(&self) -> &[Vec<usize>];
    fn predecessors(&self) -> &[Vec<usize>];
    fn successors_mut(&mut self) -> &mut Vec<Vec<usize>>;
    fn predecessors_mut(&mut self) -> &mut Vec<Vec<usize>>;
    fn block_count(&self) -> usize;
    fn cfg_facts(&self) -> &CfgFactCache;
    fn cfg_facts_mut(&mut self) -> &mut CfgFactCache;
    fn refresh_cfg_fact_cache(&mut self);
    fn dom_tree(&self) -> &DomTree;
    fn set_dom_tree(&mut self, tree: DomTree);
    fn fas_virtual_edges(&self) -> &[(usize, usize)];
    fn fas_virtual_edges_mut(&mut self) -> &mut Vec<(usize, usize)>;
    fn irreducible_edges(&self) -> &HashSet<(usize, usize)>;
    fn irreducible_edges_mut(&mut self) -> &mut HashSet<(usize, usize)>;
    fn virtual_block_map(&self) -> &[usize];
    fn loop_bodies(&self) -> &[LoopBody];
    fn set_loop_bodies(&mut self, bodies: Vec<LoopBody>);
    fn follow_blocks(&self) -> &[Option<usize>];
    fn set_follow_blocks(&mut self, blocks: Vec<Option<usize>>);
    fn active_switch_targets(&self) -> &HashSet<usize>;
    fn active_switch_targets_mut(&mut self) -> &mut HashSet<usize>;

    // ── Options / identity ─────────────────────────────────────────────────
    fn options(&self) -> &MlilPreviewOptions;
    fn function_entry_address(&self) -> u64;
    fn current_function_name(&self) -> Option<&str>;
    fn structuring_start(&self) -> Option<std::time::Instant>;
    fn set_structuring_start(&mut self, t: Option<std::time::Instant>);

    // ── Address / block identity ───────────────────────────────────────────
    fn block_target_key(&self, idx: usize) -> u64;
    fn block_start_address(&self, idx: usize) -> u64;
    fn find_block_index_by_address(&self, address: u64) -> Option<usize>;
    fn next_block_address(&self, idx: usize) -> Option<u64>;
    fn fallthrough_index(&self, idx: usize) -> Option<usize>;
    fn pcode_block_idx(&self, idx: usize) -> usize;

    // ── Lowering hooks (HIR-only surface; p-code stays in the host) ────────
    fn lower_block_stmts(&mut self, block_idx: usize) -> Result<Vec<HirStmt>, MlilPreviewError>;
    fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        join_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError>;

    // ── Telemetry ──────────────────────────────────────────────────────────
    fn bump_region_proof_candidate(&mut self);
    fn bump_guarded_tail_candidate(&mut self);
    fn bump_promotion_rejected_by_shape(&mut self);
    fn bump_promotion_rejected_by_gate(&mut self);
    fn bump_region_emit_ready_failed(&mut self);

    // ── Diagnostics ────────────────────────────────────────────────────────
    fn emit_ready_trace_enabled(&self) -> bool;
    fn emit_ready_trace(&self, message: &str);
    fn guarded_tail_trace_enabled(&self) -> bool;
    fn log_try_lower_if_reject(&self, idx: usize, reason: &str);

    // ── Derived CFG helpers ────────────────────────────────────────────────
    fn analyze_cfg_scc(&self) -> SccAnalysis {
        self.cfg_facts().scc().clone()
    }
    fn analyze_cfg_dominators(&self) -> DomTree {
        self.cfg_facts().dominators().clone()
    }
}
