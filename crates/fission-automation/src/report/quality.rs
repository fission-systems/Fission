//! Quality ratios and top counter rollups derived from aggregates.

use crate::report::snapshot::AutomationSummary;
use fission_pcode::NirBuildStats;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMeasurementSnapshot {
    pub rows_emitted: usize,
    pub direct_success_count: usize,
    pub nir_output_class_counts: BTreeMap<String, usize>,
    pub structured_ratio_all_rows: f64,
    pub structured_ratio_success_rows: f64,
    pub linear_fallback_ratio_all_rows: f64,
    pub linear_fallback_ratio_success_rows: f64,
    pub top_build_stats: Vec<(String, usize)>,
    pub build_stat_families: Vec<(String, usize)>,
    pub structuring_fallback_reasons: Vec<(String, usize)>,
}

fn ratio(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

fn build_stats_pairs(stats: &NirBuildStats) -> Vec<(&'static str, usize)> {
    vec![
        (
            "build_duration_ms",
            stats.build_duration_ms,
        ),
        (
            "normalize_duration_ms",
            stats.normalize_duration_ms,
        ),
        (
            "forced_linear_structuring_count",
            stats.forced_linear_structuring_count,
        ),
        (
            "region_linearize_structuring_count",
            stats.region_linearize_structuring_count,
        ),
        (
            "region_linearize_rejected_non_structuring_failure_count",
            stats.region_linearize_rejected_non_structuring_failure_count,
        ),
        (
            "region_linearize_rejected_no_exit_count",
            stats.region_linearize_rejected_no_exit_count,
        ),
        (
            "region_linearize_rejected_body_lowering_failed_count",
            stats.region_linearize_rejected_body_lowering_failed_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
        ),
        (
            "region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count",
            stats.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
        ),
        (
            "region_linearize_rejected_body_lowering_successor_inline_rejected_count",
            stats.region_linearize_rejected_body_lowering_successor_inline_rejected_count,
        ),
        (
            "region_linearize_rejected_body_lowering_revisit_cycle_count",
            stats.region_linearize_rejected_body_lowering_revisit_cycle_count,
        ),
        (
            "region_linearize_rejected_body_lowering_unsupported_terminator_count",
            stats.region_linearize_rejected_body_lowering_unsupported_terminator_count,
        ),
        (
            "region_linearize_rejected_non_advancing_count",
            stats.region_linearize_rejected_non_advancing_count,
        ),
        (
            "region_linearize_rejected_irreducible_cfg_count",
            stats.region_linearize_rejected_irreducible_cfg_count,
        ),
        (
            "structuring_scc_component_count",
            stats.structuring_scc_component_count,
        ),
        (
            "structuring_irreducible_scc_count",
            stats.structuring_irreducible_scc_count,
        ),
        (
            "structuring_irreducible_header_count",
            stats.structuring_irreducible_header_count,
        ),
        (
            "loop_control_explicit_reducer_count",
            stats.loop_control_explicit_reducer_count,
        ),
        (
            "loop_control_rewrite_break_count",
            stats.loop_control_rewrite_break_count,
        ),
        (
            "loop_control_rewrite_continue_count",
            stats.loop_control_rewrite_continue_count,
        ),
        (
            "loop_control_rewrite_skipped_nested_scope_count",
            stats.loop_control_rewrite_skipped_nested_scope_count,
        ),
        (
            "region_proof_candidate_count",
            stats.region_proof_candidate_count,
        ),
        (
            "region_proof_completed_count",
            stats.region_proof_completed_count,
        ),
        (
            "region_emit_ready_failed_count",
            stats.region_emit_ready_failed_count,
        ),
        (
            "conditional_region_candidate_count",
            stats.conditional_region_candidate_count,
        ),
        (
            "conditional_region_promoted_count",
            stats.conditional_region_promoted_count,
        ),
        ("promotion_candidate_count", stats.promotion_candidate_count),
        ("promoted_region_count", stats.promoted_region_count),
        (
            "promotion_rejected_by_shape_count",
            stats.promotion_rejected_by_shape_count,
        ),
        (
            "promotion_rejected_by_shape_missing_terminal_join_target_count",
            stats.promotion_rejected_by_shape_missing_terminal_join_target_count,
        ),
        (
            "promotion_rejected_by_shape_empty_nonterminal_tail_count",
            stats.promotion_rejected_by_shape_empty_nonterminal_tail_count,
        ),
        (
            "promotion_rejected_by_gate_count",
            stats.promotion_rejected_by_gate_count,
        ),
        (
            "discovery_seen_guarded_tail_like_shape_count",
            stats.discovery_seen_guarded_tail_like_shape_count,
        ),
        (
            "discovery_rejected_noncanonical_layout_count",
            stats.discovery_rejected_noncanonical_layout_count,
        ),
        (
            "canonicalized_guarded_tail_shape_count",
            stats.canonicalized_guarded_tail_shape_count,
        ),
        (
            "canonicalization_failed_multiple_payload_entries",
            stats.canonicalization_failed_multiple_payload_entries,
        ),
        (
            "canonicalization_failed_interleaved_join_uses",
            stats.canonicalization_failed_interleaved_join_uses,
        ),
        (
            "canonicalization_failed_interleaved_join_uses_no_next_label_count",
            stats.canonicalization_failed_interleaved_join_uses_no_next_label_count,
        ),
        (
            "canonicalization_failed_interleaved_join_uses_nontrivial_segment_count",
            stats.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count,
        ),
        (
            "canonicalization_failed_nonterminal_join_label",
            stats.canonicalization_failed_nonterminal_join_label,
        ),
        (
            "canonicalization_failed_nested_tail_escape",
            stats.canonicalization_failed_nested_tail_escape,
        ),
        (
            "canonicalized_interleaved_join_use_count",
            stats.canonicalized_interleaved_join_use_count,
        ),
        (
            "canonicalized_local_nonfallthrough_alias_count",
            stats.canonicalized_local_nonfallthrough_alias_count,
        ),
        (
            "canonicalization_failed_alias_not_fallthrough_count",
            stats.canonicalization_failed_alias_not_fallthrough_count,
        ),
        (
            "canonicalization_failed_alias_not_fallthrough_top_level_after_label_count",
            stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count,
        ),
        (
            "canonicalization_failed_alias_not_fallthrough_nested_after_label_count",
            stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count,
        ),
        (
            "canonicalization_failed_alias_has_multiple_internal_predecessors_count",
            stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        ),
        (
            "canonicalization_failed_alias_has_nonlocal_ref_count",
            stats.canonicalization_failed_alias_has_nonlocal_ref_count,
        ),
        (
            "canonicalization_failed_alias_has_nonlocal_ref_external_before_count",
            stats.canonicalization_failed_alias_has_nonlocal_ref_external_before_count,
        ),
        (
            "canonicalization_failed_alias_has_nonlocal_ref_nested_before_count",
            stats.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count,
        ),
        (
            "canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count",
            stats.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count,
        ),
        (
            "canonicalization_failed_alias_body_not_trivial_count",
            stats.canonicalization_failed_alias_body_not_trivial_count,
        ),
        (
            "canonicalization_failed_join_has_external_ref_count",
            stats.canonicalization_failed_join_has_external_ref_count,
        ),
        (
            "canonicalization_failed_payload_crosses_join_count",
            stats.canonicalization_failed_payload_crosses_join_count,
        ),
        ("rejected_must_emit_label", stats.rejected_must_emit_label),
        (
            "rejected_must_emit_label_surviving_middle_ref",
            stats.rejected_must_emit_label_surviving_middle_ref,
        ),
        (
            "rejected_must_emit_label_surviving_external_ref",
            stats.rejected_must_emit_label_surviving_external_ref,
        ),
        (
            "rejected_must_emit_label_owner_conflict",
            stats.rejected_must_emit_label_owner_conflict,
        ),
        (
            "rejected_not_single_pred_succ",
            stats.rejected_not_single_pred_succ,
        ),
        ("rejected_external_entry", stats.rejected_external_entry),
        (
            "rejected_loop_or_switch_target",
            stats.rejected_loop_or_switch_target,
        ),
        (
            "condition_fold_and_count",
            stats.condition_fold_and_count,
        ),
        (
            "condition_fold_or_count",
            stats.condition_fold_or_count,
        ),
        (
            "condition_fold_rejected_side_effect",
            stats.condition_fold_rejected_side_effect,
        ),
        (
            "entry_param_promotion_spill_rename_count",
            stats.entry_param_promotion_spill_rename_count,
        ),
        (
            "variadic_stack_region_fold_count",
            stats.variadic_stack_region_fold_count,
        ),
        ("abi_slot_recovered_count", stats.abi_slot_recovered_count),
        ("home_slot_promoted_count", stats.home_slot_promoted_count),
        ("va_start_recovered_count", stats.va_start_recovered_count),
        (
            "call_signature_refined_count",
            stats.call_signature_refined_count,
        ),
        (
            "security_cookie_fold_count",
            stats.security_cookie_fold_count,
        ),
        (
            "call_artifact_removed_count",
            stats.call_artifact_removed_count,
        ),
        (
            "object_shape_recovered_count",
            stats.object_shape_recovered_count,
        ),
        (
            "object_root_recovered_count",
            stats.object_root_recovered_count,
        ),
        (
            "typed_fact_evidence_count",
            stats.typed_fact_evidence_count,
        ),
        (
            "typed_fact_conflict_count",
            stats.typed_fact_conflict_count,
        ),
        (
            "object_root_fact_promotion_count",
            stats.object_root_fact_promotion_count,
        ),
        (
            "typed_object_shape_refined_count",
            stats.typed_object_shape_refined_count,
        ),
        (
            "surface_binding_promoted_count",
            stats.surface_binding_promoted_count,
        ),
        (
            "surface_fact_promotion_count",
            stats.surface_fact_promotion_count,
        ),
        (
            "prototype_summary_refined_count",
            stats.prototype_summary_refined_count,
        ),
        (
            "prototype_summary_round_count",
            stats.prototype_summary_round_count,
        ),
        (
            "call_effect_summary_refined_count",
            stats.call_effect_summary_refined_count,
        ),
        (
            "wrapper_summary_fold_count",
            stats.wrapper_summary_fold_count,
        ),
        (
            "cleanup_budget_skip_count",
            stats.cleanup_budget_skip_count,
        ),
        (
            "cleanup_family_binding_init_count",
            stats.cleanup_family_binding_init_count,
        ),
        (
            "cleanup_family_stmt_canonical_count",
            stats.cleanup_family_stmt_canonical_count,
        ),
        (
            "cleanup_stmt_fold_count",
            stats.cleanup_stmt_fold_count,
        ),
        (
            "cleanup_boundary_label_count",
            stats.cleanup_boundary_label_count,
        ),
        (
            "cleanup_loopish_rewrite_count",
            stats.cleanup_loopish_rewrite_count,
        ),
        (
            "cleanup_family_dead_binding_count",
            stats.cleanup_family_dead_binding_count,
        ),
        (
            "interproc_signature_constraint_rounds",
            stats.interproc_signature_constraint_rounds,
        ),
        (
            "unsupported_indirect_control_count",
            stats.unsupported_indirect_control_count,
        ),
        (
            "unsupported_indirect_call_count",
            stats.unsupported_indirect_call_count,
        ),
        (
            "unsupported_external_target_count",
            stats.unsupported_external_target_count,
        ),
        (
            "indirect_surface_preserved_count",
            stats.indirect_surface_preserved_count,
        ),
        (
            "indirect_target_set_refined_count",
            stats.indirect_target_set_refined_count,
        ),
        (
            "dispatcher_shape_recovered_count",
            stats.dispatcher_shape_recovered_count,
        ),
        (
            "switch_emit_ready_failed_count",
            stats.switch_emit_ready_failed_count,
        ),
        (
            "structuring_reason_region_legality_count",
            stats.structuring_reason_region_legality_count,
        ),
        (
            "structuring_reason_follow_failure_count",
            stats.structuring_reason_follow_failure_count,
        ),
        (
            "structuring_reason_irreducible_count",
            stats.structuring_reason_irreducible_count,
        ),
        (
            "structuring_reason_loop_exit_count",
            stats.structuring_reason_loop_exit_count,
        ),
        (
            "structuring_reason_switch_shape_count",
            stats.structuring_reason_switch_shape_count,
        ),
        (
            "structuring_reason_budget_count",
            stats.structuring_reason_budget_count,
        ),
    ]
}

pub(crate) fn build_stat_families(stats: &NirBuildStats) -> Vec<(String, usize)> {
    let mut families = vec![
        (
            "abi".to_string(),
            stats.entry_param_promotion_spill_rename_count
                + stats.abi_slot_recovered_count
                + stats.home_slot_promoted_count,
        ),
        (
            "memory_shape".to_string(),
            stats.call_artifact_removed_count
                + stats.object_shape_recovered_count
                + stats.object_root_recovered_count
                + stats.object_root_fact_promotion_count
                + stats.typed_object_shape_refined_count
                + stats.surface_binding_promoted_count
                + stats.surface_fact_promotion_count,
        ),
        (
            "variadic".to_string(),
            stats.variadic_stack_region_fold_count + stats.va_start_recovered_count,
        ),
        (
            "call_signature".to_string(),
            stats.call_signature_refined_count
                + stats.prototype_summary_refined_count
                + stats.prototype_summary_round_count
                + stats.call_effect_summary_refined_count
                + stats.wrapper_summary_fold_count
                + stats.interproc_signature_constraint_rounds
                + stats.unsupported_indirect_call_count
                + stats.indirect_target_set_refined_count,
        ),
        (
            "cleanup_budget".to_string(),
            stats.cleanup_budget_skip_count,
        ),
        (
            "cleanup_binding_init".to_string(),
            stats.cleanup_family_binding_init_count,
        ),
        (
            "cleanup_stmt_canonical".to_string(),
            stats.cleanup_family_stmt_canonical_count,
        ),
        (
            "cleanup_stmt_fold".to_string(),
            stats.cleanup_stmt_fold_count,
        ),
        (
            "cleanup_boundary_label".to_string(),
            stats.cleanup_boundary_label_count,
        ),
        (
            "cleanup_loopish_rewrite".to_string(),
            stats.cleanup_loopish_rewrite_count,
        ),
        (
            "cleanup_dead_binding".to_string(),
            stats.cleanup_family_dead_binding_count,
        ),
        (
            "structuring".to_string(),
            stats.region_linearize_structuring_count
                + stats.forced_linear_structuring_count
                + stats.region_proof_candidate_count
                + stats.region_proof_completed_count
                + stats.region_emit_ready_failed_count
                + stats.conditional_region_candidate_count
                + stats.conditional_region_promoted_count
                + stats.structuring_reason_region_legality_count
                + stats.structuring_reason_follow_failure_count
                + stats.structuring_reason_irreducible_count
                + stats.structuring_reason_loop_exit_count
                + stats.structuring_reason_switch_shape_count
                + stats.structuring_reason_budget_count
                + stats.promotion_candidate_count
                + stats.promoted_region_count,
        ),
        (
            "dispatcher".to_string(),
            stats.dispatcher_shape_recovered_count
                + stats.region_emit_ready_failed_count
                + stats.switch_emit_ready_failed_count
                + stats.unsupported_indirect_control_count
                + stats.unsupported_external_target_count
                + stats.indirect_surface_preserved_count,
        ),
        ("security".to_string(), stats.security_cookie_fold_count),
    ];
    families.retain(|(_, value)| *value > 0);
    families.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    families
}

pub(crate) fn top_build_stats(stats: &NirBuildStats, limit: usize) -> Vec<(String, usize)> {
    let mut pairs = build_stats_pairs(stats)
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .map(|(name, value)| (name.to_string(), value))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(limit);
    pairs
}

pub(crate) fn structuring_fallback_reasons(stats: &NirBuildStats) -> Vec<(String, usize)> {
    let mut pairs = vec![
        ("non_structuring_failure", stats.region_linearize_rejected_non_structuring_failure_count),
        ("no_exit", stats.region_linearize_rejected_no_exit_count),
        ("body_lowering_failed", stats.region_linearize_rejected_body_lowering_failed_count),
        ("cond_tail_exit_mismatch", stats.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count),
        ("cond_tail_no_common_follow", stats.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count),
        ("cond_tail_follow_beyond", stats.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count),
        ("cond_tail_side_entry_or_exit", stats.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count),
        ("cond_tail_complex_shape", stats.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count),
        ("cond_tail_depth_budget", stats.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count),
        ("cond_tail_arm_lowering_failed", stats.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count),
        ("cond_tail_one_arm_lowering_failed", stats.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count),
        ("cond_tail_both_arms_lowering_failed", stats.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count),
        ("cond_tail_follow_lowering_failed", stats.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count),
        ("cond_tail_ambiguous_follows", stats.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count),
        ("successor_inline_rejected", stats.region_linearize_rejected_body_lowering_successor_inline_rejected_count),
        ("revisit_cycle", stats.region_linearize_rejected_body_lowering_revisit_cycle_count),
        ("unsupported_terminator", stats.region_linearize_rejected_body_lowering_unsupported_terminator_count),
        ("non_advancing", stats.region_linearize_rejected_non_advancing_count),
        ("irreducible_cfg", stats.region_linearize_rejected_irreducible_cfg_count),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(k, v)| (k.to_string(), v))
    .collect::<Vec<_>>();

    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs
}

pub fn build_quality_measurement(summary: &AutomationSummary) -> QualityMeasurementSnapshot {
    let output_counts = &summary.aggregate.nir_output_class_counts;
    let structured = output_counts.get("structured").copied().unwrap_or(0);
    let linear_fallback = output_counts.get("linear_fallback").copied().unwrap_or(0);
    QualityMeasurementSnapshot {
        rows_emitted: summary.aggregate.rows_emitted,
        direct_success_count: summary.aggregate.direct_success_count,
        nir_output_class_counts: output_counts.clone(),
        structured_ratio_all_rows: ratio(structured, summary.aggregate.rows_emitted),
        structured_ratio_success_rows: ratio(structured, summary.aggregate.direct_success_count),
        linear_fallback_ratio_all_rows: ratio(linear_fallback, summary.aggregate.rows_emitted),
        linear_fallback_ratio_success_rows: ratio(
            linear_fallback,
            summary.aggregate.direct_success_count,
        ),
        top_build_stats: top_build_stats(&summary.aggregate.nir_build_stats_totals, 8),
        build_stat_families: build_stat_families(&summary.aggregate.nir_build_stats_totals),
        structuring_fallback_reasons: structuring_fallback_reasons(
            &summary.aggregate.nir_build_stats_totals,
        ),
    }
}
