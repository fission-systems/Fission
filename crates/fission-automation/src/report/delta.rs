//! Baseline delta between automation summaries.

use crate::report::snapshot::AutomationSummary;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SummaryDelta {
    pub direct_success_count: isize,
    pub nir_failure_count: isize,
    pub explicit_fact_nonzero_count: isize,
    pub strict_explicit_candidate_count: isize,
    pub inventory_surface_gap_count: isize,
    pub pdb_nonzero_rows: isize,
    pub region_linearized_count: isize,
    pub forced_linear_count: isize,
    pub conditional_tail_exit_mismatch_count: isize,
    pub body_lowering_failed_count: isize,
    pub successor_inline_rejected_count: isize,
    pub revisit_cycle_count: isize,
    pub unsupported_terminator_count: isize,
    pub rejected_irreducible_cfg_count: isize,
    pub structuring_scc_component_count: isize,
    pub structuring_irreducible_scc_count: isize,
    pub structuring_irreducible_header_count: isize,
    pub loop_control_explicit_reducer_count: isize,
    pub loop_control_rewrite_break_count: isize,
    pub loop_control_rewrite_continue_count: isize,
    pub loop_control_rewrite_skipped_nested_scope_count: isize,
}

pub fn compute_delta(
    current: &AutomationSummary,
    baseline: Option<&AutomationSummary>,
) -> Option<SummaryDelta> {
    let baseline = baseline?;
    Some(SummaryDelta {
        direct_success_count: current.aggregate.direct_success_count as isize
            - baseline.aggregate.direct_success_count as isize,
        nir_failure_count: current.aggregate.nir_failure_count as isize
            - baseline.aggregate.nir_failure_count as isize,
        explicit_fact_nonzero_count: current.aggregate.explicit_fact_nonzero_count as isize
            - baseline.aggregate.explicit_fact_nonzero_count as isize,
        strict_explicit_candidate_count: current.aggregate.strict_explicit_candidate_count as isize
            - baseline.aggregate.strict_explicit_candidate_count as isize,
        inventory_surface_gap_count: current.aggregate.inventory_surface_gap_count as isize
            - baseline.aggregate.inventory_surface_gap_count as isize,
        pdb_nonzero_rows: current.aggregate.provenance_surface_totals.pdb_nonzero_rows as isize
            - baseline
                .aggregate
                .provenance_surface_totals
                .pdb_nonzero_rows as isize,
        region_linearized_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_structuring_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_structuring_count as isize,
        forced_linear_count: current
            .aggregate
            .nir_build_stats_totals
            .forced_linear_structuring_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .forced_linear_structuring_count as isize,
        conditional_tail_exit_mismatch_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
                as isize,
        body_lowering_failed_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_failed_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_failed_count as isize,
        successor_inline_rejected_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_successor_inline_rejected_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_successor_inline_rejected_count
                as isize,
        revisit_cycle_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_revisit_cycle_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_revisit_cycle_count as isize,
        unsupported_terminator_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_unsupported_terminator_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_unsupported_terminator_count
                as isize,
        rejected_irreducible_cfg_count: current
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_irreducible_cfg_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_irreducible_cfg_count as isize,
        structuring_scc_component_count: current
            .aggregate
            .nir_build_stats_totals
            .structuring_scc_component_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_scc_component_count as isize,
        structuring_irreducible_scc_count: current
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_scc_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_scc_count as isize,
        structuring_irreducible_header_count: current
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_header_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_header_count as isize,
        loop_control_explicit_reducer_count: current
            .aggregate
            .nir_build_stats_totals
            .loop_control_explicit_reducer_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .loop_control_explicit_reducer_count as isize,
        loop_control_rewrite_break_count: current
            .aggregate
            .nir_build_stats_totals
            .loop_control_rewrite_break_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .loop_control_rewrite_break_count as isize,
        loop_control_rewrite_continue_count: current
            .aggregate
            .nir_build_stats_totals
            .loop_control_rewrite_continue_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .loop_control_rewrite_continue_count as isize,
        loop_control_rewrite_skipped_nested_scope_count: current
            .aggregate
            .nir_build_stats_totals
            .loop_control_rewrite_skipped_nested_scope_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .loop_control_rewrite_skipped_nested_scope_count as isize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProvenanceSurfaceTotals, SourcePresenceCounts};
    use crate::report::snapshot::{AggregateSnapshot, AutomationSummary};
    use fission_pcode::NirBuildStats;
    use std::collections::BTreeMap;

    fn minimal_summary(direct_success_count: usize) -> AutomationSummary {
        AutomationSummary {
            generated_at: "t".into(),
            lane: "nir".into(),
            run_id: "r".into(),
            run_profile: "mid".into(),
            target_count: 1,
            inventory_elapsed_ms: 0,
            diagnosis_elapsed_ms: 0,
            write_outputs_elapsed_ms: 0,
            total_elapsed_ms: 0,
            binaries: vec![],
            aggregate: AggregateSnapshot {
                rows_emitted: 0,
                direct_success_count,
                nir_failure_count: 0,
                explicit_fact_nonzero_count: 0,
                strict_explicit_candidate_count: 0,
                inventory_surface_gap_count: 0,
                source_presence_counts: SourcePresenceCounts::default(),
                provenance_surface_totals: ProvenanceSurfaceTotals::default(),
                diagnosis_bucket_counts: BTreeMap::new(),
                nir_block_signature_counts: BTreeMap::new(),
                recommended_next_patch: None,
                recovery_strategy_attempted_counts: BTreeMap::new(),
                recovery_strategy_applied_counts: BTreeMap::new(),
                recovery_outcome_counts: BTreeMap::new(),
                recovery_quality_flag_counts: BTreeMap::new(),
                recovery_structuring_mode_counts: BTreeMap::new(),
                nir_output_class_counts: BTreeMap::new(),
                nir_build_stats_totals: NirBuildStats::default(),
            },
        }
    }

    #[test]
    fn compute_delta_without_baseline_is_none() {
        let cur = minimal_summary(1);
        assert!(compute_delta(&cur, None).is_none());
    }

    #[test]
    fn compute_delta_tracks_direct_success_shift() {
        let base = minimal_summary(1);
        let cur = minimal_summary(4);
        let d = compute_delta(&cur, Some(&base)).expect("delta");
        assert_eq!(d.direct_success_count, 3);
    }
}
