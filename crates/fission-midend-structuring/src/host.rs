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
use crate::guarded_tail::types::{
    GuardedTailCanonicalizationFailure, GuardedTailExecutionPlan, GuardedTailExecutionRejection,
    GuardedTailTrial, GuardedTailVerification, PromotionGateRejection, PromotionShapeRejection,
};
use crate::linear_types::{
    ConditionalTailKey, ConditionalTailMismatchSubtype, IfLoweringBudget,
    LinearBodyCacheKey, LinearBodyCachedOutcome, LinearBodyLoweringOutcome,
    LinearBodyRejectReason, LinearExit, LoweredTerminator,
};
use crate::loop_analysis::LoopBody;
use fission_midend_core::ir::{HirExpr, HirStmt, MlilPreviewError, MlilPreviewOptions};
use crate::HashMap;
use crate::HashSet;

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
    /// Resets the deterministic call-count budget consumed by
    /// `sese_region_proof_budget_exceeded()`. Call once per structuring
    /// attempt, alongside `set_structuring_start`.
    fn reset_sese_region_proof_budget(&mut self);
    /// Shared, live-updating checkpoint counter for the whole structuring
    /// attempt's total-work budget (`STRUCTURING_TOTAL_WORK_BUDGET`).
    /// `IfLoweringBudget` instances hold a clone of this handle so their
    /// `checkpoint()` calls (which don't have `&host` in scope) can still
    /// contribute to and observe one shared total across the many
    /// `try_lower_if`/`try_lower_while` attempts a single function's
    /// structuring makes. Reset alongside `reset_sese_region_proof_budget`.
    fn structuring_total_work_counter(&self) -> std::rc::Rc<std::cell::Cell<u64>>;
    fn reset_structuring_total_work_counter(&mut self);

    // ── Address / block identity ───────────────────────────────────────────
    fn block_target_key(&self, idx: usize) -> u64;
    fn block_start_address(&self, idx: usize) -> u64;
    fn find_block_index_by_address(&self, address: u64) -> Option<usize>;
    fn next_block_address(&self, idx: usize) -> Option<u64>;
    fn fallthrough_index(&self, idx: usize) -> Option<usize>;
    fn pcode_block_idx(&self, idx: usize) -> usize;

    // ── Lowering hooks (HIR-only surface; p-code stays in the host) ────────
    fn lower_block_stmts(&mut self, block_idx: usize) -> Result<Vec<HirStmt>, MlilPreviewError>;
    fn lower_block_terminator(
        &mut self,
        block_idx: usize,
    ) -> Result<LoweredTerminator, MlilPreviewError>;
    fn lower_return_join_expr_for_predecessor(
        &mut self,
        pred_idx: usize,
        join_idx: usize,
    ) -> Result<Option<HirExpr>, MlilPreviewError>;
    fn lower_linear_body(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError>;
    fn lower_linear_body_with_budget(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError>;
    fn linear_exit(
        &mut self,
        idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError>;
    fn linear_exit_with_budget(
        &mut self,
        idx: usize,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<Option<LinearExit>, MlilPreviewError>;
    fn shared_linear_exit(
        &mut self,
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError>;
    fn has_linear_body_cache(&self, start_idx: usize, exit: LinearExit) -> bool;
    fn linear_body_cache_get(
        &self,
        key: &LinearBodyCacheKey,
    ) -> Option<LinearBodyCachedOutcome>;
    fn linear_body_cache_insert(
        &mut self,
        key: LinearBodyCacheKey,
        value: LinearBodyCachedOutcome,
    );
    /// Returns `false` if `key` is already active (revisit cycle).
    fn linear_body_active_insert(&mut self, key: LinearBodyCacheKey) -> bool;
    fn linear_body_active_remove(&mut self, key: &LinearBodyCacheKey);
    /// Returns `false` if `key` is already active.
    fn conditional_tail_active_insert(&mut self, key: ConditionalTailKey) -> bool;
    fn conditional_tail_active_remove(&mut self, key: &ConditionalTailKey);
    fn linear_exit_cache_get(&self, idx: usize) -> Option<Option<LinearExit>>;
    fn linear_exit_cache_insert(&mut self, idx: usize, exit: Option<LinearExit>);
    fn can_inline_linear_successor(
        &self,
        from_idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
    ) -> bool;
    fn can_inline_linear_successor_for_region(
        &self,
        from_idx: usize,
        next_idx: usize,
        visited: &HashSet<usize>,
        exit: LinearExit,
    ) -> bool;
    fn is_trivial_forwarding_block(&self, idx: usize, next_idx: usize) -> bool;
    fn is_trivial_linear_tail(&self, idx: usize) -> bool;
    fn forwarding_block_defines_return_tail_live_in(&self, idx: usize, join_idx: usize) -> bool;
    fn record_conditional_tail_mismatch_subtype(
        &mut self,
        subtype: ConditionalTailMismatchSubtype,
    );
    fn record_conditional_tail_mismatch_sample(
        &self,
        origin_idx: usize,
        true_idx: Option<usize>,
        false_idx: Option<usize>,
        exit: LinearExit,
        subtype: ConditionalTailMismatchSubtype,
        stage: &str,
    );
    fn bump_rule_block_if_no_exit(&mut self);
    fn shared_exit_for_indices(
        &mut self,
        indices: &[usize],
    ) -> Result<Option<LinearExit>, MlilPreviewError>;
    fn collect_jump_targets(&mut self) -> Result<HashSet<u64>, MlilPreviewError>;
    /// Labels of blocks reachable only via an out-of-band edge with no
    /// textual `Goto` representation at all -- currently just C++ exception
    /// landing pads (see `fission_loader::loader::elf::lsda`), reachable
    /// only via the personality routine unwinding into them at runtime.
    /// `cleanup_redundant_labels`/`collect_referenced_labels` treat any
    /// label with zero referencing `Goto` as dead and drop it -- correct
    /// for ordinary code, wrong here, since the label is a real entry point
    /// that just has no `HirStmt::Goto` anywhere pointing at it. Callers
    /// that clean up labels must union this set into their "referenced"
    /// set rather than relying on `collect_referenced_labels` alone.
    fn lsda_landing_pad_labels(&self) -> HashSet<String>;
    fn accept_structured_region(
        &mut self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool;
    fn sese_region_proof_budget_exceeded(&self) -> bool;
    fn region_has_external_entry(&self, region: &HashSet<usize>, header_idx: usize) -> bool;
    /// Whether the condition head block has only pure ops discardable for for-loop form.
    fn head_has_only_discardable_pure_ops(&self, block_idx: usize) -> bool;
    /// Peek cached terminator branch targets for label pre-seeding (no lowering).
    fn cached_terminator_branch_targets(&self, block_idx: usize) -> Option<Vec<u64>>;
    /// Region-recovery linear body lowering (detailed reject reasons).
    fn lower_linear_body_for_region_recovery_detailed(
        &mut self,
        start_idx: usize,
        exit: LinearExit,
        budget: Option<&mut IfLoweringBudget>,
    ) -> Result<LinearBodyLoweringOutcome, MlilPreviewError>;
    fn record_region_body_lowering_reject_reason(&mut self, reason: LinearBodyRejectReason);
    fn bump_region_linearize_rejected_irreducible_cfg(&mut self);
    fn bump_region_linearize_rejected_non_structuring_failure(&mut self);
    fn bump_region_linearize_rejected_no_exit(&mut self);
    fn bump_region_linearize_rejected_body_lowering_failed(&mut self);
    fn bump_region_linearize_rejected_non_advancing(&mut self);
    fn bump_region_linearize_structuring(&mut self);

    // ── Telemetry ──────────────────────────────────────────────────────────
    fn bump_region_proof_candidate(&mut self);
    fn bump_region_proof_completed(&mut self);
    /// Record a completed structured-region candidate (proof telemetry).
    fn record_region_candidate(&mut self, proof: &crate::regions::RegionProof);
    /// Record that a structured candidate was selected for collapse insertion.
    fn record_selected_region(&mut self, node: &crate::graph::StructureNode);
    fn bump_guarded_tail_candidate(&mut self);
    fn bump_promotion_candidate(&mut self);
    fn bump_promotion_rejected_by_shape(&mut self);
    fn bump_promotion_rejected_by_gate(&mut self);
    fn bump_region_emit_ready_failed(&mut self);
    fn bump_fas_virtual_goto(&mut self);
    fn bump_select_bad_edge(&mut self);
    fn bump_condition_fold_and(&mut self, fold_count: usize);
    fn bump_condition_fold_or(&mut self, fold_count: usize);
    fn bump_condition_fold_rejected_side_effect(&mut self);
    fn track_loop_control_rewrite_stats(
        &mut self,
        break_rewrites: usize,
        continue_rewrites: usize,
        skipped_nested_scope_count: usize,
    );
    fn bump_loop_control_explicit_reducer(&mut self);
    fn bump_loop_while_subgraph_lowered(&mut self);
    fn bump_loop_multi_tail_dowhile_lowered(&mut self);
    fn bump_loop_for_lowered(&mut self);
    fn bump_loop_multi_exit_break(&mut self);
    fn bump_switch_fallthrough_detected(&mut self, count: usize);
    fn bump_switch_emit_ready_failed(&mut self);
    fn bump_proof_payload_direct_emit(&mut self);
    fn bump_sese_child_localized_linear(&mut self);

    // ── Caches ─────────────────────────────────────────────────────────────
    /// Drop any cached terminator lowering for `block_idx` after CFG mutation.
    fn invalidate_terminator_cache(&mut self, block_idx: usize);

    // ── Diagnostics ────────────────────────────────────────────────────────
    fn emit_ready_trace_enabled(&self) -> bool;
    fn emit_ready_trace(&self, message: &str);
    fn guarded_tail_trace_enabled(&self) -> bool;
    fn log_try_lower_if_reject(&self, idx: usize, reason: &str);

    // ── Guarded-tail residual hooks ─────────────────────────────────────────
    /// Free-fn pipeline entry (implemented by calling midend free owners).
    fn try_build_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        referenced: &HashMap<String, usize>,
    ) -> Option<Result<GuardedTailTrial, crate::guarded_tail::types::GuardedTailWitnessRejection>>;
    fn verify_guarded_tail_trial(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
    ) -> GuardedTailVerification;
    fn build_guarded_tail_execution_plan(
        &mut self,
        body: &[HirStmt],
        idx: usize,
        trial: &GuardedTailTrial,
        verification: &GuardedTailVerification,
    ) -> Result<GuardedTailExecutionPlan, GuardedTailExecutionRejection>;
    fn execute_guarded_tail_plan(
        &mut self,
        body: &mut Vec<HirStmt>,
        idx: usize,
        trial: GuardedTailTrial,
        plan: GuardedTailExecutionPlan,
        cond: HirExpr,
    );
    fn discover_guarded_tail_candidates_in_body(&mut self, body: &[HirStmt]);
    fn mark_promotion_shape_rejection(&mut self, reason: PromotionShapeRejection);
    fn mark_promotion_gate_rejection(&mut self, reason: PromotionGateRejection);
    fn mark_guarded_tail_execution_rejection(&mut self, reason: GuardedTailExecutionRejection);
    fn mark_guarded_tail_canonicalization_failure(
        &mut self,
        failure: GuardedTailCanonicalizationFailure,
    );

    // Residual PreviewBuilder-only GT helpers used by free-fn bodies.
    fn mark_alias_nonlocal_external_before(&mut self);
    fn mark_alias_nonlocal_from_external_sites(
        &mut self,
        external_top_level_before: usize,
        external_nested_before: usize,
        external_refs_after: usize,
    );
    fn resolve_terminal_join_target(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        target_label: &str,
        referenced: &HashMap<String, usize>,
    ) -> Option<(String, usize)>;
    fn mark_noncanonical_layout_rejection(&mut self);
    fn record_guarded_tail_blockgraph_proof(
        &mut self,
        candidate_idx: usize,
        witness: &crate::guarded_tail::types::RegionShapeWitness,
        legality_reason: crate::regions::BlockGraphLegalityReason,
    );
    fn guarded_tail_function_address(&self) -> u64;
    fn guarded_tail_trace_emit_snapshot(&self, prefix: &str, stmts: &[HirStmt], limit: usize);
    fn bump_structuring_counter(
        &mut self,
        counter: crate::guarded_tail::bodies::StructuringCounter,
        amount: usize,
    );
    fn alloc_temp_binding(
        &mut self,
        ty: fission_midend_core::ir::NirType,
        origin: Option<fission_midend_core::ir::NirBindingOrigin>,
    ) -> String;
    fn find_earliest_owned_join_label_with_diag(
        &mut self,
        body: &[HirStmt],
        anchor_idx: usize,
        terminal_label_idx: usize,
        referenced: &HashMap<String, usize>,
        trace_enabled: bool,
    ) -> Option<(String, usize)>;

    /// Residual: preview callee-analysis summary for unsafe suffix call reject.
    fn suffix_call_uses_preview_unsafe_callee(&self, stmt: &HirStmt) -> Option<String>;
    /// Residual: optional GT-TRACE provenance dump for unknown suffix calls.
    fn trace_suffix_unknown_call_provenance(&self, stmt_idx: usize, stmt: &HirStmt);
    /// Residual: look up NIR call-effect summary by callee name.
    fn call_effect_summary_for_target(
        &self,
        target: &str,
    ) -> Option<fission_midend_core::ir::NirCallEffectSummary>;
    /// Residual: binary / type-context facts for suffix call provenance traces.
    fn suffix_call_provenance_facts(
        &self,
        target: &str,
        target_addr: Option<u64>,
    ) -> crate::guarded_tail::suffix_window::SuffixCallProvenanceFacts;
    /// Residual: inventory note when linear multiblock emits unsupported terminator.
    fn note_unsupported_terminator_emit(&mut self, block_addr: u64);
    /// Residual: admit switch recovery from CFG shape at `idx`.
    fn switch_recovery_cfg_admitted(&self, idx: usize) -> bool;

    /// True when another p-code block shares this virtual block's start address.
    fn has_same_start_address_peer(&self, idx: usize) -> bool;
    /// Emit residual unsupported control as HIR (call/return surface polish).
    fn emit_unsupported_control_surface(
        &mut self,
        evidence: fission_midend_core::ir::UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
    ) -> HirStmt;

    // ── Derived CFG helpers ────────────────────────────────────────────────
    fn analyze_cfg_scc(&self) -> SccAnalysis {
        self.cfg_facts().scc().clone()
    }
    fn analyze_cfg_dominators(&self) -> DomTree {
        self.cfg_facts().dominators().clone()
    }
    fn get_loop_body(&self, head_idx: usize) -> Option<&LoopBody> {
        self.loop_bodies().iter().find(|lb| lb.head == head_idx)
    }
}
