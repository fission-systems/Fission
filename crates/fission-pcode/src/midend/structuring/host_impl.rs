//! `StructuringHost` implementation for `PreviewBuilder`.

use super::*;
use fission_midend_structuring::StructuringHost;
use fission_midend_structuring::cfg_analysis::{CfgFactCache, DomTree};
use fission_midend_structuring::loop_analysis::LoopBody;

impl<'a> StructuringHost for PreviewBuilder<'a> {
    fn successors(&self) -> &[Vec<usize>] {
        &self.successors
    }
    fn predecessors(&self) -> &[Vec<usize>] {
        &self.predecessors
    }
    fn successors_mut(&mut self) -> &mut Vec<Vec<usize>> {
        &mut self.successors
    }
    fn predecessors_mut(&mut self) -> &mut Vec<Vec<usize>> {
        &mut self.predecessors
    }
    fn block_count(&self) -> usize {
        self.pcode.blocks.len() + self.virtual_block_map.len()
    }
    fn cfg_facts(&self) -> &CfgFactCache {
        &self.cfg_facts
    }
    fn cfg_facts_mut(&mut self) -> &mut CfgFactCache {
        &mut self.cfg_facts
    }
    fn refresh_cfg_fact_cache(&mut self) {
        let facts = CfgFactCache::analyze(&self.successors, &self.predecessors);
        self.dom_tree = facts.dominators().clone();
        self.cfg_facts = facts;
    }
    fn dom_tree(&self) -> &DomTree {
        &self.dom_tree
    }
    fn set_dom_tree(&mut self, tree: DomTree) {
        self.dom_tree = tree;
    }
    fn fas_virtual_edges(&self) -> &[(usize, usize)] {
        &self.fas_virtual_edges
    }
    fn fas_virtual_edges_mut(&mut self) -> &mut Vec<(usize, usize)> {
        &mut self.fas_virtual_edges
    }
    fn irreducible_edges(&self) -> &std::collections::HashSet<(usize, usize)> {
        &self.irreducible_edges
    }
    fn irreducible_edges_mut(&mut self) -> &mut std::collections::HashSet<(usize, usize)> {
        &mut self.irreducible_edges
    }
    fn virtual_block_map(&self) -> &[usize] {
        &self.virtual_block_map
    }
    fn loop_bodies(&self) -> &[LoopBody] {
        &self.loop_bodies
    }
    fn set_loop_bodies(&mut self, bodies: Vec<LoopBody>) {
        self.loop_bodies = bodies;
    }
    fn follow_blocks(&self) -> &[Option<usize>] {
        &self.follow_blocks
    }
    fn set_follow_blocks(&mut self, blocks: Vec<Option<usize>>) {
        self.follow_blocks = blocks;
    }
    fn active_switch_targets(&self) -> &std::collections::HashSet<usize> {
        &self.active_switch_targets
    }
    fn active_switch_targets_mut(&mut self) -> &mut std::collections::HashSet<usize> {
        &mut self.active_switch_targets
    }
    fn options(&self) -> &MlilPreviewOptions {
        self.options
    }
    fn function_entry_address(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|b| b.start_address)
            .unwrap_or(0)
    }
    fn current_function_name(&self) -> Option<&str> {
        self.current_function_name.as_deref()
    }
    fn structuring_start(&self) -> Option<std::time::Instant> {
        self.structuring_start
    }
    fn set_structuring_start(&mut self, t: Option<std::time::Instant>) {
        self.structuring_start = t;
    }
    fn block_target_key(&self, idx: usize) -> u64 {
        PreviewBuilder::block_target_key(self, idx)
    }
    fn block_start_address(&self, idx: usize) -> u64 {
        PreviewBuilder::block_start_address(self, idx)
    }
    fn find_block_index_by_address(&self, address: u64) -> Option<usize> {
        PreviewBuilder::find_block_index_by_address(self, address)
    }
    fn next_block_address(&self, idx: usize) -> Option<u64> {
        PreviewBuilder::next_block_address(self, idx)
    }
    fn fallthrough_index(&self, idx: usize) -> Option<usize> {
        PreviewBuilder::fallthrough_index(self, idx)
    }
    fn pcode_block_idx(&self, idx: usize) -> usize {
        PreviewBuilder::pcode_block_idx(self, idx)
    }
    fn lower_block_stmts(&mut self, block_idx: usize) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let pcode_idx = PreviewBuilder::pcode_block_idx(self, block_idx);
        // Index into blocks without holding a borrow across the mutable lower call.
        let block_ptr = self.pcode.blocks.as_ptr();
        // SAFETY: pcode is immutable for the lifetime of PreviewBuilder; we only
        // reborrow a block by index for the duration of lower_block_stmts.
        let block = unsafe { &*block_ptr.add(pcode_idx) };
        PreviewBuilder::lower_block_stmts(self, block)
    }
    fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        join_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        PreviewBuilder::lower_return_join_expr_for_predecessor(self, pred_idx, join_idx)
    }
    fn bump_region_proof_candidate(&mut self) {
        self.telemetry.structuring.region_proof_candidate_count += 1;
    }
    fn bump_guarded_tail_candidate(&mut self) {
        self.telemetry.structuring.guarded_tail_candidate_count += 1;
    }
    fn bump_promotion_rejected_by_shape(&mut self) {
        self.telemetry.structuring.promotion_rejected_by_shape_count += 1;
    }
    fn bump_promotion_rejected_by_gate(&mut self) {
        self.telemetry.structuring.promotion_rejected_by_gate_count += 1;
    }
    fn bump_region_emit_ready_failed(&mut self) {
        self.telemetry.structuring.region_emit_ready_failed_count += 1;
    }
    fn emit_ready_trace_enabled(&self) -> bool {
        PreviewBuilder::emit_ready_trace_enabled_for_current_fn(self)
    }
    fn emit_ready_trace(&self, message: &str) {
        PreviewBuilder::emit_ready_trace(self, message);
    }
    fn guarded_tail_trace_enabled(&self) -> bool {
        PreviewBuilder::guarded_tail_trace_enabled_for_current_fn(self)
    }
    fn log_try_lower_if_reject(&self, idx: usize, reason: &str) {
        let addr = PreviewBuilder::block_start_address(self, idx);
        PreviewBuilder::log_try_lower_if_reject(self, true, idx, addr, reason);
    }
}
