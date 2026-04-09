use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn preview_build_stats(&self) -> PreviewBuildStats {
        PreviewBuildStats {
            build_duration_ms: self.build_duration_ms,
            normalize_duration_ms: self.normalize_duration_ms,
            forced_linear_structuring_count: self.forced_linear_structuring_count,
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
            interproc_signature_constraint_rounds: 0,
        }
    }
}
