//! Build [`AutomationSummary`](crate::report::snapshot::AutomationSummary) from inventory rows.

use crate::corpus::InventorySummaryTotals;
use crate::diagnosis::{DiagnosisAggregate, DiagnosisReport};
use crate::model::{InventorySummary, ProvenanceSurfaceTotals};
use crate::report::snapshot::{AggregateSnapshot, AutomationSummary, BinarySnapshot};
use fission_pcode::NirBuildStats;
use std::collections::BTreeMap;

pub fn build_summary(
    generated_at: String,
    lane: &str,
    run_id: &str,
    run_profile: &str,
    target_count: usize,
    inventory_summaries: &[InventorySummary],
    totals: &InventorySummaryTotals,
    diagnosis: &DiagnosisAggregate,
) -> AutomationSummary {
    let mut aggregate_recovery_strategy_attempted_counts = BTreeMap::new();
    let mut aggregate_recovery_strategy_applied_counts = BTreeMap::new();
    let mut aggregate_recovery_outcome_counts = BTreeMap::new();
    let mut aggregate_recovery_quality_flag_counts = BTreeMap::new();
    let mut aggregate_recovery_structuring_mode_counts = BTreeMap::new();
    let mut aggregate_nir_output_class_counts = BTreeMap::new();
    let mut aggregate_nir_build_stats_totals = NirBuildStats::default();
    let mut binaries = Vec::new();
    for summary in inventory_summaries {
        merge_counts(
            &mut aggregate_recovery_strategy_attempted_counts,
            &summary.recovery_strategy_attempted_counts,
        );
        merge_counts(
            &mut aggregate_recovery_strategy_applied_counts,
            &summary.recovery_strategy_applied_counts,
        );
        merge_counts(
            &mut aggregate_recovery_outcome_counts,
            &summary.recovery_outcome_counts,
        );
        merge_counts(
            &mut aggregate_recovery_quality_flag_counts,
            &summary.recovery_quality_flag_counts,
        );
        merge_counts(
            &mut aggregate_recovery_structuring_mode_counts,
            &summary.recovery_structuring_mode_counts,
        );
        merge_counts(
            &mut aggregate_nir_output_class_counts,
            &summary.nir_output_class_counts,
        );
        aggregate_nir_build_stats_totals.merge_assign(&summary.nir_build_stats_totals);
        binaries.push(BinarySnapshot {
            binary: summary.binary.clone(),
            rows_emitted: summary.rows_emitted,
            direct_success_count: summary.direct_success_count,
            nir_failure_count: summary.nir_failure_count,
            explicit_fact_nonzero_count: summary.explicit_fact_nonzero_count,
            strict_explicit_candidate_count: summary.strict_explicit_candidate_count,
            inventory_surface_gap_count: summary.inventory_surface_gap_count,
            source_presence_counts: summary.source_presence_counts.clone(),
            provenance_surface_totals: summary.provenance_surface_totals.clone(),
            recovery_strategy_attempted_counts: summary.recovery_strategy_attempted_counts.clone(),
            recovery_strategy_applied_counts: summary.recovery_strategy_applied_counts.clone(),
            recovery_outcome_counts: summary.recovery_outcome_counts.clone(),
            recovery_quality_flag_counts: summary.recovery_quality_flag_counts.clone(),
            recovery_structuring_mode_counts: summary.recovery_structuring_mode_counts.clone(),
            nir_output_class_counts: summary.nir_output_class_counts.clone(),
            nir_build_stats_totals: summary.nir_build_stats_totals.clone(),
        });
    }
    AutomationSummary {
        generated_at,
        lane: lane.to_string(),
        run_id: run_id.to_string(),
        run_profile: run_profile.to_string(),
        target_count,
        inventory_elapsed_ms: 0,
        diagnosis_elapsed_ms: 0,
        write_outputs_elapsed_ms: 0,
        total_elapsed_ms: 0,
        binaries,
        aggregate: AggregateSnapshot {
            rows_emitted: totals.rows_emitted,
            direct_success_count: totals.direct_success_count,
            nir_failure_count: totals.nir_failure_count,
            explicit_fact_nonzero_count: totals.explicit_fact_nonzero_count,
            strict_explicit_candidate_count: totals.strict_explicit_candidate_count,
            inventory_surface_gap_count: totals.inventory_surface_gap_count,
            source_presence_counts: totals.source_presence_counts.clone(),
            provenance_surface_totals: ProvenanceSurfaceTotals {
                dwarf_nonzero_rows: 0,
                pdb_nonzero_rows: 0,
                native_nonzero_rows: 0,
                loader_nonzero_rows: 0,
            },
            diagnosis_bucket_counts: diagnosis.diagnosis_bucket_counts.clone(),
            nir_block_signature_counts: diagnosis.nir_block_signature_counts.clone(),
            recommended_next_patch: diagnosis.recommended_next_patch.clone(),
            recovery_strategy_attempted_counts: aggregate_recovery_strategy_attempted_counts,
            recovery_strategy_applied_counts: aggregate_recovery_strategy_applied_counts,
            recovery_outcome_counts: aggregate_recovery_outcome_counts,
            recovery_quality_flag_counts: aggregate_recovery_quality_flag_counts,
            recovery_structuring_mode_counts: aggregate_recovery_structuring_mode_counts,
            nir_output_class_counts: aggregate_nir_output_class_counts,
            nir_build_stats_totals: aggregate_nir_build_stats_totals,
        },
    }
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (key, value) in source {
        *target.entry(key.clone()).or_default() += *value;
    }
}
pub fn enrich_summary_with_provenance(
    summary: &mut AutomationSummary,
    diagnosis_report: &DiagnosisReport,
) {
    let mut dwarf_nonzero_rows = 0usize;
    let mut pdb_nonzero_rows = 0usize;
    let mut native_nonzero_rows = 0usize;
    let mut loader_nonzero_rows = 0usize;
    for entry in &diagnosis_report.binaries {
        dwarf_nonzero_rows += entry
            .derived_metrics
            .provenance_surface_totals
            .dwarf_nonzero_rows;
        pdb_nonzero_rows += entry
            .derived_metrics
            .provenance_surface_totals
            .pdb_nonzero_rows;
        native_nonzero_rows += entry
            .derived_metrics
            .provenance_surface_totals
            .native_nonzero_rows;
        loader_nonzero_rows += entry
            .derived_metrics
            .provenance_surface_totals
            .loader_nonzero_rows;
    }
    summary.aggregate.provenance_surface_totals = ProvenanceSurfaceTotals {
        dwarf_nonzero_rows,
        pdb_nonzero_rows,
        native_nonzero_rows,
        loader_nonzero_rows,
    };
}
