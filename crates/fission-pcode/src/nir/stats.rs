//! Internal implementation for the flat public [`NirBuildStats`] telemetry contract.
//!
//! The serialized shape remains owned by `types.rs`. This module groups merge
//! behavior by responsibility so builder/normalize/structuring refactors do not
//! need to duplicate the public flat field list at call sites.

use super::types::{NirBuildStats, PassAggregate};

macro_rules! merge_additive_fields {
    ($target:expr, $other:expr, [$($field:ident),+ $(,)?]) => {
        $(
            $target.$field += $other.$field;
        )+
    };
}

impl NirBuildStats {
    pub fn merge_assign(&mut self, other: &Self) {
        merge_core_stats(self, other);
        merge_action_pipeline_stats(self, other);
        merge_mir_stats(self, other);
        merge_procedure_stats(self, other);
        merge_structuring_stats(self, other);
        merge_type_and_normalize_stats(self, other);
        merge_call_target_stats(self, other);
        merge_indirect_control_stats(self, other);
        merge_materialization_stats(self, other);
        merge_dispatcher_stats(self, other);
        merge_admission_stats(self, other);
        merge_structuring_reason_stats(self, other);
        merge_pass_metrics(&mut self.pass_metrics, &other.pass_metrics);
    }

    pub(crate) fn merge_guarded_tail_discovery_assign(&mut self, other: &Self) {
        merge_guarded_tail_discovery_stats(self, other);
    }

    pub fn refresh_structuring_reason_families(&mut self) {
        self.structuring_reason_region_legality_count = self
            .region_linearize_rejected_non_structuring_failure_count
            + self.region_linearize_rejected_no_exit_count
            + self.region_linearize_rejected_body_lowering_unsupported_terminator_count
            + self.region_emit_ready_failed_count
            + self.guarded_tail_rejected_side_effectful_callee_count
            + self.guarded_tail_rejected_missing_terminal_join_count
            + self.guarded_tail_rejected_alias_interleave_conflict_count
            + self.rejected_external_entry
            + self.rejected_not_single_pred_succ;
        self.structuring_reason_follow_failure_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count
                + self.guarded_tail_rejected_ambiguous_follow_count
                + self.region_linearize_rejected_body_lowering_successor_inline_rejected_count;
        self.structuring_reason_irreducible_count = self
            .region_linearize_rejected_irreducible_cfg_count
            + self.structuring_irreducible_scc_count
            + self.structuring_irreducible_header_count;
        self.structuring_reason_loop_exit_count = self
            .region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count
            + self.guarded_tail_rejected_side_entry_conflict_count
            + self.loop_control_rewrite_skipped_nested_scope_count
            + self.rejected_loop_or_switch_target;
        self.structuring_reason_switch_shape_count = self
            .region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count
            + self.switch_emit_ready_failed_count;
        self.structuring_reason_budget_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count
                + self.region_linearize_rejected_non_advancing_count
                + self.region_linearize_rejected_body_lowering_revisit_cycle_count;
    }
}

fn merge_core_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            build_duration_ms,
            normalize_duration_ms,
            structuring_duration_ms,
            render_duration_ms,
            rendered_code_len,
            invalid_pcode_shape_count,
            validated_pcode_op_count,
            raw_pcode_compat_import_count,
        ]
    );
    target.max_structuring_scc_component_size = target
        .max_structuring_scc_component_size
        .max(other.max_structuring_scc_component_size);
}

fn merge_action_pipeline_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            ghidra_action_stage_count,
            ghidra_action_funcdata_build_count,
            ghidra_action_heritage_value_recovery_count,
            ghidra_action_normalize_count,
            ghidra_action_prototype_types_count,
            ghidra_action_blockgraph_structuring_count,
            ghidra_action_printc_count,
            ghidra_clean_room_pipeline_complete_count,
        ]
    );
}

fn merge_mir_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            mir_enabled_count,
            mir_function_count,
            mir_block_count,
            mir_value_count,
            mir_memory_region_count,
            mir_join_proof_count,
            mir_region_proof_count,
            mir_projection_duration_ms,
            mir_blockgraph_admission_enabled_count,
            mir_blockgraph_irreducible_budget_bypass_count,
            mir_blockgraph_extreme_budget_blocked_count,
        ]
    );
}

fn merge_procedure_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            procedure_summary_contracted_count,
            procedure_summary_tail_wrapper_count,
            procedure_summary_import_thunk_count,
        ]
    );
}

fn merge_structuring_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            forced_linear_structuring_count,
            structuring_force_linear_explicit_count,
            structuring_force_linear_irreducible_budget_count,
            structuring_force_linear_extreme_budget_count,
            region_linearize_structuring_count,
            region_linearize_rejected_non_structuring_failure_count,
            region_linearize_rejected_no_exit_count,
            region_linearize_rejected_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count,
            region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
            region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
            region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
            region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count,
            region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count,
            region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
            region_linearize_rejected_body_lowering_successor_inline_rejected_count,
            region_linearize_rejected_body_lowering_revisit_cycle_count,
            region_linearize_rejected_body_lowering_unsupported_terminator_count,
            region_linearize_rejected_non_advancing_count,
            region_linearize_rejected_irreducible_cfg_count,
            structuring_scc_component_count,
            structuring_irreducible_scc_count,
            rule_block_if_no_exit_count,
            rule_block_if_no_exit_accepted_count,
            structuring_irreducible_header_count,
            loop_control_explicit_reducer_count,
            loop_control_rewrite_break_count,
            loop_control_rewrite_continue_count,
            loop_control_rewrite_skipped_nested_scope_count,
            loop_while_subgraph_lowered_count,
            loop_multi_tail_dowhile_lowered_count,
            loop_multi_exit_break_count,
            loop_for_lowered_count,
            region_proof_candidate_count,
            region_proof_completed_count,
            region_emit_ready_failed_count,
            blockgraph_region_candidate_count,
            blockgraph_region_complete_count,
            blockgraph_region_rejected_missing_follow_count,
            blockgraph_region_rejected_must_emit_label_count,
            blockgraph_region_rejected_middle_ref_count,
            blockgraph_region_rejected_external_ref_count,
            blockgraph_region_rejected_join_owner_conflict_count,
            blockgraph_region_rejected_nonterminal_join_count,
            blockgraph_region_rejected_follow_owner_conflict_count,
            blockgraph_region_rejected_emit_ready_count,
            blockgraph_region_rejected_irreducible_count,
            conditional_region_candidate_count,
            conditional_region_promoted_count,
            guarded_tail_candidate_count,
            guarded_tail_promoted_count,
            promotion_candidate_count,
            promoted_region_count,
            promotion_rejected_by_shape_count,
            promotion_rejected_by_shape_missing_terminal_join_target_count,
            promotion_rejected_by_shape_empty_nonterminal_tail_count,
            promotion_rejected_by_gate_count,
            discovery_seen_guarded_tail_like_shape_count,
            guarded_tail_rejected_missing_terminal_join_count,
            guarded_tail_rejected_side_entry_conflict_count,
            guarded_tail_rejected_alias_interleave_conflict_count,
            guarded_tail_rejected_ambiguous_follow_count,
            guarded_tail_rejected_side_effectful_callee_count,
            guarded_tail_replacement_plan_candidate_count,
            guarded_tail_replacement_plan_completed_count,
            guarded_tail_replacement_plan_merge_created_count,
            guarded_tail_replacement_plan_rejected_missing_merge_count,
            guarded_tail_replacement_plan_rejected_unstable_read_count,
            guarded_tail_exported_binding_count,
            guarded_tail_replacement_read_count,
            guarded_tail_replacement_read_rewritten_count,
            guarded_tail_replacement_read_rejected_nondominated_count,
            guarded_tail_replacement_read_rejected_nonremovable_op_count,
            discovery_rejected_noncanonical_layout_count,
            canonicalized_guarded_tail_shape_count,
            canonicalization_failed_multiple_payload_entries,
            canonicalization_failed_interleaved_join_uses,
            canonicalization_failed_interleaved_join_uses_no_next_label_count,
            canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
            canonicalization_failed_nonterminal_join_label,
            canonicalization_failed_nested_tail_escape,
            canonicalized_interleaved_join_use_count,
            canonicalized_local_nonfallthrough_alias_count,
            canonicalization_failed_alias_not_fallthrough_count,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count,
            canonicalization_failed_alias_has_nonlocal_ref_count,
            canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
            canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
            canonicalization_failed_alias_body_not_trivial_count,
            canonicalization_failed_join_has_external_ref_count,
            canonicalization_failed_payload_crosses_join_count,
            rejected_must_emit_label,
            rejected_must_emit_label_surviving_middle_ref,
            rejected_must_emit_label_surviving_external_ref,
            rejected_must_emit_label_owner_conflict,
            rejected_not_single_pred_succ,
            rejected_external_entry,
            rejected_loop_or_switch_target,
            condition_fold_and_count,
            condition_fold_or_count,
            condition_fold_rejected_side_effect,
            fas_virtual_goto_count,
            switch_fallthrough_detected_count,
            structuring_sese_orphan_goto_fallback_count,
        ]
    );
}

fn merge_guarded_tail_discovery_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            promotion_candidate_count,
            promoted_region_count,
            promotion_rejected_by_shape_count,
            promotion_rejected_by_gate_count,
            discovery_seen_guarded_tail_like_shape_count,
            discovery_rejected_noncanonical_layout_count,
            canonicalized_guarded_tail_shape_count,
            canonicalization_failed_multiple_payload_entries,
            canonicalization_failed_interleaved_join_uses,
            canonicalization_failed_interleaved_join_uses_no_next_label_count,
            canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
            canonicalization_failed_nonterminal_join_label,
            canonicalization_failed_nested_tail_escape,
            canonicalized_interleaved_join_use_count,
            canonicalized_local_nonfallthrough_alias_count,
            canonicalization_failed_alias_not_fallthrough_count,
            canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
            canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
            canonicalization_failed_alias_has_multiple_internal_predecessors_count,
            canonicalization_failed_alias_has_nonlocal_ref_count,
            canonicalization_failed_alias_body_not_trivial_count,
            canonicalization_failed_join_has_external_ref_count,
            canonicalization_failed_payload_crosses_join_count,
            rejected_must_emit_label,
            rejected_not_single_pred_succ,
            rejected_external_entry,
            rejected_loop_or_switch_target,
        ]
    );
}

fn merge_type_and_normalize_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            entry_param_promotion_spill_rename_count,
            variadic_stack_region_fold_count,
            abi_slot_recovered_count,
            home_slot_promoted_count,
            va_start_recovered_count,
            call_signature_refined_count,
            call_prototype_exact_api_arity_pruned_count,
            call_prototype_unknown_target_kept_count,
            call_prototype_wrapper_resolved_count,
            call_prototype_signature_missing_count,
            security_cookie_fold_count,
            call_artifact_removed_count,
            object_shape_recovered_count,
            object_root_recovered_count,
            typed_fact_evidence_count,
            typed_fact_conflict_count,
            object_root_fact_promotion_count,
            typed_object_shape_refined_count,
            surface_binding_promoted_count,
            surface_fact_promotion_count,
            prototype_summary_refined_count,
            prototype_summary_round_count,
            call_effect_summary_refined_count,
            wrapper_summary_fold_count,
            cleanup_budget_skip_count,
            cleanup_family_binding_init_count,
            cleanup_family_stmt_canonical_count,
            cleanup_stmt_fold_count,
            cleanup_boundary_label_count,
            cleanup_loopish_rewrite_count,
            cleanup_family_dead_binding_count,
            interproc_signature_constraint_rounds,
            preserved_temp_prune_blocked_count,
            preserved_temp_copyprop_skip_count,
            gvn_join_preserved_count,
            pass_rerun_skipped_by_preservation_count,
            compare_chain_dispatcher_count,
            candidate_scoped_jump_resolver_count,
            sccp_skipped_by_admission_count,
            wide_dead_assignment_rerun_admitted_count,
            wide_dead_assignment_rerun_skipped_by_admission_count,
            memory_fact_prefilter_skip_count,
            aggregate_fields_skipped_by_admission_count,
            memory_slot_cheap_exit_count,
        ]
    );
}

fn merge_call_target_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            call_target_import_resolved_count,
            call_target_direct_symbol_resolved_count,
            call_target_unresolved_sub_fallback_count,
            call_target_context_missing_count,
            call_target_exact_index_hit_count,
            call_target_exact_index_ambiguous_count,
            call_target_export_thunk_target_resolved_count,
            call_target_indirect_const_resolved_count,
            call_target_iat_slot_resolved_count,
            call_target_indirect_load_resolved_count,
            call_target_indirect_ptr_const_folded_count,
            call_target_indirect_rejected_non_iat_load_count,
            call_target_indirect_rejected_non_const_ptr_count,
            call_target_indirect_rejected_unsupported_ptr_opcode_count,
            call_target_indirect_rejected_ambiguous_def_count,
            call_target_indirect_rejected_non_dominating_def_count,
            call_target_indirect_rejected_no_def_count,
            call_target_indirect_rejected_width_mismatch_count,
            call_target_unresolved_no_exact_identity_count,
        ]
    );
}

fn merge_indirect_control_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            unsupported_indirect_control_count,
            unsupported_indirect_call_count,
            unsupported_external_target_count,
            indirect_surface_preserved_count,
            indirect_target_set_refined_count,
            dispatcher_shape_recovered_count,
        ]
    );
}

fn merge_materialization_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            materialization_stabilized_count,
            replacement_plan_candidate_count,
            replacement_plan_completed_count,
            replacement_plan_merge_binding_count,
            replacement_plan_rejected_alias_unsafe_count,
            replacement_plan_rejected_missing_merge_count,
            replacement_plan_rejected_representative_root_attribution_count,
            replacement_plan_rejected_temp_only_representative_lifecycle_count,
            replacement_plan_rejected_dead_temp_representative_count,
            materialization_inline_suppressed_count,
            representative_downgrade_count,
            representative_downgrade_no_aliassafe_source_count,
            representative_downgrade_join_conflict_count,
        ]
    );
}

fn merge_dispatcher_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            proof_payload_direct_emit_count,
            dispatcher_proof_unit_count,
            dispatcher_proof_completed_count,
            dispatcher_proof_failed_count,
            switch_emit_ready_failed_count,
        ]
    );
}

fn merge_admission_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(target, other, [pe_admission_profile_mismatch_count]);
}

fn merge_structuring_reason_stats(target: &mut NirBuildStats, other: &NirBuildStats) {
    merge_additive_fields!(
        target,
        other,
        [
            structuring_reason_region_legality_count,
            structuring_reason_follow_failure_count,
            structuring_reason_irreducible_count,
            structuring_reason_loop_exit_count,
            structuring_reason_switch_shape_count,
            structuring_reason_budget_count,
        ]
    );
}

fn merge_pass_metrics(
    target: &mut std::collections::BTreeMap<String, PassAggregate>,
    other: &std::collections::BTreeMap<String, PassAggregate>,
) {
    for (name, agg) in other {
        let current = target.entry(name.clone()).or_default();
        current.total_time_ms += agg.total_time_ms;
        current.total_invocations += agg.total_invocations;
        current.changed_count += agg.changed_count;
        current.stmts_reduced += agg.stmts_reduced;
        current.locals_reduced += agg.locals_reduced;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Number, Value};

    #[test]
    fn merge_assign_preserves_flat_public_numeric_contract() {
        let mut payload =
            serde_json::to_value(NirBuildStats::default()).expect("serialize default stats");
        let Value::Object(ref mut object) = payload else {
            panic!("NirBuildStats must serialize as an object");
        };

        for value in object.values_mut() {
            if value.is_number() {
                *value = Value::Number(Number::from(1));
            }
        }
        object.insert(
            "pass_metrics".to_string(),
            serde_json::json!({
                "cleanup": {
                    "total_time_ms": 1.5,
                    "total_invocations": 1,
                    "changed_count": 1,
                    "stmts_reduced": -1,
                    "locals_reduced": 2
                }
            }),
        );

        let source: NirBuildStats =
            serde_json::from_value(payload.clone()).expect("deserialize synthetic stats payload");
        let mut merged = NirBuildStats::default();
        merged.merge_assign(&source);

        assert_eq!(
            serde_json::to_value(merged).expect("serialize merged stats"),
            payload
        );
    }

    #[test]
    fn refresh_structuring_reason_families_overwrites_derived_totals() {
        let mut stats = NirBuildStats {
            region_emit_ready_failed_count: 2,
            guarded_tail_rejected_missing_terminal_join_count: 3,
            rejected_not_single_pred_succ: 5,
            structuring_reason_region_legality_count: 99,
            ..NirBuildStats::default()
        };

        stats.refresh_structuring_reason_families();

        assert_eq!(stats.structuring_reason_region_legality_count, 10);
    }

    #[test]
    fn merge_assign_preserves_telemetry_sentinel_counters() {
        let source = NirBuildStats {
            replacement_plan_rejected_alias_unsafe_count: 2,
            replacement_plan_rejected_missing_merge_count: 3,
            guarded_tail_replacement_plan_rejected_missing_merge_count: 5,
            guarded_tail_replacement_plan_rejected_unstable_read_count: 7,
            region_emit_ready_failed_count: 11,
            call_target_unresolved_sub_fallback_count: 13,
            structuring_irreducible_scc_count: 17,
            max_structuring_scc_component_size: 19,
            ..NirBuildStats::default()
        };
        let mut target = NirBuildStats {
            replacement_plan_rejected_alias_unsafe_count: 20,
            replacement_plan_rejected_missing_merge_count: 30,
            guarded_tail_replacement_plan_rejected_missing_merge_count: 50,
            guarded_tail_replacement_plan_rejected_unstable_read_count: 70,
            region_emit_ready_failed_count: 110,
            call_target_unresolved_sub_fallback_count: 130,
            structuring_irreducible_scc_count: 170,
            max_structuring_scc_component_size: 3,
            ..NirBuildStats::default()
        };

        target.merge_assign(&source);

        assert_eq!(target.replacement_plan_rejected_alias_unsafe_count, 22);
        assert_eq!(target.replacement_plan_rejected_missing_merge_count, 33);
        assert_eq!(
            target.guarded_tail_replacement_plan_rejected_missing_merge_count,
            55
        );
        assert_eq!(
            target.guarded_tail_replacement_plan_rejected_unstable_read_count,
            77
        );
        assert_eq!(target.region_emit_ready_failed_count, 121);
        assert_eq!(target.call_target_unresolved_sub_fallback_count, 143);
        assert_eq!(target.structuring_irreducible_scc_count, 187);
        assert_eq!(target.max_structuring_scc_component_size, 19);
    }

    #[test]
    fn guarded_tail_discovery_merge_preserves_legacy_projection_boundary() {
        let source = NirBuildStats {
            promotion_candidate_count: 1,
            canonicalized_guarded_tail_shape_count: 2,
            region_emit_ready_failed_count: 3,
            blockgraph_region_complete_count: 4,
            ..NirBuildStats::default()
        };
        let mut target = NirBuildStats::default();

        target.merge_guarded_tail_discovery_assign(&source);

        assert_eq!(target.promotion_candidate_count, 1);
        assert_eq!(target.canonicalized_guarded_tail_shape_count, 2);
        assert_eq!(target.region_emit_ready_failed_count, 0);
        assert_eq!(target.blockgraph_region_complete_count, 0);
    }
}
