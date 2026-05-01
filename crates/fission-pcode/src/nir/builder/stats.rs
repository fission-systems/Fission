use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn preview_build_stats(&self) -> PreviewBuildStats {
        PreviewBuildStats {
            build_duration_ms: self.build_duration_ms,
            normalize_duration_ms: self.normalize_duration_ms,
            structuring_duration_ms: self.structuring_duration_ms,
            render_duration_ms: self.render_duration_ms,
            rendered_code_len: self.rendered_code_len,
            max_structuring_scc_component_size: self.max_structuring_scc_component_size,
            invalid_pcode_shape_count: 0,
            validated_pcode_op_count: self
                .pcode
                .blocks
                .iter()
                .map(|block| block.ops.len())
                .sum(),
            raw_pcode_compat_import_count: 0,
            ghidra_action_stage_count: 0,
            ghidra_action_funcdata_build_count: 0,
            ghidra_action_heritage_value_recovery_count: 0,
            ghidra_action_normalize_count: 0,
            ghidra_action_prototype_types_count: 0,
            ghidra_action_blockgraph_structuring_count: 0,
            ghidra_action_printc_count: 0,
            ghidra_clean_room_pipeline_complete_count: 0,
            mir_enabled_count: 0,
            mir_function_count: 0,
            mir_block_count: 0,
            mir_value_count: 0,
            mir_memory_region_count: 0,
            mir_join_proof_count: 0,
            mir_region_proof_count: 0,
            mir_projection_duration_ms: 0,
            mir_blockgraph_admission_enabled_count: self.mir_blockgraph_admission_enabled_count,
            mir_blockgraph_irreducible_budget_bypass_count: self
                .mir_blockgraph_irreducible_budget_bypass_count,
            mir_blockgraph_extreme_budget_blocked_count: self
                .mir_blockgraph_extreme_budget_blocked_count,
            procedure_summary_contracted_count: self.procedure_summary_contracted_count,
            procedure_summary_tail_wrapper_count: self.procedure_summary_tail_wrapper_count,
            procedure_summary_import_thunk_count: self.procedure_summary_import_thunk_count,
            forced_linear_structuring_count: self.forced_linear_structuring_count,
            structuring_force_linear_explicit_count: self.structuring_force_linear_explicit_count,
            structuring_force_linear_irreducible_budget_count: self
                .structuring_force_linear_irreducible_budget_count,
            structuring_force_linear_extreme_budget_count: self
                .structuring_force_linear_extreme_budget_count,
            region_linearize_structuring_count: self.region_linearize_structuring_count,
            region_linearize_rejected_non_structuring_failure_count: self
                .region_linearize_rejected_non_structuring_failure_count,
            region_linearize_rejected_no_exit_count: self.region_linearize_rejected_no_exit_count,
            region_linearize_rejected_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count,
            region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
            region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
            region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count,
            region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count: self
                .region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
            region_linearize_rejected_body_lowering_successor_inline_rejected_count: self
                .region_linearize_rejected_body_lowering_successor_inline_rejected_count,
            region_linearize_rejected_body_lowering_revisit_cycle_count: self
                .region_linearize_rejected_body_lowering_revisit_cycle_count,
            region_linearize_rejected_body_lowering_unsupported_terminator_count: self
                .region_linearize_rejected_body_lowering_unsupported_terminator_count,
            region_linearize_rejected_non_advancing_count: self
                .region_linearize_rejected_non_advancing_count,
            region_linearize_rejected_irreducible_cfg_count: self
                .region_linearize_rejected_irreducible_cfg_count,
            structuring_scc_component_count: self.structuring_scc_component_count,
            structuring_irreducible_scc_count: self.structuring_irreducible_scc_count,
            rule_block_if_no_exit_count: self.rule_block_if_no_exit_count,
            rule_block_if_no_exit_accepted_count: self.rule_block_if_no_exit_accepted_count,
            structuring_irreducible_header_count: self.structuring_irreducible_header_count,
            loop_control_explicit_reducer_count: self.loop_control_explicit_reducer_count,
            loop_control_rewrite_break_count: self.loop_control_rewrite_break_count,
            loop_control_rewrite_continue_count: self.loop_control_rewrite_continue_count,
            loop_control_rewrite_skipped_nested_scope_count: self
                .loop_control_rewrite_skipped_nested_scope_count,
            loop_while_subgraph_lowered_count: self.loop_while_subgraph_lowered_count,
            loop_multi_exit_break_count: self.loop_multi_exit_break_count,
            loop_for_lowered_count: self.loop_for_lowered_count,
            region_proof_candidate_count: self.region_proof_candidate_count,
            region_proof_completed_count: self.region_proof_completed_count,
            region_emit_ready_failed_count: self.region_emit_ready_failed_count,
            blockgraph_region_candidate_count: self.blockgraph_region_candidate_count,
            blockgraph_region_complete_count: self.blockgraph_region_complete_count,
            blockgraph_region_rejected_missing_follow_count: self
                .blockgraph_region_rejected_missing_follow_count,
            blockgraph_region_rejected_must_emit_label_count: self
                .blockgraph_region_rejected_must_emit_label_count,
            blockgraph_region_rejected_middle_ref_count: self
                .blockgraph_region_rejected_middle_ref_count,
            blockgraph_region_rejected_external_ref_count: self
                .blockgraph_region_rejected_external_ref_count,
            blockgraph_region_rejected_join_owner_conflict_count: self
                .blockgraph_region_rejected_join_owner_conflict_count,
            blockgraph_region_rejected_nonterminal_join_count: self
                .blockgraph_region_rejected_nonterminal_join_count,
            blockgraph_region_rejected_follow_owner_conflict_count: self
                .blockgraph_region_rejected_follow_owner_conflict_count,
            blockgraph_region_rejected_emit_ready_count: self
                .blockgraph_region_rejected_emit_ready_count,
            blockgraph_region_rejected_irreducible_count: self
                .blockgraph_region_rejected_irreducible_count,
            conditional_region_candidate_count: self.conditional_region_candidate_count,
            conditional_region_promoted_count: self.conditional_region_promoted_count,
            guarded_tail_candidate_count: self.guarded_tail_candidate_count,
            guarded_tail_promoted_count: self.guarded_tail_promoted_count,
            promotion_candidate_count: self.promotion_candidate_count,
            promoted_region_count: self.promoted_region_count,
            promotion_rejected_by_shape_count: self.promotion_rejected_by_shape_count,
            promotion_rejected_by_shape_missing_terminal_join_target_count: self
                .promotion_rejected_by_shape_missing_terminal_join_target_count,
            promotion_rejected_by_shape_empty_nonterminal_tail_count: self
                .promotion_rejected_by_shape_empty_nonterminal_tail_count,
            promotion_rejected_by_gate_count: self.promotion_rejected_by_gate_count,
            discovery_seen_guarded_tail_like_shape_count: self
                .discovery_seen_guarded_tail_like_shape_count,
            guarded_tail_rejected_missing_terminal_join_count: self
                .guarded_tail_rejected_missing_terminal_join_count,
            guarded_tail_rejected_side_entry_conflict_count: self
                .guarded_tail_rejected_side_entry_conflict_count,
            guarded_tail_rejected_alias_interleave_conflict_count: self
                .guarded_tail_rejected_alias_interleave_conflict_count,
            guarded_tail_rejected_ambiguous_follow_count: self
                .guarded_tail_rejected_ambiguous_follow_count,
            guarded_tail_rejected_side_effectful_callee_count: self
                .guarded_tail_rejected_side_effectful_callee_count,
            guarded_tail_replacement_plan_candidate_count: self
                .guarded_tail_replacement_plan_candidate_count,
            guarded_tail_replacement_plan_completed_count: self
                .guarded_tail_replacement_plan_completed_count,
            guarded_tail_replacement_plan_merge_created_count: self
                .guarded_tail_replacement_plan_merge_created_count,
            guarded_tail_replacement_plan_rejected_missing_merge_count: self
                .guarded_tail_replacement_plan_rejected_missing_merge_count,
            guarded_tail_replacement_plan_rejected_unstable_read_count: self
                .guarded_tail_replacement_plan_rejected_unstable_read_count,
            guarded_tail_exported_binding_count: self.guarded_tail_exported_binding_count,
            guarded_tail_replacement_read_count: self.guarded_tail_replacement_read_count,
            guarded_tail_replacement_read_rewritten_count: self
                .guarded_tail_replacement_read_rewritten_count,
            guarded_tail_replacement_read_rejected_nondominated_count: self
                .guarded_tail_replacement_read_rejected_nondominated_count,
            guarded_tail_replacement_read_rejected_nonremovable_op_count: self
                .guarded_tail_replacement_read_rejected_nonremovable_op_count,
            discovery_rejected_noncanonical_layout_count: self
                .discovery_rejected_noncanonical_layout_count,
            canonicalized_guarded_tail_shape_count: self.canonicalized_guarded_tail_shape_count,
            canonicalization_failed_multiple_payload_entries: self
                .canonicalization_failed_multiple_payload_entries,
            canonicalization_failed_interleaved_join_uses: self
                .canonicalization_failed_interleaved_join_uses,
            canonicalization_failed_interleaved_join_uses_no_next_label_count: self
                .canonicalization_failed_interleaved_join_uses_no_next_label_count,
            canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: self
                .canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
            canonicalization_failed_nonterminal_join_label: self
                .canonicalization_failed_nonterminal_join_label,
            canonicalization_failed_nested_tail_escape: self
                .canonicalization_failed_nested_tail_escape,
            canonicalized_interleaved_join_use_count: self.canonicalized_interleaved_join_use_count,
            canonicalized_local_nonfallthrough_alias_count: self
                .canonicalized_local_nonfallthrough_alias_count,
            canonicalization_failed_alias_not_fallthrough_count: self
                .canonicalization_failed_alias_not_fallthrough_count,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: self
                .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count: self
                .canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count: self
                .canonicalization_failed_alias_has_multiple_internal_predecessors_count,
            canonicalization_failed_alias_has_nonlocal_ref_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_count,
            canonicalization_failed_alias_has_nonlocal_ref_external_before_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: self
                .canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
            canonicalization_failed_alias_body_not_trivial_count: self
                .canonicalization_failed_alias_body_not_trivial_count,
            canonicalization_failed_join_has_external_ref_count: self
                .canonicalization_failed_join_has_external_ref_count,
            canonicalization_failed_payload_crosses_join_count: self
                .canonicalization_failed_payload_crosses_join_count,
            rejected_must_emit_label: self.rejected_must_emit_label,
            rejected_must_emit_label_surviving_middle_ref: self
                .rejected_must_emit_label_surviving_middle_ref,
            rejected_must_emit_label_surviving_external_ref: self
                .rejected_must_emit_label_surviving_external_ref,
            rejected_must_emit_label_owner_conflict: self
                .rejected_must_emit_label_owner_conflict,
            rejected_not_single_pred_succ: self.rejected_not_single_pred_succ,
            rejected_external_entry: self.rejected_external_entry,
            rejected_loop_or_switch_target: self.rejected_loop_or_switch_target,
            condition_fold_and_count: 0,
            condition_fold_or_count: 0,
            condition_fold_rejected_side_effect: 0,
            entry_param_promotion_spill_rename_count: 0,
            variadic_stack_region_fold_count: 0,
            abi_slot_recovered_count: 0,
            home_slot_promoted_count: 0,
            va_start_recovered_count: 0,
            call_signature_refined_count: 0,
            call_prototype_exact_api_arity_pruned_count: 0,
            call_prototype_unknown_target_kept_count: 0,
            call_prototype_wrapper_resolved_count: 0,
            call_prototype_signature_missing_count: 0,
            call_target_import_resolved_count: self.call_target_import_resolved_count,
            call_target_direct_symbol_resolved_count: self.call_target_direct_symbol_resolved_count,
            call_target_unresolved_sub_fallback_count: self.call_target_unresolved_sub_fallback_count,
            call_target_context_missing_count: self.call_target_context_missing_count,
            security_cookie_fold_count: 0,
            call_artifact_removed_count: 0,
            object_shape_recovered_count: 0,
            object_root_recovered_count: 0,
            typed_fact_evidence_count: 0,
            typed_fact_conflict_count: 0,
            object_root_fact_promotion_count: 0,
            typed_object_shape_refined_count: 0,
            surface_binding_promoted_count: 0,
            surface_fact_promotion_count: 0,
            prototype_summary_refined_count: 0,
            prototype_summary_round_count: 0,
            call_effect_summary_refined_count: 0,
            wrapper_summary_fold_count: 0,
            cleanup_budget_skip_count: 0,
            cleanup_family_binding_init_count: 0,
            cleanup_family_stmt_canonical_count: 0,
            cleanup_stmt_fold_count: 0,
            cleanup_boundary_label_count: 0,
            cleanup_loopish_rewrite_count: 0,
            cleanup_family_dead_binding_count: 0,
            interproc_signature_constraint_rounds: 0,
            unsupported_indirect_control_count: self.unsupported_indirect_control_count,
            unsupported_indirect_call_count: self.unsupported_indirect_call_count,
            unsupported_external_target_count: self.unsupported_external_target_count,
            indirect_surface_preserved_count: self.indirect_surface_preserved_count,
            indirect_target_set_refined_count: self.indirect_target_set_refined_count,
            dispatcher_shape_recovered_count: self.dispatcher_shape_recovered_count,
            materialization_stabilized_count: self.materialization_stabilized_count,
            replacement_plan_candidate_count: self.replacement_plan_candidate_count,
            replacement_plan_completed_count: self.replacement_plan_completed_count,
            replacement_plan_merge_binding_count: self.replacement_plan_merge_binding_count,
            replacement_plan_rejected_alias_unsafe_count: self
                .replacement_plan_rejected_alias_unsafe_count,
            replacement_plan_rejected_missing_merge_count: self
                .replacement_plan_rejected_missing_merge_count,
            replacement_plan_rejected_representative_root_attribution_count: self
                .replacement_plan_rejected_representative_root_attribution_count,
            replacement_plan_rejected_temp_only_representative_lifecycle_count: self
                .replacement_plan_rejected_temp_only_representative_lifecycle_count,
            replacement_plan_rejected_dead_temp_representative_count: self
                .replacement_plan_rejected_dead_temp_representative_count,
            materialization_inline_suppressed_count: self.materialization_inline_suppressed_count,
            representative_downgrade_count: self.representative_downgrade_count,
            representative_downgrade_no_aliassafe_source_count: self
                .representative_downgrade_no_aliassafe_source_count,
            representative_downgrade_join_conflict_count: self
                .representative_downgrade_join_conflict_count,
            preserved_temp_prune_blocked_count: 0,
            preserved_temp_copyprop_skip_count: 0,
            gvn_join_preserved_count: 0,
            dispatcher_proof_unit_count: self.dispatcher_proof_unit_count,
            dispatcher_proof_completed_count: self.dispatcher_proof_completed_count,
            dispatcher_proof_failed_count: self.dispatcher_proof_failed_count,
            switch_emit_ready_failed_count: self.switch_emit_ready_failed_count,
            compare_chain_dispatcher_count: 0,
            candidate_scoped_jump_resolver_count: 0,
            sccp_skipped_by_admission_count: 0,
            wide_dead_assignment_rerun_admitted_count: 0,
            wide_dead_assignment_rerun_skipped_by_admission_count: 0,
            pe_admission_profile_mismatch_count: self.pe_admission_profile_mismatch_count,
            memory_fact_prefilter_skip_count: 0,
            aggregate_fields_skipped_by_admission_count: 0,
            memory_slot_cheap_exit_count: 0,
            proof_payload_direct_emit_count: self.proof_payload_direct_emit_count,
            pass_rerun_skipped_by_preservation_count: 0,
            structuring_reason_region_legality_count: 0,
            structuring_reason_follow_failure_count: 0,
            structuring_reason_irreducible_count: 0,
            structuring_reason_loop_exit_count: 0,
            structuring_reason_switch_shape_count: 0,
            structuring_reason_budget_count: 0,
            pass_metrics: std::collections::BTreeMap::new(),
        }
    }
}
