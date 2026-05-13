use super::*;

#[derive(Debug, Default)]
struct BuilderStatsAggregate {
    core: CoreBuildStats,
    mir: MirAdmissionStats,
    procedure: ProcedureSummaryStats,
    structuring: StructuringStats,
    call_targets: CallTargetStats,
    indirect_control: IndirectControlStats,
    materialization: MaterializationStats,
    dispatcher: DispatcherStats,
}

#[derive(Debug, Default)]
struct CoreBuildStats {
    build_duration_ms: usize,
    normalize_duration_ms: usize,
    structuring_duration_ms: usize,
    render_duration_ms: usize,
    rendered_code_len: usize,
    max_structuring_scc_component_size: usize,
}

#[derive(Debug, Default)]
struct MirAdmissionStats {
    blockgraph_admission_enabled_count: usize,
    blockgraph_irreducible_budget_bypass_count: usize,
    blockgraph_extreme_budget_blocked_count: usize,
}

#[derive(Debug, Default)]
struct ProcedureSummaryStats {
    contracted_count: usize,
    tail_wrapper_count: usize,
    import_thunk_count: usize,
}

#[derive(Debug, Default)]
struct StructuringStats {
    forced_linear_structuring_count: usize,
    force_linear_explicit_count: usize,
    force_linear_irreducible_budget_count: usize,
    force_linear_extreme_budget_count: usize,
    region_linearize_structuring_count: usize,
    region_linearize_rejected_non_structuring_failure_count: usize,
    region_linearize_rejected_no_exit_count: usize,
    region_linearize_rejected_body_lowering_failed_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count: usize,
    region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    region_linearize_rejected_non_advancing_count: usize,
    region_linearize_rejected_irreducible_cfg_count: usize,
    scc_component_count: usize,
    irreducible_scc_count: usize,
    rule_block_if_no_exit_count: usize,
    rule_block_if_no_exit_accepted_count: usize,
    irreducible_header_count: usize,
    loop_control_explicit_reducer_count: usize,
    loop_control_rewrite_break_count: usize,
    loop_control_rewrite_continue_count: usize,
    loop_control_rewrite_skipped_nested_scope_count: usize,
    loop_while_subgraph_lowered_count: usize,
    loop_multi_exit_break_count: usize,
    loop_for_lowered_count: usize,
    region_proof_candidate_count: usize,
    region_proof_completed_count: usize,
    region_emit_ready_failed_count: usize,
    blockgraph_region_candidate_count: usize,
    blockgraph_region_complete_count: usize,
    blockgraph_region_rejected_missing_follow_count: usize,
    blockgraph_region_rejected_must_emit_label_count: usize,
    blockgraph_region_rejected_middle_ref_count: usize,
    blockgraph_region_rejected_external_ref_count: usize,
    blockgraph_region_rejected_join_owner_conflict_count: usize,
    blockgraph_region_rejected_nonterminal_join_count: usize,
    blockgraph_region_rejected_follow_owner_conflict_count: usize,
    blockgraph_region_rejected_emit_ready_count: usize,
    blockgraph_region_rejected_irreducible_count: usize,
    conditional_region_candidate_count: usize,
    conditional_region_promoted_count: usize,
    guarded_tail_candidate_count: usize,
    guarded_tail_promoted_count: usize,
    promotion_candidate_count: usize,
    promoted_region_count: usize,
    promotion_rejected_by_shape_count: usize,
    promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    promotion_rejected_by_gate_count: usize,
    discovery_seen_guarded_tail_like_shape_count: usize,
    guarded_tail_rejected_missing_terminal_join_count: usize,
    guarded_tail_rejected_side_entry_conflict_count: usize,
    guarded_tail_rejected_alias_interleave_conflict_count: usize,
    guarded_tail_rejected_ambiguous_follow_count: usize,
    guarded_tail_rejected_side_effectful_callee_count: usize,
    guarded_tail_replacement_plan_candidate_count: usize,
    guarded_tail_replacement_plan_completed_count: usize,
    guarded_tail_replacement_plan_merge_created_count: usize,
    guarded_tail_replacement_plan_rejected_missing_merge_count: usize,
    guarded_tail_replacement_plan_rejected_unstable_read_count: usize,
    guarded_tail_exported_binding_count: usize,
    guarded_tail_replacement_read_count: usize,
    guarded_tail_replacement_read_rewritten_count: usize,
    guarded_tail_replacement_read_rejected_nondominated_count: usize,
    guarded_tail_replacement_read_rejected_nonremovable_op_count: usize,
    discovery_rejected_noncanonical_layout_count: usize,
    canonicalized_guarded_tail_shape_count: usize,
    canonicalization_failed_multiple_payload_entries: usize,
    canonicalization_failed_interleaved_join_uses: usize,
    canonicalization_failed_interleaved_join_uses_no_next_label_count: usize,
    canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: usize,
    canonicalization_failed_nonterminal_join_label: usize,
    canonicalization_failed_nested_tail_escape: usize,
    canonicalized_interleaved_join_use_count: usize,
    canonicalized_local_nonfallthrough_alias_count: usize,
    canonicalization_failed_alias_not_fallthrough_count: usize,
    canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    canonicalization_failed_alias_body_not_trivial_count: usize,
    canonicalization_failed_join_has_external_ref_count: usize,
    canonicalization_failed_payload_crosses_join_count: usize,
    rejected_must_emit_label: usize,
    rejected_must_emit_label_surviving_middle_ref: usize,
    rejected_must_emit_label_surviving_external_ref: usize,
    rejected_must_emit_label_owner_conflict: usize,
    rejected_not_single_pred_succ: usize,
    rejected_external_entry: usize,
    rejected_loop_or_switch_target: usize,
}

#[derive(Debug, Default)]
struct CallTargetStats {
    import_resolved_count: usize,
    direct_symbol_resolved_count: usize,
    unresolved_sub_fallback_count: usize,
    context_missing_count: usize,
    exact_index_hit_count: usize,
    exact_index_ambiguous_count: usize,
    export_thunk_target_resolved_count: usize,
    indirect_const_resolved_count: usize,
    iat_slot_resolved_count: usize,
    indirect_load_resolved_count: usize,
    indirect_ptr_const_folded_count: usize,
    indirect_rejected_non_iat_load_count: usize,
    indirect_rejected_non_const_ptr_count: usize,
    indirect_rejected_unsupported_ptr_opcode_count: usize,
    indirect_rejected_ambiguous_def_count: usize,
    indirect_rejected_non_dominating_def_count: usize,
    indirect_rejected_no_def_count: usize,
    indirect_rejected_width_mismatch_count: usize,
    unresolved_no_exact_identity_count: usize,
}

#[derive(Debug, Default)]
struct IndirectControlStats {
    unsupported_indirect_control_count: usize,
    unsupported_indirect_call_count: usize,
    unsupported_external_target_count: usize,
    indirect_surface_preserved_count: usize,
    indirect_target_set_refined_count: usize,
    dispatcher_shape_recovered_count: usize,
}

#[derive(Debug, Default)]
struct MaterializationStats {
    stabilized_count: usize,
    replacement_plan_candidate_count: usize,
    replacement_plan_completed_count: usize,
    replacement_plan_merge_binding_count: usize,
    replacement_plan_rejected_alias_unsafe_count: usize,
    replacement_plan_rejected_missing_merge_count: usize,
    replacement_plan_rejected_representative_root_attribution_count: usize,
    replacement_plan_rejected_temp_only_representative_lifecycle_count: usize,
    replacement_plan_rejected_dead_temp_representative_count: usize,
    inline_suppressed_count: usize,
    representative_downgrade_count: usize,
    representative_downgrade_no_aliassafe_source_count: usize,
    representative_downgrade_join_conflict_count: usize,
}

#[derive(Debug, Default)]
struct DispatcherStats {
    proof_unit_count: usize,
    proof_completed_count: usize,
    proof_failed_count: usize,
    switch_emit_ready_failed_count: usize,
    pe_admission_profile_mismatch_count: usize,
    proof_payload_direct_emit_count: usize,
}

impl<'a> From<&PreviewBuilder<'a>> for BuilderStatsAggregate {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            core: CoreBuildStats::from(builder),
            mir: MirAdmissionStats::from(builder),
            procedure: ProcedureSummaryStats::from(builder),
            structuring: StructuringStats::from(builder),
            call_targets: CallTargetStats::from(builder),
            indirect_control: IndirectControlStats::from(builder),
            materialization: MaterializationStats::from(builder),
            dispatcher: DispatcherStats::from(builder),
        }
    }
}

impl BuilderStatsAggregate {
    fn into_public_stats(self, validated_pcode_op_count: usize) -> PreviewBuildStats {
        let mut stats = PreviewBuildStats {
            validated_pcode_op_count,
            ..PreviewBuildStats::default()
        };
        self.core.apply_to(&mut stats);
        self.mir.apply_to(&mut stats);
        self.procedure.apply_to(&mut stats);
        self.structuring.apply_to(&mut stats);
        self.call_targets.apply_to(&mut stats);
        self.indirect_control.apply_to(&mut stats);
        self.materialization.apply_to(&mut stats);
        self.dispatcher.apply_to(&mut stats);
        stats
    }
}

impl<'a> From<&PreviewBuilder<'a>> for CoreBuildStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            build_duration_ms: builder.build_duration_ms,
            normalize_duration_ms: builder.normalize_duration_ms,
            structuring_duration_ms: builder.structuring_duration_ms,
            render_duration_ms: builder.render_duration_ms,
            rendered_code_len: builder.rendered_code_len,
            max_structuring_scc_component_size: builder.max_structuring_scc_component_size,
        }
    }
}

impl CoreBuildStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.build_duration_ms = self.build_duration_ms;
        stats.normalize_duration_ms = self.normalize_duration_ms;
        stats.structuring_duration_ms = self.structuring_duration_ms;
        stats.render_duration_ms = self.render_duration_ms;
        stats.rendered_code_len = self.rendered_code_len;
        stats.max_structuring_scc_component_size = self.max_structuring_scc_component_size;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for MirAdmissionStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            blockgraph_admission_enabled_count: builder.mir_blockgraph_admission_enabled_count,
            blockgraph_irreducible_budget_bypass_count: builder
                .mir_blockgraph_irreducible_budget_bypass_count,
            blockgraph_extreme_budget_blocked_count: builder
                .mir_blockgraph_extreme_budget_blocked_count,
        }
    }
}

impl MirAdmissionStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.mir_blockgraph_admission_enabled_count = self.blockgraph_admission_enabled_count;
        stats.mir_blockgraph_irreducible_budget_bypass_count =
            self.blockgraph_irreducible_budget_bypass_count;
        stats.mir_blockgraph_extreme_budget_blocked_count =
            self.blockgraph_extreme_budget_blocked_count;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for ProcedureSummaryStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            contracted_count: builder.procedure_summary_contracted_count,
            tail_wrapper_count: builder.procedure_summary_tail_wrapper_count,
            import_thunk_count: builder.procedure_summary_import_thunk_count,
        }
    }
}

impl ProcedureSummaryStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.procedure_summary_contracted_count = self.contracted_count;
        stats.procedure_summary_tail_wrapper_count = self.tail_wrapper_count;
        stats.procedure_summary_import_thunk_count = self.import_thunk_count;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for StructuringStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            forced_linear_structuring_count: builder.forced_linear_structuring_count,
            force_linear_explicit_count: builder.structuring_force_linear_explicit_count,
            force_linear_irreducible_budget_count: builder
                .structuring_force_linear_irreducible_budget_count,
            force_linear_extreme_budget_count: builder
                .structuring_force_linear_extreme_budget_count,
            region_linearize_structuring_count: builder.region_linearize_structuring_count,
            region_linearize_rejected_non_structuring_failure_count: builder
                .region_linearize_rejected_non_structuring_failure_count,
            region_linearize_rejected_no_exit_count: builder
                .region_linearize_rejected_no_exit_count,
            region_linearize_rejected_body_lowering_failed_count: builder
                .region_linearize_rejected_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count,
            region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
            region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
            region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count,
            region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count: builder
                .region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
            region_linearize_rejected_body_lowering_successor_inline_rejected_count: builder
                .region_linearize_rejected_body_lowering_successor_inline_rejected_count,
            region_linearize_rejected_body_lowering_revisit_cycle_count: builder
                .region_linearize_rejected_body_lowering_revisit_cycle_count,
            region_linearize_rejected_body_lowering_unsupported_terminator_count: builder
                .region_linearize_rejected_body_lowering_unsupported_terminator_count,
            region_linearize_rejected_non_advancing_count: builder
                .region_linearize_rejected_non_advancing_count,
            region_linearize_rejected_irreducible_cfg_count: builder
                .region_linearize_rejected_irreducible_cfg_count,
            scc_component_count: builder.structuring_scc_component_count,
            irreducible_scc_count: builder.structuring_irreducible_scc_count,
            rule_block_if_no_exit_count: builder.rule_block_if_no_exit_count,
            rule_block_if_no_exit_accepted_count: builder.rule_block_if_no_exit_accepted_count,
            irreducible_header_count: builder.structuring_irreducible_header_count,
            loop_control_explicit_reducer_count: builder.loop_control_explicit_reducer_count,
            loop_control_rewrite_break_count: builder.loop_control_rewrite_break_count,
            loop_control_rewrite_continue_count: builder.loop_control_rewrite_continue_count,
            loop_control_rewrite_skipped_nested_scope_count: builder
                .loop_control_rewrite_skipped_nested_scope_count,
            loop_while_subgraph_lowered_count: builder.loop_while_subgraph_lowered_count,
            loop_multi_exit_break_count: builder.loop_multi_exit_break_count,
            loop_for_lowered_count: builder.loop_for_lowered_count,
            region_proof_candidate_count: builder.region_proof_candidate_count,
            region_proof_completed_count: builder.region_proof_completed_count,
            region_emit_ready_failed_count: builder.region_emit_ready_failed_count,
            blockgraph_region_candidate_count: builder.blockgraph_region_candidate_count,
            blockgraph_region_complete_count: builder.blockgraph_region_complete_count,
            blockgraph_region_rejected_missing_follow_count: builder
                .blockgraph_region_rejected_missing_follow_count,
            blockgraph_region_rejected_must_emit_label_count: builder
                .blockgraph_region_rejected_must_emit_label_count,
            blockgraph_region_rejected_middle_ref_count: builder
                .blockgraph_region_rejected_middle_ref_count,
            blockgraph_region_rejected_external_ref_count: builder
                .blockgraph_region_rejected_external_ref_count,
            blockgraph_region_rejected_join_owner_conflict_count: builder
                .blockgraph_region_rejected_join_owner_conflict_count,
            blockgraph_region_rejected_nonterminal_join_count: builder
                .blockgraph_region_rejected_nonterminal_join_count,
            blockgraph_region_rejected_follow_owner_conflict_count: builder
                .blockgraph_region_rejected_follow_owner_conflict_count,
            blockgraph_region_rejected_emit_ready_count: builder
                .blockgraph_region_rejected_emit_ready_count,
            blockgraph_region_rejected_irreducible_count: builder
                .blockgraph_region_rejected_irreducible_count,
            conditional_region_candidate_count: builder.conditional_region_candidate_count,
            conditional_region_promoted_count: builder.conditional_region_promoted_count,
            guarded_tail_candidate_count: builder.guarded_tail_candidate_count,
            guarded_tail_promoted_count: builder.guarded_tail_promoted_count,
            promotion_candidate_count: builder.promotion_candidate_count,
            promoted_region_count: builder.promoted_region_count,
            promotion_rejected_by_shape_count: builder.promotion_rejected_by_shape_count,
            promotion_rejected_by_shape_missing_terminal_join_target_count: builder
                .promotion_rejected_by_shape_missing_terminal_join_target_count,
            promotion_rejected_by_shape_empty_nonterminal_tail_count: builder
                .promotion_rejected_by_shape_empty_nonterminal_tail_count,
            promotion_rejected_by_gate_count: builder.promotion_rejected_by_gate_count,
            discovery_seen_guarded_tail_like_shape_count: builder
                .discovery_seen_guarded_tail_like_shape_count,
            guarded_tail_rejected_missing_terminal_join_count: builder
                .guarded_tail_rejected_missing_terminal_join_count,
            guarded_tail_rejected_side_entry_conflict_count: builder
                .guarded_tail_rejected_side_entry_conflict_count,
            guarded_tail_rejected_alias_interleave_conflict_count: builder
                .guarded_tail_rejected_alias_interleave_conflict_count,
            guarded_tail_rejected_ambiguous_follow_count: builder
                .guarded_tail_rejected_ambiguous_follow_count,
            guarded_tail_rejected_side_effectful_callee_count: builder
                .guarded_tail_rejected_side_effectful_callee_count,
            guarded_tail_replacement_plan_candidate_count: builder
                .guarded_tail_replacement_plan_candidate_count,
            guarded_tail_replacement_plan_completed_count: builder
                .guarded_tail_replacement_plan_completed_count,
            guarded_tail_replacement_plan_merge_created_count: builder
                .guarded_tail_replacement_plan_merge_created_count,
            guarded_tail_replacement_plan_rejected_missing_merge_count: builder
                .guarded_tail_replacement_plan_rejected_missing_merge_count,
            guarded_tail_replacement_plan_rejected_unstable_read_count: builder
                .guarded_tail_replacement_plan_rejected_unstable_read_count,
            guarded_tail_exported_binding_count: builder.guarded_tail_exported_binding_count,
            guarded_tail_replacement_read_count: builder.guarded_tail_replacement_read_count,
            guarded_tail_replacement_read_rewritten_count: builder
                .guarded_tail_replacement_read_rewritten_count,
            guarded_tail_replacement_read_rejected_nondominated_count: builder
                .guarded_tail_replacement_read_rejected_nondominated_count,
            guarded_tail_replacement_read_rejected_nonremovable_op_count: builder
                .guarded_tail_replacement_read_rejected_nonremovable_op_count,
            discovery_rejected_noncanonical_layout_count: builder
                .discovery_rejected_noncanonical_layout_count,
            canonicalized_guarded_tail_shape_count: builder.canonicalized_guarded_tail_shape_count,
            canonicalization_failed_multiple_payload_entries: builder
                .canonicalization_failed_multiple_payload_entries,
            canonicalization_failed_interleaved_join_uses: builder
                .canonicalization_failed_interleaved_join_uses,
            canonicalization_failed_interleaved_join_uses_no_next_label_count: builder
                .canonicalization_failed_interleaved_join_uses_no_next_label_count,
            canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: builder
                .canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
            canonicalization_failed_nonterminal_join_label: builder
                .canonicalization_failed_nonterminal_join_label,
            canonicalization_failed_nested_tail_escape: builder
                .canonicalization_failed_nested_tail_escape,
            canonicalized_interleaved_join_use_count: builder
                .canonicalized_interleaved_join_use_count,
            canonicalized_local_nonfallthrough_alias_count: builder
                .canonicalized_local_nonfallthrough_alias_count,
            canonicalization_failed_alias_not_fallthrough_count: builder
                .canonicalization_failed_alias_not_fallthrough_count,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: builder
                .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count: builder
                .canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count: builder
                .canonicalization_failed_alias_has_multiple_internal_predecessors_count,
            canonicalization_failed_alias_has_nonlocal_ref_count: builder
                .canonicalization_failed_alias_has_nonlocal_ref_count,
            canonicalization_failed_alias_has_nonlocal_ref_external_before_count: builder
                .canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: builder
                .canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: builder
                .canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
            canonicalization_failed_alias_body_not_trivial_count: builder
                .canonicalization_failed_alias_body_not_trivial_count,
            canonicalization_failed_join_has_external_ref_count: builder
                .canonicalization_failed_join_has_external_ref_count,
            canonicalization_failed_payload_crosses_join_count: builder
                .canonicalization_failed_payload_crosses_join_count,
            rejected_must_emit_label: builder.rejected_must_emit_label,
            rejected_must_emit_label_surviving_middle_ref: builder
                .rejected_must_emit_label_surviving_middle_ref,
            rejected_must_emit_label_surviving_external_ref: builder
                .rejected_must_emit_label_surviving_external_ref,
            rejected_must_emit_label_owner_conflict: builder.rejected_must_emit_label_owner_conflict,
            rejected_not_single_pred_succ: builder.rejected_not_single_pred_succ,
            rejected_external_entry: builder.rejected_external_entry,
            rejected_loop_or_switch_target: builder.rejected_loop_or_switch_target,
        }
    }
}

impl StructuringStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.forced_linear_structuring_count = self.forced_linear_structuring_count;
        stats.structuring_force_linear_explicit_count = self.force_linear_explicit_count;
        stats.structuring_force_linear_irreducible_budget_count =
            self.force_linear_irreducible_budget_count;
        stats.structuring_force_linear_extreme_budget_count =
            self.force_linear_extreme_budget_count;
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
        stats.structuring_scc_component_count = self.scc_component_count;
        stats.structuring_irreducible_scc_count = self.irreducible_scc_count;
        stats.rule_block_if_no_exit_count = self.rule_block_if_no_exit_count;
        stats.rule_block_if_no_exit_accepted_count = self.rule_block_if_no_exit_accepted_count;
        stats.structuring_irreducible_header_count = self.irreducible_header_count;
        stats.loop_control_explicit_reducer_count = self.loop_control_explicit_reducer_count;
        stats.loop_control_rewrite_break_count = self.loop_control_rewrite_break_count;
        stats.loop_control_rewrite_continue_count = self.loop_control_rewrite_continue_count;
        stats.loop_control_rewrite_skipped_nested_scope_count =
            self.loop_control_rewrite_skipped_nested_scope_count;
        stats.loop_while_subgraph_lowered_count = self.loop_while_subgraph_lowered_count;
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
    }
}

impl<'a> From<&PreviewBuilder<'a>> for CallTargetStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            import_resolved_count: builder.call_target_import_resolved_count,
            direct_symbol_resolved_count: builder.call_target_direct_symbol_resolved_count,
            unresolved_sub_fallback_count: builder.call_target_unresolved_sub_fallback_count,
            context_missing_count: builder.call_target_context_missing_count,
            exact_index_hit_count: builder.call_target_exact_index_hit_count,
            exact_index_ambiguous_count: builder.call_target_exact_index_ambiguous_count,
            export_thunk_target_resolved_count: builder
                .call_target_export_thunk_target_resolved_count,
            indirect_const_resolved_count: builder.call_target_indirect_const_resolved_count,
            iat_slot_resolved_count: builder.call_target_iat_slot_resolved_count,
            indirect_load_resolved_count: builder.call_target_indirect_load_resolved_count,
            indirect_ptr_const_folded_count: builder.call_target_indirect_ptr_const_folded_count,
            indirect_rejected_non_iat_load_count: builder
                .call_target_indirect_rejected_non_iat_load_count,
            indirect_rejected_non_const_ptr_count: builder
                .call_target_indirect_rejected_non_const_ptr_count,
            indirect_rejected_unsupported_ptr_opcode_count: builder
                .call_target_indirect_rejected_unsupported_ptr_opcode_count,
            indirect_rejected_ambiguous_def_count: builder
                .call_target_indirect_rejected_ambiguous_def_count,
            indirect_rejected_non_dominating_def_count: builder
                .call_target_indirect_rejected_non_dominating_def_count,
            indirect_rejected_no_def_count: builder.call_target_indirect_rejected_no_def_count,
            indirect_rejected_width_mismatch_count: builder
                .call_target_indirect_rejected_width_mismatch_count,
            unresolved_no_exact_identity_count: builder
                .call_target_unresolved_no_exact_identity_count,
        }
    }
}

impl CallTargetStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.call_target_import_resolved_count = self.import_resolved_count;
        stats.call_target_direct_symbol_resolved_count = self.direct_symbol_resolved_count;
        stats.call_target_unresolved_sub_fallback_count = self.unresolved_sub_fallback_count;
        stats.call_target_context_missing_count = self.context_missing_count;
        stats.call_target_exact_index_hit_count = self.exact_index_hit_count;
        stats.call_target_exact_index_ambiguous_count = self.exact_index_ambiguous_count;
        stats.call_target_export_thunk_target_resolved_count =
            self.export_thunk_target_resolved_count;
        stats.call_target_indirect_const_resolved_count = self.indirect_const_resolved_count;
        stats.call_target_iat_slot_resolved_count = self.iat_slot_resolved_count;
        stats.call_target_indirect_load_resolved_count = self.indirect_load_resolved_count;
        stats.call_target_indirect_ptr_const_folded_count = self.indirect_ptr_const_folded_count;
        stats.call_target_indirect_rejected_non_iat_load_count =
            self.indirect_rejected_non_iat_load_count;
        stats.call_target_indirect_rejected_non_const_ptr_count =
            self.indirect_rejected_non_const_ptr_count;
        stats.call_target_indirect_rejected_unsupported_ptr_opcode_count =
            self.indirect_rejected_unsupported_ptr_opcode_count;
        stats.call_target_indirect_rejected_ambiguous_def_count =
            self.indirect_rejected_ambiguous_def_count;
        stats.call_target_indirect_rejected_non_dominating_def_count =
            self.indirect_rejected_non_dominating_def_count;
        stats.call_target_indirect_rejected_no_def_count = self.indirect_rejected_no_def_count;
        stats.call_target_indirect_rejected_width_mismatch_count =
            self.indirect_rejected_width_mismatch_count;
        stats.call_target_unresolved_no_exact_identity_count =
            self.unresolved_no_exact_identity_count;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for IndirectControlStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            unsupported_indirect_control_count: builder.unsupported_indirect_control_count,
            unsupported_indirect_call_count: builder.unsupported_indirect_call_count,
            unsupported_external_target_count: builder.unsupported_external_target_count,
            indirect_surface_preserved_count: builder.indirect_surface_preserved_count,
            indirect_target_set_refined_count: builder.indirect_target_set_refined_count,
            dispatcher_shape_recovered_count: builder.dispatcher_shape_recovered_count,
        }
    }
}

impl IndirectControlStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.unsupported_indirect_control_count = self.unsupported_indirect_control_count;
        stats.unsupported_indirect_call_count = self.unsupported_indirect_call_count;
        stats.unsupported_external_target_count = self.unsupported_external_target_count;
        stats.indirect_surface_preserved_count = self.indirect_surface_preserved_count;
        stats.indirect_target_set_refined_count = self.indirect_target_set_refined_count;
        stats.dispatcher_shape_recovered_count = self.dispatcher_shape_recovered_count;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for MaterializationStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            stabilized_count: builder.materialization_stabilized_count,
            replacement_plan_candidate_count: builder.replacement_plan_candidate_count,
            replacement_plan_completed_count: builder.replacement_plan_completed_count,
            replacement_plan_merge_binding_count: builder.replacement_plan_merge_binding_count,
            replacement_plan_rejected_alias_unsafe_count: builder
                .replacement_plan_rejected_alias_unsafe_count,
            replacement_plan_rejected_missing_merge_count: builder
                .replacement_plan_rejected_missing_merge_count,
            replacement_plan_rejected_representative_root_attribution_count: builder
                .replacement_plan_rejected_representative_root_attribution_count,
            replacement_plan_rejected_temp_only_representative_lifecycle_count: builder
                .replacement_plan_rejected_temp_only_representative_lifecycle_count,
            replacement_plan_rejected_dead_temp_representative_count: builder
                .replacement_plan_rejected_dead_temp_representative_count,
            inline_suppressed_count: builder.materialization_inline_suppressed_count,
            representative_downgrade_count: builder.representative_downgrade_count,
            representative_downgrade_no_aliassafe_source_count: builder
                .representative_downgrade_no_aliassafe_source_count,
            representative_downgrade_join_conflict_count: builder
                .representative_downgrade_join_conflict_count,
        }
    }
}

impl MaterializationStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.materialization_stabilized_count = self.stabilized_count;
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
        stats.materialization_inline_suppressed_count = self.inline_suppressed_count;
        stats.representative_downgrade_count = self.representative_downgrade_count;
        stats.representative_downgrade_no_aliassafe_source_count =
            self.representative_downgrade_no_aliassafe_source_count;
        stats.representative_downgrade_join_conflict_count =
            self.representative_downgrade_join_conflict_count;
    }
}

impl<'a> From<&PreviewBuilder<'a>> for DispatcherStats {
    fn from(builder: &PreviewBuilder<'a>) -> Self {
        Self {
            proof_unit_count: builder.dispatcher_proof_unit_count,
            proof_completed_count: builder.dispatcher_proof_completed_count,
            proof_failed_count: builder.dispatcher_proof_failed_count,
            switch_emit_ready_failed_count: builder.switch_emit_ready_failed_count,
            pe_admission_profile_mismatch_count: builder.pe_admission_profile_mismatch_count,
            proof_payload_direct_emit_count: builder.proof_payload_direct_emit_count,
        }
    }
}

impl DispatcherStats {
    fn apply_to(self, stats: &mut PreviewBuildStats) {
        stats.dispatcher_proof_unit_count = self.proof_unit_count;
        stats.dispatcher_proof_completed_count = self.proof_completed_count;
        stats.dispatcher_proof_failed_count = self.proof_failed_count;
        stats.switch_emit_ready_failed_count = self.switch_emit_ready_failed_count;
        stats.pe_admission_profile_mismatch_count = self.pe_admission_profile_mismatch_count;
        stats.proof_payload_direct_emit_count = self.proof_payload_direct_emit_count;
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn preview_build_stats(&self) -> PreviewBuildStats {
        let validated_pcode_op_count = self.pcode.blocks.iter().map(|block| block.ops.len()).sum();
        BuilderStatsAggregate::from(self).into_public_stats(validated_pcode_op_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_stats_projection_keeps_public_nir_build_stats_field_set() {
        let default_keys = serialized_keys(&PreviewBuildStats::default());
        let projected_keys =
            serialized_keys(&BuilderStatsAggregate::default().into_public_stats(17));
        assert_eq!(projected_keys, default_keys);
    }

    fn serialized_keys(stats: &PreviewBuildStats) -> Vec<String> {
        let serde_json::Value::Object(object) =
            serde_json::to_value(stats).expect("serialize NirBuildStats")
        else {
            panic!("NirBuildStats must serialize as an object");
        };
        object.keys().cloned().collect()
    }
}
