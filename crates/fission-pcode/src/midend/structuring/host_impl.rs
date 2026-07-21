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
    fn irreducible_edges(&self) -> &HashSet<(usize, usize)> {
        &self.irreducible_edges
    }
    fn irreducible_edges_mut(&mut self) -> &mut HashSet<(usize, usize)> {
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
    fn active_switch_targets(&self) -> &HashSet<usize> {
        &self.active_switch_targets
    }
    fn active_switch_targets_mut(&mut self) -> &mut HashSet<usize> {
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
    fn reset_sese_region_proof_budget(&mut self) {
        PreviewBuilder::reset_sese_region_proof_budget(self)
    }
    fn structuring_total_work_counter(&self) -> std::rc::Rc<std::cell::Cell<u64>> {
        self.structuring_total_work_units.clone()
    }
    fn reset_structuring_total_work_counter(&mut self) {
        self.structuring_total_work_units.set(0);
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
    fn lower_block_terminator(
        &mut self,
        block_idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError> {
        PreviewBuilder::lower_block_terminator(self, block_idx)
    }
    fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        join_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        PreviewBuilder::lower_return_join_expr_for_predecessor(self, pred_idx, join_idx)
    }
    fn lower_linear_body(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        fission_midend_structuring::lower_linear_body(self, start_idx, exit)
    }
    fn lower_linear_body_with_budget(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        fission_midend_structuring::lower_linear_body_with_budget(self, start_idx, exit, budget)
    }
    fn linear_exit(&mut self, idx: usize) -> Result<Option<LinearExit>, MlilPreviewError> {
        fission_midend_structuring::linear_exit(self, idx)
    }
    fn linear_exit_with_budget(
        &mut self,
        idx: usize,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        fission_midend_structuring::linear_exit_with_budget(self, idx, budget)
    }
    fn shared_linear_exit(
        &mut self,
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        fission_midend_structuring::shared_linear_exit(self, lhs_idx, rhs_idx)
    }
    fn has_linear_body_cache(&self, start_idx: usize, exit: LinearExit) -> bool {
        fission_midend_structuring::has_linear_body_cache(self, start_idx, exit)
    }
    fn linear_body_cache_get(
        &self,
        key: &LinearBodyCacheKey,
    ) -> Option<LinearBodyCachedOutcome> {
        self.linear_body_cache.get(key).cloned()
    }
    fn linear_body_cache_insert(
        &mut self,
        key: LinearBodyCacheKey,
        value: LinearBodyCachedOutcome,
    ) {
        self.linear_body_cache.insert(key, value);
    }
    fn linear_body_active_insert(&mut self, key: LinearBodyCacheKey) -> bool {
        self.active_linear_body_keys.insert(key)
    }
    fn linear_body_active_remove(&mut self, key: &LinearBodyCacheKey) {
        self.active_linear_body_keys.remove(key);
    }
    fn conditional_tail_active_insert(&mut self, key: ConditionalTailKey) -> bool {
        self.active_conditional_tail_keys.insert(key)
    }
    fn conditional_tail_active_remove(&mut self, key: &ConditionalTailKey) {
        self.active_conditional_tail_keys.remove(key);
    }
    fn linear_exit_cache_get(&self, idx: usize) -> Option<Option<LinearExit>> {
        self.linear_exit_cache.get(&idx).cloned()
    }
    fn linear_exit_cache_insert(&mut self, idx: usize, exit: Option<LinearExit>) {
        self.linear_exit_cache.insert(idx, exit);
    }
    fn can_inline_linear_successor(
        &self,
        from_idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool {
        fission_midend_structuring::can_inline_linear_successor(self, from_idx, next_idx, visited)
    }
    fn can_inline_linear_successor_for_region(
        &self,
        from_idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
        exit: LinearExit,
    ) -> bool {
        fission_midend_structuring::can_inline_linear_successor_for_region(
            self, from_idx, next_idx, visited, exit,
        )
    }
    fn is_trivial_forwarding_block(&self, idx: usize, next_idx: usize) -> bool {
        PreviewBuilder::is_trivial_forwarding_block(self, idx, next_idx)
    }
    fn is_trivial_linear_tail(&self, idx: usize) -> bool {
        PreviewBuilder::is_trivial_linear_tail(self, idx)
    }
    fn forwarding_block_defines_return_tail_live_in(&self, idx: usize, join_idx: usize) -> bool {
        PreviewBuilder::forwarding_block_defines_return_tail_live_in(self, idx, join_idx)
    }
    fn record_conditional_tail_mismatch_subtype(
        &mut self,
        subtype: fission_midend_structuring::ConditionalTailMismatchSubtype,
    ) {
        PreviewBuilder::record_conditional_tail_mismatch_subtype(self, subtype)
    }
    fn record_conditional_tail_mismatch_sample(
        &self,
        origin_idx: usize,
        true_idx: Option<usize>,
        false_idx: Option<usize>,
        exit: LinearExit,
        subtype: fission_midend_structuring::ConditionalTailMismatchSubtype,
        stage: &str,
    ) {
        PreviewBuilder::record_conditional_tail_mismatch_sample(
            self, origin_idx, true_idx, false_idx, exit, subtype, stage,
        )
    }
    fn bump_rule_block_if_no_exit(&mut self) {
        self.telemetry.structuring.rule_block_if_no_exit_count += 1;
        self.telemetry
            .structuring
            .rule_block_if_no_exit_accepted_count += 1;
    }
    fn shared_exit_for_indices(
        &mut self,
        indices: &[usize],
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        fission_midend_structuring::shared_exit_for_indices(self, indices)
    }
    fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError> {
        PreviewBuilder::collect_jump_targets(self)
    }
    fn lsda_landing_pad_labels(&self) -> HashSet<String> {
        let Some(binary) = self.binary else {
            return HashSet::default();
        };
        let Some(entry_address) = self.pcode.blocks.first().map(|block| block.start_address) else {
            return HashSet::default();
        };
        let Some(info) = binary.eh_lsda.get(&entry_address) else {
            return HashSet::default();
        };
        info.call_sites
            .iter()
            .filter_map(|call_site| call_site.landing_pad)
            .map(fission_midend_structuring::block_label)
            .collect()
    }
    fn accept_structured_region(
        &mut self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        PreviewBuilder::accept_structured_region(self, start_idx, skip_to, targeted)
    }
    fn sese_region_proof_budget_exceeded(&self) -> bool {
        PreviewBuilder::sese_region_proof_budget_exceeded(self)
    }
    fn region_has_external_entry(
        &self,
        region: &HashSet<usize>,
        header_idx: usize,
    ) -> bool {
        PreviewBuilder::region_has_external_entry(self, region, header_idx)
    }
    fn head_has_only_discardable_pure_ops(&self, block_idx: usize) -> bool {
        let pcode_idx = PreviewBuilder::pcode_block_idx(self, block_idx);
        let Some(block) = self.pcode.blocks.get(pcode_idx) else {
            return false;
        };
        PreviewBuilder::for_condition_head_has_only_discardable_pure_ops(block)
    }
    fn cached_terminator_branch_targets(&self, block_idx: usize) -> Option<Vec<u64>> {
        let term = self.terminator_cache.get(&block_idx)?;
        let mut out = Vec::new();
        match term {
            LoweredTerminator::Goto(t) | LoweredTerminator::Fallthrough(Some(t)) => {
                out.push(*t);
            }
            LoweredTerminator::Cond {
                true_target,
                false_target,
                ..
            } => {
                out.push(*true_target);
                if let Some(ft) = false_target {
                    out.push(*ft);
                }
            }
            LoweredTerminator::Switch {
                targets,
                default_target,
                ..
            } => {
                out.extend(targets.iter().copied());
                if let Some(dt) = default_target {
                    out.push(*dt);
                }
            }
            _ => {}
        }
        Some(out)
    }
    fn lower_linear_body_for_region_recovery_detailed(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError> {
        fission_midend_structuring::lower_linear_body_for_region_recovery_detailed(
            self, start_idx, exit, budget,
        )
    }
    fn record_region_body_lowering_reject_reason(&mut self, reason: LinearBodyRejectReason) {
        match reason {
            LinearBodyRejectReason::ConditionalTailExitMismatch => {
                self.telemetry.structuring.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count += 1;
            }
            LinearBodyRejectReason::SuccessorInlineRejected => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_successor_inline_rejected_count += 1;
            }
            LinearBodyRejectReason::RevisitCycle => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_revisit_cycle_count += 1;
            }
            LinearBodyRejectReason::UnsupportedTerminator
            | LinearBodyRejectReason::TargetIndexMissing
            | LinearBodyRejectReason::ExitMismatch
            | LinearBodyRejectReason::BudgetTripped => {
                self.telemetry
                    .structuring
                    .region_linearize_rejected_body_lowering_unsupported_terminator_count += 1;
            }
        }
    }
    fn bump_region_linearize_rejected_irreducible_cfg(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_rejected_irreducible_cfg_count += 1;
    }
    fn bump_region_linearize_rejected_non_structuring_failure(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_rejected_non_structuring_failure_count += 1;
    }
    fn bump_region_linearize_rejected_no_exit(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_rejected_no_exit_count += 1;
    }
    fn bump_region_linearize_rejected_body_lowering_failed(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_rejected_body_lowering_failed_count += 1;
    }
    fn bump_region_linearize_rejected_non_advancing(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_rejected_non_advancing_count += 1;
    }
    fn bump_region_linearize_structuring(&mut self) {
        self.telemetry
            .structuring
            .region_linearize_structuring_count += 1;
    }
    fn bump_region_proof_candidate(&mut self) {
        self.telemetry.structuring.region_proof_candidate_count += 1;
    }
    fn bump_region_proof_completed(&mut self) {
        self.telemetry.structuring.region_proof_completed_count += 1;
    }
    fn record_region_candidate(&mut self, proof: &fission_midend_structuring::regions::RegionProof) {
        PreviewBuilder::record_region_candidate_impl(self, proof)
    }
    fn record_selected_region(&mut self, node: &fission_midend_structuring::StructureNode) {
        PreviewBuilder::record_selected_region_impl(self, node)
    }

    fn bump_guarded_tail_candidate(&mut self) {
        self.telemetry.structuring.guarded_tail_candidate_count += 1;
    }
    fn bump_promotion_candidate(&mut self) {
        self.telemetry.structuring.promotion_candidate_count += 1;
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
    fn bump_fas_virtual_goto(&mut self) {
        self.telemetry.structuring.fas_virtual_goto_count += 1;
    }
    fn bump_select_bad_edge(&mut self) {
        self.telemetry.structuring.structuring_select_bad_edge_count += 1;
    }
    fn bump_condition_fold_and(&mut self, fold_count: usize) {
        self.telemetry.structuring.condition_fold_and_count += fold_count;
    }
    fn bump_condition_fold_or(&mut self, fold_count: usize) {
        self.telemetry.structuring.condition_fold_or_count += fold_count;
    }
    fn bump_condition_fold_rejected_side_effect(&mut self) {
        self.telemetry
            .structuring
            .condition_fold_rejected_side_effect += 1;
    }
    fn track_loop_control_rewrite_stats(
        &mut self,
        break_rewrites: usize,
        continue_rewrites: usize,
        skipped_nested_scope_count: usize,
    ) {
        self.telemetry.structuring.loop_control_rewrite_break_count += break_rewrites;
        self.telemetry
            .structuring
            .loop_control_rewrite_continue_count += continue_rewrites;
        self.telemetry
            .structuring
            .loop_control_rewrite_skipped_nested_scope_count += skipped_nested_scope_count;
    }
    fn bump_loop_control_explicit_reducer(&mut self) {
        self.telemetry
            .structuring
            .loop_control_explicit_reducer_count += 1;
    }
    fn bump_loop_while_subgraph_lowered(&mut self) {
        self.telemetry.structuring.loop_while_subgraph_lowered_count += 1;
    }
    fn bump_loop_multi_tail_dowhile_lowered(&mut self) {
        self.telemetry
            .structuring
            .loop_multi_tail_dowhile_lowered_count += 1;
    }
    fn bump_loop_for_lowered(&mut self) {
        self.telemetry.structuring.loop_for_lowered_count += 1;
    }
    fn bump_loop_multi_exit_break(&mut self) {
        self.telemetry.structuring.loop_multi_exit_break_count += 1;
    }
    fn bump_switch_fallthrough_detected(&mut self, count: usize) {
        self.telemetry.structuring.switch_fallthrough_detected_count += count;
    }
    fn bump_switch_emit_ready_failed(&mut self) {
        self.telemetry.dispatcher.switch_emit_ready_failed_count += 1;
        self.telemetry.structuring.region_proof_candidate_count += 1;
        self.telemetry.structuring.region_emit_ready_failed_count += 1;
    }
    fn bump_proof_payload_direct_emit(&mut self) {
        self.telemetry.dispatcher.proof_payload_direct_emit_count += 1;
    }
    fn bump_sese_child_localized_linear(&mut self) {
        self.telemetry.structuring.sese_child_localized_linear_count += 1;
    }
    fn invalidate_terminator_cache(&mut self, block_idx: usize) {
        self.terminator_cache.remove(&block_idx);
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
        if structuring_diag_enabled() {
            eprintln!(
                "[DIAG] try_lower_if {}: idx={} block=0x{:x}",
                reason, idx, addr
            );
        }
    }

    fn try_build_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<
        Result<
            fission_midend_structuring::guarded_tail::GuardedTailTrial,
            fission_midend_structuring::guarded_tail::GuardedTailWitnessRejection,
        >,
    > {
        fission_midend_structuring::guarded_tail::try_build_guarded_tail_trial(
            self, body, idx, referenced,
        )
    }
    fn verify_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &fission_midend_structuring::guarded_tail::GuardedTailTrial,
    ) -> fission_midend_structuring::guarded_tail::GuardedTailVerification {
        fission_midend_structuring::guarded_tail::verify_guarded_tail_trial(self, body, idx, trial)
    }
    fn build_guarded_tail_execution_plan(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &fission_midend_structuring::guarded_tail::GuardedTailTrial,
        verification: &fission_midend_structuring::guarded_tail::GuardedTailVerification,
    ) -> Result<
        fission_midend_structuring::guarded_tail::GuardedTailExecutionPlan,
        fission_midend_structuring::guarded_tail::GuardedTailExecutionRejection,
    > {
        fission_midend_structuring::guarded_tail::build_guarded_tail_execution_plan(
            self, body, idx, trial, verification,
        )
    }
    fn execute_guarded_tail_plan(
        &mut self,
        body: &mut Vec<HirStmt>,
        idx: usize,
        trial: fission_midend_structuring::guarded_tail::GuardedTailTrial,
        plan: fission_midend_structuring::guarded_tail::GuardedTailExecutionPlan,
        cond: HirExpr,
    ) {
        fission_midend_structuring::guarded_tail::execute_guarded_tail_plan(
            self, body, idx, trial, plan, cond,
        )
    }
    fn discover_guarded_tail_candidates_in_body(&mut self, body: &[HirStmt]) {
        fission_midend_structuring::guarded_tail::discover_guarded_tail_candidates_in_body(
            self, body,
        )
    }
    fn mark_promotion_shape_rejection(
        &mut self,
        reason: fission_midend_structuring::guarded_tail::PromotionShapeRejection,
    ) {
        PreviewBuilder::mark_promotion_shape_rejection(self, reason)
    }
    fn mark_promotion_gate_rejection(
        &mut self,
        reason: fission_midend_structuring::guarded_tail::PromotionGateRejection,
    ) {
        PreviewBuilder::mark_promotion_gate_rejection(self, reason)
    }
    fn mark_guarded_tail_execution_rejection(
        &mut self,
        reason: fission_midend_structuring::guarded_tail::GuardedTailExecutionRejection,
    ) {
        PreviewBuilder::mark_guarded_tail_execution_rejection(self, reason)
    }
    fn mark_guarded_tail_canonicalization_failure(
        &mut self,
        failure: fission_midend_structuring::guarded_tail::GuardedTailCanonicalizationFailure,
    ) {
        PreviewBuilder::mark_guarded_tail_canonicalization_failure(self, failure)
    }

    fn mark_alias_nonlocal_external_before(&mut self) {
        // Call inherent via distinct path (avoid trait UFCS recursion).
        self.telemetry
            .structuring
            .canonicalization_failed_alias_has_nonlocal_ref_external_before_count += 1;
    }
    fn mark_alias_nonlocal_from_external_sites(
        &mut self,
        external_top_level_before: usize,
        external_nested_before: usize,
        external_refs_after: usize,
    ) {
        if external_nested_before > 0 {
            self.telemetry
                .structuring
                .canonicalization_failed_alias_has_nonlocal_ref_nested_before_count += 1;
        } else if external_refs_after > 0 {
            self.telemetry
                .structuring
                .canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count += 1;
        } else if external_top_level_before > 0 {
            self.telemetry
                .structuring
                .canonicalization_failed_alias_has_nonlocal_ref_external_before_count += 1;
        }
    }
    fn resolve_terminal_join_target(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        target_label: &str,
        referenced: &HashMap<String, usize>,
    ) -> Option<(String, usize)> {
        // Inherent method has the same name; use an explicit free helper name.
        gt_resolve_terminal_join_target(self, body, anchor_idx, target_label, referenced)
    }
    fn mark_noncanonical_layout_rejection(&mut self) {
        self.telemetry
            .structuring
            .discovery_rejected_noncanonical_layout_count += 1;
        self.telemetry.structuring.promotion_rejected_by_shape_count += 1;
    }
    fn record_guarded_tail_blockgraph_proof(
        &mut self,
        candidate_idx: usize,
        witness: &fission_midend_structuring::guarded_tail::RegionShapeWitness,
        legality_reason: fission_midend_structuring::regions::BlockGraphLegalityReason,
    ) {
        gt_record_guarded_tail_blockgraph_proof(self, candidate_idx, witness, legality_reason)
    }
    fn guarded_tail_function_address(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or(0)
    }
    fn guarded_tail_trace_emit_snapshot(&self, prefix: &str, stmts: &[HirStmt], limit: usize) {
        let take_n = stmts.len().min(limit.max(1));
        for (idx, stmt) in stmts.iter().take(take_n).enumerate() {
            eprintln!("{prefix} [{idx:02}] {stmt:?}");
        }
        if stmts.len() > take_n {
            eprintln!(
                "{prefix} ... (truncated {} stmts)",
                stmts.len().saturating_sub(take_n)
            );
        }
    }
    fn bump_structuring_counter(
        &mut self,
        counter: fission_midend_structuring::guarded_tail::StructuringCounter,
        amount: usize,
    ) {
        use fission_midend_structuring::guarded_tail::StructuringCounter as C;
        let s = &mut self.telemetry.structuring;
        match counter {
            C::canonicalization_failed_alias_not_fallthrough_nested_after_label_count => {
                s.canonicalization_failed_alias_not_fallthrough_nested_after_label_count += amount;
            }
            C::canonicalization_failed_alias_not_fallthrough_top_level_after_label_count => {
                s.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
                    amount;
            }
            C::canonicalized_guarded_tail_shape_count => {
                s.canonicalized_guarded_tail_shape_count += amount;
            }
            C::canonicalized_interleaved_join_use_count => {
                s.canonicalized_interleaved_join_use_count += amount;
            }
            C::canonicalized_local_nonfallthrough_alias_count => {
                s.canonicalized_local_nonfallthrough_alias_count += amount;
            }
            C::discovery_seen_guarded_tail_like_shape_count => {
                s.discovery_seen_guarded_tail_like_shape_count += amount;
            }
            C::guarded_tail_candidate_count => s.guarded_tail_candidate_count += amount,
            C::guarded_tail_rejected_side_effectful_callee_count => {
                s.guarded_tail_rejected_side_effectful_callee_count += amount;
            }
            C::guarded_tail_exported_binding_count => {
                s.guarded_tail_exported_binding_count += amount;
            }
            C::guarded_tail_promoted_count => s.guarded_tail_promoted_count += amount,
            C::guarded_tail_replacement_plan_candidate_count => {
                s.guarded_tail_replacement_plan_candidate_count += amount;
            }
            C::guarded_tail_replacement_plan_completed_count => {
                s.guarded_tail_replacement_plan_completed_count += amount;
            }
            C::guarded_tail_replacement_plan_merge_created_count => {
                s.guarded_tail_replacement_plan_merge_created_count += amount;
            }
            C::guarded_tail_replacement_plan_rejected_missing_merge_count => {
                s.guarded_tail_replacement_plan_rejected_missing_merge_count += amount;
            }
            C::guarded_tail_replacement_plan_rejected_unstable_read_count => {
                s.guarded_tail_replacement_plan_rejected_unstable_read_count += amount;
            }
            C::guarded_tail_replacement_read_count => {
                s.guarded_tail_replacement_read_count += amount;
            }
            C::guarded_tail_replacement_read_rejected_nondominated_count => {
                s.guarded_tail_replacement_read_rejected_nondominated_count += amount;
            }
            C::guarded_tail_replacement_read_rejected_nonremovable_op_count => {
                s.guarded_tail_replacement_read_rejected_nonremovable_op_count += amount;
            }
            C::guarded_tail_replacement_read_rewritten_count => {
                s.guarded_tail_replacement_read_rewritten_count += amount;
            }
            C::promoted_region_count => s.promoted_region_count += amount,
            C::promotion_candidate_count => s.promotion_candidate_count += amount,
        }
    }
    fn alloc_temp_binding(
        &mut self,
        ty: fission_midend_core::ir::NirType,
        origin: Option<fission_midend_core::ir::NirBindingOrigin>,
    ) -> String {
        use fission_midend_core::ir::NirType;
        let prefix = match &ty {
            NirType::Bool => "bVar",
            NirType::Int {
                bits: 32,
                signed: true,
            } => "iVar",
            NirType::Int {
                bits: 32,
                signed: false,
            } => "uVar",
            _ => "xVar",
        };
        let name = format!("{prefix}{}", self.temp_next_id);
        self.temp_next_id = self.temp_next_id.saturating_add(1);
        self.temps.insert(
            name.clone(),
            fission_midend_core::ir::NirBinding {
                name: name.clone(),
                ty,
                surface_type_name: None,
                origin,
                initializer: None,
            },
        );
        name
    }
    fn find_earliest_owned_join_label_with_diag(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)> {
        fission_midend_structuring::guarded_tail::find_earliest_owned_join_label_with_diag(
            self,
            body,
            anchor_idx,
            terminal_label_idx,
            referenced,
            trace_enabled,
        )
    }

    fn suffix_call_uses_preview_unsafe_callee(&self, stmt: &HirStmt) -> Option<String> {
        let (target, _, _) = fission_midend_structuring::guarded_tail::pure_hir::suffix_call_expr(stmt)?;
        let summary = self.call_effect_summary_for_target(target);
        fission_midend_structuring::guarded_tail::suffix_window::preview_unsafe_callee_target(
            stmt,
            summary.as_ref(),
        )
    }

    fn trace_suffix_unknown_call_provenance(&self, stmt_idx: usize, stmt: &HirStmt) {
        fission_midend_structuring::guarded_tail::suffix_window::trace_suffix_unknown_call_provenance(
            self, stmt_idx, stmt,
        )
    }

    // ── Host residual data adapters (binary / type_context / inventory) ──
    // Pure logic lives in midend free-fns; these only read PreviewBuilder state.

    fn call_effect_summary_for_target(
        &self,
        target: &str,
    ) -> Option<fission_midend_core::ir::NirCallEffectSummary> {
        self.type_context
            .and_then(|ctx| ctx.call_effect_summaries.get(target))
            .cloned()
    }

    fn suffix_call_provenance_facts(
        &self,
        target: &str,
        target_addr: Option<u64>,
    ) -> fission_midend_structuring::guarded_tail::suffix_window::SuffixCallProvenanceFacts {
        use crate::midend::is_known_api_signature;
        let import = is_known_api_signature(target);
        let binary_function_present = target_addr.is_some_and(|addr| {
            self.binary
                .is_some_and(|binary| binary.functions.iter().any(|func| func.address == addr))
        });
        let target_ref = target_addr.and_then(|addr| {
            self.type_context
                .and_then(|ctx| ctx.call_target_refs.get(&addr))
        });
        let effect_summary = self.call_effect_summary_for_target(target);
        fission_midend_structuring::guarded_tail::suffix_window::SuffixCallProvenanceFacts {
            target_addr,
            import,
            binary_function_present,
            target_ref_present: target_ref.is_some(),
            target_ref_provenance: target_ref
                .map(|r| format!("{:?}", r.provenance))
                .unwrap_or_else(|| "None".to_string()),
            effect_summary,
        }
    }

    fn note_unsupported_terminator_emit(&mut self, block_addr: u64) {
        PreviewBuilder::record_unsupported_inventory_event(
            self,
            "build_linear_multiblock_unsupported_terminator",
            None,
            None,
            None,
            Some(block_addr),
            None,
            false,
            "hir_unsupported_emit",
        );
    }

    fn switch_recovery_cfg_admitted(&self, idx: usize) -> bool {
        fission_midend_structuring::switch_recovery_cfg_admitted(self, idx)
    }

    fn has_same_start_address_peer(&self, idx: usize) -> bool {
        let block_start = PreviewBuilder::block_start_address(self, idx);
        let pcode_idx = PreviewBuilder::pcode_block_idx(self, idx);
        self.pcode.blocks.iter().enumerate().any(|(peer_idx, block)| {
            peer_idx != pcode_idx && block.start_address == block_start
        })
    }

    fn emit_unsupported_control_surface(
        &mut self,
        evidence: fission_midend_core::ir::UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
    ) -> HirStmt {
        PreviewBuilder::emit_unsupported_control_surface(self, evidence, target_expr)
    }
}

/// Free helpers that call inherent PreviewBuilder methods without trait UFCS recursion.
fn gt_resolve_terminal_join_target(
    builder: &mut PreviewBuilder<'_>,
    body: &[HirStmt],
    anchor_idx: usize,
    target_label: &str,
    referenced: &HashMap<String, usize>,
) -> Option<(String, usize)> {
    builder.resolve_terminal_join_target_impl(body, anchor_idx, target_label, referenced)
}

fn gt_record_guarded_tail_blockgraph_proof(
    builder: &mut PreviewBuilder<'_>,
    candidate_idx: usize,
    witness: &fission_midend_structuring::guarded_tail::RegionShapeWitness,
    legality_reason: fission_midend_structuring::regions::BlockGraphLegalityReason,
) {
    builder.record_guarded_tail_blockgraph_proof_impl(candidate_idx, witness, legality_reason)
}

