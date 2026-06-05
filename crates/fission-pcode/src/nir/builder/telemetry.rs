//! Internal builder-side telemetry accumulator.
//!
//! This is intentionally crate-private. The public telemetry contract remains
//! the flat `NirBuildStats` shape emitted by `builder::stats`.

use crate::nir::types::PreviewBuildStats;

#[derive(Debug, Default)]
pub(crate) struct BuilderTelemetry {
    pub(crate) core: CoreTelemetry,
    pub(crate) procedure: ProcedureTelemetry,
    pub(crate) structuring: StructuringTelemetry,
    pub(crate) call_targets: CallTargetTelemetry,
    pub(crate) indirect_control: IndirectControlTelemetry,
    pub(crate) materialization: MaterializationTelemetry,
    pub(crate) dispatcher: DispatcherTelemetry,
}

impl BuilderTelemetry {
    pub(crate) fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        self.core.apply_to_public_stats(stats);
        self.procedure.apply_to_public_stats(stats);
        self.structuring.apply_to_public_stats(stats);
        self.call_targets.apply_to_public_stats(stats);
        self.indirect_control.apply_to_public_stats(stats);
        self.materialization.apply_to_public_stats(stats);
        self.dispatcher.apply_to_public_stats(stats);
    }
}

#[derive(Debug, Default)]
pub(crate) struct CoreTelemetry {
    pub(crate) build_duration_ms: usize,
    pub(crate) normalize_duration_ms: usize,
    pub(crate) structuring_duration_ms: usize,
    pub(crate) render_duration_ms: usize,
    pub(crate) rendered_code_len: usize,
    pub(crate) max_structuring_scc_component_size: usize,
}

impl CoreTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.build_duration_ms = self.build_duration_ms;
        stats.normalize_duration_ms = self.normalize_duration_ms;
        stats.structuring_duration_ms = self.structuring_duration_ms;
        stats.render_duration_ms = self.render_duration_ms;
        stats.rendered_code_len = self.rendered_code_len;
        stats.max_structuring_scc_component_size = self.max_structuring_scc_component_size;
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProcedureTelemetry {
    pub(crate) procedure_summary_contracted_count: usize,
    pub(crate) procedure_summary_tail_wrapper_count: usize,
    pub(crate) procedure_summary_import_thunk_count: usize,
}

impl ProcedureTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.procedure_summary_contracted_count = self.procedure_summary_contracted_count;
        stats.procedure_summary_tail_wrapper_count = self.procedure_summary_tail_wrapper_count;
        stats.procedure_summary_import_thunk_count = self.procedure_summary_import_thunk_count;
    }
}

#[derive(Debug, Default)]
pub(crate) struct CallTargetTelemetry {
    pub(crate) call_target_import_resolved_count: usize,
    pub(crate) call_target_direct_symbol_resolved_count: usize,
    pub(crate) call_target_unresolved_sub_fallback_count: usize,
    pub(crate) call_target_context_missing_count: usize,
    pub(crate) call_target_exact_index_hit_count: usize,
    pub(crate) call_target_exact_index_ambiguous_count: usize,
    pub(crate) call_target_export_thunk_target_resolved_count: usize,
    pub(crate) call_target_indirect_const_resolved_count: usize,
    pub(crate) call_target_iat_slot_resolved_count: usize,
    pub(crate) call_target_indirect_load_resolved_count: usize,
    pub(crate) call_target_indirect_ptr_const_folded_count: usize,
    pub(crate) call_target_indirect_rejected_non_iat_load_count: usize,
    pub(crate) call_target_indirect_rejected_non_const_ptr_count: usize,
    pub(crate) call_target_indirect_rejected_unsupported_ptr_opcode_count: usize,
    pub(crate) call_target_indirect_rejected_ambiguous_def_count: usize,
    pub(crate) call_target_indirect_rejected_non_dominating_def_count: usize,
    pub(crate) call_target_indirect_rejected_no_def_count: usize,
    pub(crate) call_target_indirect_rejected_width_mismatch_count: usize,
    pub(crate) call_target_unresolved_no_exact_identity_count: usize,
}

impl CallTargetTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.call_target_import_resolved_count = self.call_target_import_resolved_count;
        stats.call_target_direct_symbol_resolved_count =
            self.call_target_direct_symbol_resolved_count;
        stats.call_target_unresolved_sub_fallback_count =
            self.call_target_unresolved_sub_fallback_count;
        stats.call_target_context_missing_count = self.call_target_context_missing_count;
        stats.call_target_exact_index_hit_count = self.call_target_exact_index_hit_count;
        stats.call_target_exact_index_ambiguous_count =
            self.call_target_exact_index_ambiguous_count;
        stats.call_target_export_thunk_target_resolved_count =
            self.call_target_export_thunk_target_resolved_count;
        stats.call_target_indirect_const_resolved_count =
            self.call_target_indirect_const_resolved_count;
        stats.call_target_iat_slot_resolved_count = self.call_target_iat_slot_resolved_count;
        stats.call_target_indirect_load_resolved_count =
            self.call_target_indirect_load_resolved_count;
        stats.call_target_indirect_ptr_const_folded_count =
            self.call_target_indirect_ptr_const_folded_count;
        stats.call_target_indirect_rejected_non_iat_load_count =
            self.call_target_indirect_rejected_non_iat_load_count;
        stats.call_target_indirect_rejected_non_const_ptr_count =
            self.call_target_indirect_rejected_non_const_ptr_count;
        stats.call_target_indirect_rejected_unsupported_ptr_opcode_count =
            self.call_target_indirect_rejected_unsupported_ptr_opcode_count;
        stats.call_target_indirect_rejected_ambiguous_def_count =
            self.call_target_indirect_rejected_ambiguous_def_count;
        stats.call_target_indirect_rejected_non_dominating_def_count =
            self.call_target_indirect_rejected_non_dominating_def_count;
        stats.call_target_indirect_rejected_no_def_count =
            self.call_target_indirect_rejected_no_def_count;
        stats.call_target_indirect_rejected_width_mismatch_count =
            self.call_target_indirect_rejected_width_mismatch_count;
        stats.call_target_unresolved_no_exact_identity_count =
            self.call_target_unresolved_no_exact_identity_count;
    }
}

#[derive(Debug, Default)]
pub(crate) struct IndirectControlTelemetry {
    pub(crate) unsupported_indirect_control_count: usize,
    pub(crate) unsupported_indirect_call_count: usize,
    pub(crate) unsupported_external_target_count: usize,
    pub(crate) indirect_surface_preserved_count: usize,
    pub(crate) indirect_target_set_refined_count: usize,
    pub(crate) dispatcher_shape_recovered_count: usize,
}

impl IndirectControlTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.unsupported_indirect_control_count = self.unsupported_indirect_control_count;
        stats.unsupported_indirect_call_count = self.unsupported_indirect_call_count;
        stats.unsupported_external_target_count = self.unsupported_external_target_count;
        stats.indirect_surface_preserved_count = self.indirect_surface_preserved_count;
        stats.indirect_target_set_refined_count = self.indirect_target_set_refined_count;
        stats.dispatcher_shape_recovered_count = self.dispatcher_shape_recovered_count;
    }
}

#[derive(Debug, Default)]
pub(crate) struct MaterializationTelemetry {
    pub(crate) materialization_stabilized_count: usize,
    pub(crate) replacement_plan_candidate_count: usize,
    pub(crate) replacement_plan_completed_count: usize,
    pub(crate) replacement_plan_merge_binding_count: usize,
    pub(crate) replacement_plan_rejected_alias_unsafe_count: usize,
    pub(crate) replacement_plan_rejected_missing_merge_count: usize,
    pub(crate) replacement_plan_rejected_representative_root_attribution_count: usize,
    pub(crate) replacement_plan_rejected_temp_only_representative_lifecycle_count: usize,
    pub(crate) replacement_plan_rejected_dead_temp_representative_count: usize,
    pub(crate) materialization_inline_suppressed_count: usize,
    pub(crate) representative_downgrade_count: usize,
    pub(crate) representative_downgrade_no_aliassafe_source_count: usize,
    pub(crate) representative_downgrade_join_conflict_count: usize,
}

impl MaterializationTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.materialization_stabilized_count = self.materialization_stabilized_count;
        stats.replacement_plan_candidate_count = self.replacement_plan_candidate_count;
        stats.replacement_plan_completed_count = self.replacement_plan_completed_count;
        stats.replacement_plan_merge_binding_count = self.replacement_plan_merge_binding_count;
        stats.replacement_plan_rejected_alias_unsafe_count =
            self.replacement_plan_rejected_alias_unsafe_count;
        stats.replacement_plan_rejected_missing_merge_count =
            self.replacement_plan_rejected_missing_merge_count;
        stats.replacement_plan_rejected_representative_root_attribution_count =
            self.replacement_plan_rejected_representative_root_attribution_count;
        stats.replacement_plan_rejected_temp_only_representative_lifecycle_count =
            self.replacement_plan_rejected_temp_only_representative_lifecycle_count;
        stats.replacement_plan_rejected_dead_temp_representative_count =
            self.replacement_plan_rejected_dead_temp_representative_count;
        stats.materialization_inline_suppressed_count =
            self.materialization_inline_suppressed_count;
        stats.representative_downgrade_count = self.representative_downgrade_count;
        stats.representative_downgrade_no_aliassafe_source_count =
            self.representative_downgrade_no_aliassafe_source_count;
        stats.representative_downgrade_join_conflict_count =
            self.representative_downgrade_join_conflict_count;
    }
}

#[derive(Debug, Default)]
pub(crate) struct DispatcherTelemetry {
    pub(crate) dispatcher_proof_unit_count: usize,
    pub(crate) dispatcher_proof_completed_count: usize,
    pub(crate) dispatcher_proof_failed_count: usize,
    pub(crate) switch_emit_ready_failed_count: usize,
    pub(crate) pe_admission_profile_mismatch_count: usize,
    pub(crate) proof_payload_direct_emit_count: usize,
}

impl DispatcherTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.dispatcher_proof_unit_count = self.dispatcher_proof_unit_count;
        stats.dispatcher_proof_completed_count = self.dispatcher_proof_completed_count;
        stats.dispatcher_proof_failed_count = self.dispatcher_proof_failed_count;
        stats.switch_emit_ready_failed_count = self.switch_emit_ready_failed_count;
        stats.pe_admission_profile_mismatch_count = self.pe_admission_profile_mismatch_count;
        stats.proof_payload_direct_emit_count = self.proof_payload_direct_emit_count;
    }
}

#[derive(Debug, Default)]
pub(crate) struct StructuringTelemetry {
    pub(crate) forced_linear_structuring_count: usize,
    pub(crate) blockgraph_collapse_admission_enabled_count: usize,
    pub(crate) blockgraph_collapse_irreducible_budget_bypass_count: usize,
    pub(crate) blockgraph_collapse_extreme_budget_blocked_count: usize,
    /// Back-edges virtualized as explicit gotos by the FAS fallback when node-splitting
    /// exceeds budget (irreducible SCCs too large to split).
    pub(crate) fas_virtual_goto_count: usize,
    /// Switch cases patched with `/* fallthrough */` instead of an explicit goto.
    pub(crate) switch_fallthrough_detected_count: usize,
    pub(crate) structuring_force_linear_explicit_count: usize,
    pub(crate) structuring_force_linear_irreducible_budget_count: usize,
    pub(crate) structuring_force_linear_extreme_budget_count: usize,
    pub(crate) region_linearize_structuring_count: usize,
    pub(crate) region_linearize_rejected_non_structuring_failure_count: usize,
    pub(crate) region_linearize_rejected_no_exit_count: usize,
    pub(crate) region_linearize_rejected_body_lowering_failed_count: usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    pub(crate) region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    pub(crate) region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    pub(crate) region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    pub(crate) region_linearize_rejected_non_advancing_count: usize,
    pub(crate) region_linearize_rejected_irreducible_cfg_count: usize,
    pub(crate) structuring_scc_component_count: usize,
    pub(crate) structuring_irreducible_scc_count: usize,
    pub(crate) rule_block_if_no_exit_count: usize,
    pub(crate) rule_block_if_no_exit_accepted_count: usize,
    pub(crate) structuring_irreducible_header_count: usize,
    pub(crate) loop_control_explicit_reducer_count: usize,
    pub(crate) loop_control_rewrite_break_count: usize,
    pub(crate) loop_control_rewrite_continue_count: usize,
    pub(crate) loop_control_rewrite_skipped_nested_scope_count: usize,
    pub(crate) loop_while_subgraph_lowered_count: usize,
    /// How many multi-tail do-while loops were successfully lowered after Ghidra-style
    /// tail ordering placed the preferred (exit-adjacent) latch at index 0.
    pub(crate) loop_multi_tail_dowhile_lowered_count: usize,
    pub(crate) loop_multi_exit_break_count: usize,
    pub(crate) loop_for_lowered_count: usize,
    pub(crate) region_proof_candidate_count: usize,
    pub(crate) region_proof_completed_count: usize,
    pub(crate) region_emit_ready_failed_count: usize,
    pub(crate) blockgraph_region_candidate_count: usize,
    pub(crate) blockgraph_region_complete_count: usize,
    pub(crate) blockgraph_region_rejected_missing_follow_count: usize,
    pub(crate) blockgraph_region_rejected_must_emit_label_count: usize,
    pub(crate) blockgraph_region_rejected_middle_ref_count: usize,
    pub(crate) blockgraph_region_rejected_external_ref_count: usize,
    pub(crate) blockgraph_region_rejected_join_owner_conflict_count: usize,
    pub(crate) blockgraph_region_rejected_nonterminal_join_count: usize,
    pub(crate) blockgraph_region_rejected_follow_owner_conflict_count: usize,
    pub(crate) blockgraph_region_rejected_emit_ready_count: usize,
    pub(crate) blockgraph_region_rejected_irreducible_count: usize,
    pub(crate) conditional_region_candidate_count: usize,
    pub(crate) conditional_region_promoted_count: usize,
    pub(crate) guarded_tail_candidate_count: usize,
    pub(crate) guarded_tail_promoted_count: usize,
    pub(crate) promotion_candidate_count: usize,
    pub(crate) promoted_region_count: usize,
    pub(crate) promotion_rejected_by_shape_count: usize,
    pub(crate) promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    pub(crate) promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    pub(crate) promotion_rejected_by_gate_count: usize,
    pub(crate) discovery_seen_guarded_tail_like_shape_count: usize,
    pub(crate) guarded_tail_rejected_missing_terminal_join_count: usize,
    pub(crate) guarded_tail_rejected_side_entry_conflict_count: usize,
    pub(crate) guarded_tail_rejected_alias_interleave_conflict_count: usize,
    pub(crate) guarded_tail_rejected_ambiguous_follow_count: usize,
    pub(crate) guarded_tail_rejected_side_effectful_callee_count: usize,
    pub(crate) guarded_tail_replacement_plan_candidate_count: usize,
    pub(crate) guarded_tail_replacement_plan_completed_count: usize,
    pub(crate) guarded_tail_replacement_plan_merge_created_count: usize,
    pub(crate) guarded_tail_replacement_plan_rejected_missing_merge_count: usize,
    pub(crate) guarded_tail_replacement_plan_rejected_unstable_read_count: usize,
    pub(crate) guarded_tail_exported_binding_count: usize,
    pub(crate) guarded_tail_replacement_read_count: usize,
    pub(crate) guarded_tail_replacement_read_rewritten_count: usize,
    pub(crate) guarded_tail_replacement_read_rejected_nondominated_count: usize,
    pub(crate) guarded_tail_replacement_read_rejected_nonremovable_op_count: usize,
    pub(crate) discovery_rejected_noncanonical_layout_count: usize,
    pub(crate) canonicalized_guarded_tail_shape_count: usize,
    pub(crate) canonicalization_failed_multiple_payload_entries: usize,
    pub(crate) canonicalization_failed_interleaved_join_uses: usize,
    pub(crate) canonicalization_failed_interleaved_join_uses_no_next_label_count: usize,
    pub(crate) canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: usize,
    pub(crate) canonicalization_failed_nonterminal_join_label: usize,
    pub(crate) canonicalization_failed_nested_tail_escape: usize,
    pub(crate) canonicalized_interleaved_join_use_count: usize,
    pub(crate) canonicalized_local_nonfallthrough_alias_count: usize,
    pub(crate) canonicalization_failed_alias_not_fallthrough_count: usize,
    pub(crate) canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    pub(crate) canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    pub(crate) canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    pub(crate) canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    pub(crate) canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    pub(crate) canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    pub(crate) canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    pub(crate) canonicalization_failed_alias_body_not_trivial_count: usize,
    pub(crate) canonicalization_failed_join_has_external_ref_count: usize,
    pub(crate) canonicalization_failed_payload_crosses_join_count: usize,
    pub(crate) rejected_must_emit_label: usize,
    pub(crate) rejected_must_emit_label_surviving_middle_ref: usize,
    pub(crate) rejected_must_emit_label_surviving_external_ref: usize,
    pub(crate) rejected_must_emit_label_owner_conflict: usize,
    pub(crate) rejected_not_single_pred_succ: usize,
    pub(crate) rejected_external_entry: usize,
    pub(crate) rejected_loop_or_switch_target: usize,
    pub(crate) condition_fold_and_count: usize,
    pub(crate) condition_fold_or_count: usize,
    pub(crate) condition_fold_rejected_side_effect: usize,
    /// SESE structuring succeeded but produced orphan goto labels (Goto without matching Label),
    /// indicating a back-edge label was omitted. Fell back to linear structuring.
    pub(crate) structuring_sese_orphan_goto_fallback_count: usize,
    /// Orphan goto labels were localized by appending missing block labels/bodies.
    pub(crate) structuring_orphan_goto_localized_count: usize,
    /// Orphan goto labels could not be localized; full linear fallback was used.
    pub(crate) structuring_orphan_goto_unrepairable_count: usize,
    /// A child SESE region failed and was isolated as linear without discarding parent structure.
    pub(crate) sese_child_localized_linear_count: usize,
    /// Edges virtualized via iterative select-bad-edge during collapse loop.
    pub(crate) structuring_select_bad_edge_count: usize,
}

impl StructuringTelemetry {
    fn apply_to_public_stats(&self, stats: &mut PreviewBuildStats) {
        stats.forced_linear_structuring_count = self.forced_linear_structuring_count;
        stats.blockgraph_collapse_admission_enabled_count =
            self.blockgraph_collapse_admission_enabled_count;
        stats.blockgraph_collapse_irreducible_budget_bypass_count =
            self.blockgraph_collapse_irreducible_budget_bypass_count;
        stats.blockgraph_collapse_extreme_budget_blocked_count =
            self.blockgraph_collapse_extreme_budget_blocked_count;
        stats.fas_virtual_goto_count = self.fas_virtual_goto_count;
        stats.switch_fallthrough_detected_count = self.switch_fallthrough_detected_count;
        stats.structuring_force_linear_explicit_count =
            self.structuring_force_linear_explicit_count;
        stats.structuring_force_linear_irreducible_budget_count =
            self.structuring_force_linear_irreducible_budget_count;
        stats.structuring_force_linear_extreme_budget_count =
            self.structuring_force_linear_extreme_budget_count;
        stats.region_linearize_structuring_count = self.region_linearize_structuring_count;
        stats.region_linearize_rejected_non_structuring_failure_count =
            self.region_linearize_rejected_non_structuring_failure_count;
        stats.region_linearize_rejected_no_exit_count =
            self.region_linearize_rejected_no_exit_count;
        stats.region_linearize_rejected_body_lowering_failed_count =
            self.region_linearize_rejected_body_lowering_failed_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count;
        stats.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count;
        stats.region_linearize_rejected_body_lowering_successor_inline_rejected_count =
            self.region_linearize_rejected_body_lowering_successor_inline_rejected_count;
        stats.region_linearize_rejected_body_lowering_revisit_cycle_count =
            self.region_linearize_rejected_body_lowering_revisit_cycle_count;
        stats.region_linearize_rejected_body_lowering_unsupported_terminator_count =
            self.region_linearize_rejected_body_lowering_unsupported_terminator_count;
        stats.region_linearize_rejected_non_advancing_count =
            self.region_linearize_rejected_non_advancing_count;
        stats.region_linearize_rejected_irreducible_cfg_count =
            self.region_linearize_rejected_irreducible_cfg_count;
        stats.structuring_scc_component_count = self.structuring_scc_component_count;
        stats.structuring_irreducible_scc_count = self.structuring_irreducible_scc_count;
        stats.rule_block_if_no_exit_count = self.rule_block_if_no_exit_count;
        stats.rule_block_if_no_exit_accepted_count = self.rule_block_if_no_exit_accepted_count;
        stats.structuring_irreducible_header_count = self.structuring_irreducible_header_count;
        stats.loop_control_explicit_reducer_count = self.loop_control_explicit_reducer_count;
        stats.loop_control_rewrite_break_count = self.loop_control_rewrite_break_count;
        stats.loop_control_rewrite_continue_count = self.loop_control_rewrite_continue_count;
        stats.loop_control_rewrite_skipped_nested_scope_count =
            self.loop_control_rewrite_skipped_nested_scope_count;
        stats.loop_while_subgraph_lowered_count = self.loop_while_subgraph_lowered_count;
        stats.loop_multi_tail_dowhile_lowered_count = self.loop_multi_tail_dowhile_lowered_count;
        stats.loop_multi_exit_break_count = self.loop_multi_exit_break_count;
        stats.loop_for_lowered_count = self.loop_for_lowered_count;
        stats.region_proof_candidate_count = self.region_proof_candidate_count;
        stats.region_proof_completed_count = self.region_proof_completed_count;
        stats.region_emit_ready_failed_count = self.region_emit_ready_failed_count;
        stats.blockgraph_region_candidate_count = self.blockgraph_region_candidate_count;
        stats.blockgraph_region_complete_count = self.blockgraph_region_complete_count;
        stats.blockgraph_region_rejected_missing_follow_count =
            self.blockgraph_region_rejected_missing_follow_count;
        stats.blockgraph_region_rejected_must_emit_label_count =
            self.blockgraph_region_rejected_must_emit_label_count;
        stats.blockgraph_region_rejected_middle_ref_count =
            self.blockgraph_region_rejected_middle_ref_count;
        stats.blockgraph_region_rejected_external_ref_count =
            self.blockgraph_region_rejected_external_ref_count;
        stats.blockgraph_region_rejected_join_owner_conflict_count =
            self.blockgraph_region_rejected_join_owner_conflict_count;
        stats.blockgraph_region_rejected_nonterminal_join_count =
            self.blockgraph_region_rejected_nonterminal_join_count;
        stats.blockgraph_region_rejected_follow_owner_conflict_count =
            self.blockgraph_region_rejected_follow_owner_conflict_count;
        stats.blockgraph_region_rejected_emit_ready_count =
            self.blockgraph_region_rejected_emit_ready_count;
        stats.blockgraph_region_rejected_irreducible_count =
            self.blockgraph_region_rejected_irreducible_count;
        stats.conditional_region_candidate_count = self.conditional_region_candidate_count;
        stats.conditional_region_promoted_count = self.conditional_region_promoted_count;
        stats.guarded_tail_candidate_count = self.guarded_tail_candidate_count;
        stats.guarded_tail_promoted_count = self.guarded_tail_promoted_count;
        stats.promotion_candidate_count = self.promotion_candidate_count;
        stats.promoted_region_count = self.promoted_region_count;
        stats.promotion_rejected_by_shape_count = self.promotion_rejected_by_shape_count;
        stats.promotion_rejected_by_shape_missing_terminal_join_target_count =
            self.promotion_rejected_by_shape_missing_terminal_join_target_count;
        stats.promotion_rejected_by_shape_empty_nonterminal_tail_count =
            self.promotion_rejected_by_shape_empty_nonterminal_tail_count;
        stats.promotion_rejected_by_gate_count = self.promotion_rejected_by_gate_count;
        stats.discovery_seen_guarded_tail_like_shape_count =
            self.discovery_seen_guarded_tail_like_shape_count;
        stats.guarded_tail_rejected_missing_terminal_join_count =
            self.guarded_tail_rejected_missing_terminal_join_count;
        stats.guarded_tail_rejected_side_entry_conflict_count =
            self.guarded_tail_rejected_side_entry_conflict_count;
        stats.guarded_tail_rejected_alias_interleave_conflict_count =
            self.guarded_tail_rejected_alias_interleave_conflict_count;
        stats.guarded_tail_rejected_ambiguous_follow_count =
            self.guarded_tail_rejected_ambiguous_follow_count;
        stats.guarded_tail_rejected_side_effectful_callee_count =
            self.guarded_tail_rejected_side_effectful_callee_count;
        stats.guarded_tail_replacement_plan_candidate_count =
            self.guarded_tail_replacement_plan_candidate_count;
        stats.guarded_tail_replacement_plan_completed_count =
            self.guarded_tail_replacement_plan_completed_count;
        stats.guarded_tail_replacement_plan_merge_created_count =
            self.guarded_tail_replacement_plan_merge_created_count;
        stats.guarded_tail_replacement_plan_rejected_missing_merge_count =
            self.guarded_tail_replacement_plan_rejected_missing_merge_count;
        stats.guarded_tail_replacement_plan_rejected_unstable_read_count =
            self.guarded_tail_replacement_plan_rejected_unstable_read_count;
        stats.guarded_tail_exported_binding_count = self.guarded_tail_exported_binding_count;
        stats.guarded_tail_replacement_read_count = self.guarded_tail_replacement_read_count;
        stats.guarded_tail_replacement_read_rewritten_count =
            self.guarded_tail_replacement_read_rewritten_count;
        stats.guarded_tail_replacement_read_rejected_nondominated_count =
            self.guarded_tail_replacement_read_rejected_nondominated_count;
        stats.guarded_tail_replacement_read_rejected_nonremovable_op_count =
            self.guarded_tail_replacement_read_rejected_nonremovable_op_count;
        stats.discovery_rejected_noncanonical_layout_count =
            self.discovery_rejected_noncanonical_layout_count;
        stats.canonicalized_guarded_tail_shape_count = self.canonicalized_guarded_tail_shape_count;
        stats.canonicalization_failed_multiple_payload_entries =
            self.canonicalization_failed_multiple_payload_entries;
        stats.canonicalization_failed_interleaved_join_uses =
            self.canonicalization_failed_interleaved_join_uses;
        stats.canonicalization_failed_interleaved_join_uses_no_next_label_count =
            self.canonicalization_failed_interleaved_join_uses_no_next_label_count;
        stats.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count =
            self.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count;
        stats.canonicalization_failed_nonterminal_join_label =
            self.canonicalization_failed_nonterminal_join_label;
        stats.canonicalization_failed_nested_tail_escape =
            self.canonicalization_failed_nested_tail_escape;
        stats.canonicalized_interleaved_join_use_count =
            self.canonicalized_interleaved_join_use_count;
        stats.canonicalized_local_nonfallthrough_alias_count =
            self.canonicalized_local_nonfallthrough_alias_count;
        stats.canonicalization_failed_alias_not_fallthrough_count =
            self.canonicalization_failed_alias_not_fallthrough_count;
        stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count =
            self.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count;
        stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count =
            self.canonicalization_failed_alias_not_fallthrough_nested_after_label_count;
        stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count =
            self.canonicalization_failed_alias_has_multiple_internal_predecessors_count;
        stats.canonicalization_failed_alias_has_nonlocal_ref_count =
            self.canonicalization_failed_alias_has_nonlocal_ref_count;
        stats.canonicalization_failed_alias_has_nonlocal_ref_external_before_count =
            self.canonicalization_failed_alias_has_nonlocal_ref_external_before_count;
        stats.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count =
            self.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count;
        stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count =
            self.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count;
        stats.canonicalization_failed_alias_body_not_trivial_count =
            self.canonicalization_failed_alias_body_not_trivial_count;
        stats.canonicalization_failed_join_has_external_ref_count =
            self.canonicalization_failed_join_has_external_ref_count;
        stats.canonicalization_failed_payload_crosses_join_count =
            self.canonicalization_failed_payload_crosses_join_count;
        stats.rejected_must_emit_label = self.rejected_must_emit_label;
        stats.rejected_must_emit_label_surviving_middle_ref =
            self.rejected_must_emit_label_surviving_middle_ref;
        stats.rejected_must_emit_label_surviving_external_ref =
            self.rejected_must_emit_label_surviving_external_ref;
        stats.rejected_must_emit_label_owner_conflict =
            self.rejected_must_emit_label_owner_conflict;
        stats.rejected_not_single_pred_succ = self.rejected_not_single_pred_succ;
        stats.rejected_external_entry = self.rejected_external_entry;
        stats.rejected_loop_or_switch_target = self.rejected_loop_or_switch_target;
        stats.condition_fold_and_count = self.condition_fold_and_count;
        stats.condition_fold_or_count = self.condition_fold_or_count;
        stats.condition_fold_rejected_side_effect = self.condition_fold_rejected_side_effect;
        stats.structuring_sese_orphan_goto_fallback_count =
            self.structuring_sese_orphan_goto_fallback_count;
        stats.structuring_orphan_goto_localized_count =
            self.structuring_orphan_goto_localized_count;
        stats.structuring_orphan_goto_unrepairable_count =
            self.structuring_orphan_goto_unrepairable_count;
        stats.sese_child_localized_linear_count = self.sese_child_localized_linear_count;
        stats.structuring_select_bad_edge_count = self.structuring_select_bad_edge_count;
    }
}
