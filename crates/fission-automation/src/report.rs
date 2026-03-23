use crate::corpus::{CorpusArtifacts, InventorySummaryTotals};
use crate::diagnosis::{DiagnosisAggregate, DiagnosisReport};
use crate::model::{InventoryRow, InventorySummary, ProvenanceSurfaceTotals, SourcePresenceCounts};
use anyhow::{Context, Result};
use fission_pcode::NirBuildStats;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinarySnapshot {
    pub binary: String,
    #[serde(default)]
    pub rows_emitted: usize,
    pub direct_success_count: usize,
    #[serde(alias = "preview_failure_count")]
    pub nir_failure_count: usize,
    pub explicit_fact_nonzero_count: usize,
    pub strict_explicit_candidate_count: usize,
    pub inventory_surface_gap_count: usize,
    pub source_presence_counts: SourcePresenceCounts,
    pub provenance_surface_totals: ProvenanceSurfaceTotals,
    #[serde(default)]
    pub recovery_strategy_attempted_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_strategy_applied_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_outcome_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_quality_flag_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_structuring_mode_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub nir_output_class_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub nir_build_stats_totals: NirBuildStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateSnapshot {
    #[serde(default)]
    pub rows_emitted: usize,
    pub direct_success_count: usize,
    #[serde(alias = "preview_failure_count")]
    pub nir_failure_count: usize,
    pub explicit_fact_nonzero_count: usize,
    pub strict_explicit_candidate_count: usize,
    pub inventory_surface_gap_count: usize,
    pub source_presence_counts: SourcePresenceCounts,
    pub provenance_surface_totals: ProvenanceSurfaceTotals,
    pub diagnosis_bucket_counts: BTreeMap<String, usize>,
    #[serde(alias = "preview_block_signature_counts")]
    pub nir_block_signature_counts: BTreeMap<String, usize>,
    pub recommended_next_patch: Option<String>,
    #[serde(default)]
    pub recovery_strategy_attempted_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_strategy_applied_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_outcome_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_quality_flag_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_structuring_mode_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub nir_output_class_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub nir_build_stats_totals: NirBuildStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSummary {
    pub generated_at: String,
    pub lane: String,
    pub run_id: String,
    pub binaries: Vec<BinarySnapshot>,
    pub aggregate: AggregateSnapshot,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchRowSnapshot {
    pub binary: String,
    pub address: String,
    pub name: String,
    pub mismatch_count: usize,
    pub body_lowering_failed_count: usize,
    pub recovery_structuring_mode: Option<String>,
    pub nir_output_class: Option<String>,
    pub subtype_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MismatchRowDelta {
    pub binary: String,
    pub address: String,
    pub name: String,
    pub baseline_mismatch_count: usize,
    pub current_mismatch_count: usize,
    pub mismatch_delta: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoStopDecisionGate {
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDecisionInsights {
    pub mismatch_subtype_ranking: Vec<(String, usize)>,
    pub top_mismatch_rows: Vec<MismatchRowSnapshot>,
    pub mismatch_row_deltas: Vec<MismatchRowDelta>,
    pub changed_row_count: usize,
    pub go_stop_gate: GoStopDecisionGate,
}

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
}

pub fn build_summary(
    generated_at: String,
    lane: &str,
    run_id: &str,
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
            nir_build_stats_totals: summary.nir_build_stats_totals,
        });
    }
    AutomationSummary {
        generated_at,
        lane: lane.to_string(),
        run_id: run_id.to_string(),
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
            "forced_linear_structuring_count",
            stats.forced_linear_structuring_count,
        ),
        (
            "region_linearize_structuring_count",
            stats.region_linearize_structuring_count,
        ),
        (
            "region_linearize_heuristic_exit_count",
            stats.region_linearize_heuristic_exit_count,
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
        ("promotion_candidate_count", stats.promotion_candidate_count),
        ("promoted_region_count", stats.promoted_region_count),
        (
            "promotion_rejected_by_shape_count",
            stats.promotion_rejected_by_shape_count,
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
            "canonicalization_failed_alias_has_multiple_internal_predecessors_count",
            stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count,
        ),
        (
            "canonicalization_failed_alias_has_nonlocal_ref_count",
            stats.canonicalization_failed_alias_has_nonlocal_ref_count,
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
            "rejected_not_single_pred_succ",
            stats.rejected_not_single_pred_succ,
        ),
        ("rejected_external_entry", stats.rejected_external_entry),
        (
            "rejected_loop_or_switch_target",
            stats.rejected_loop_or_switch_target,
        ),
    ]
}

fn top_build_stats(stats: &NirBuildStats, limit: usize) -> Vec<(String, usize)> {
    let mut pairs = build_stats_pairs(stats)
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .map(|(name, value)| (name.to_string(), value))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(limit);
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

pub fn load_baseline(path: &Path) -> Result<Option<AutomationSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let data =
        fs::read_to_string(path).with_context(|| format!("read baseline {}", path.display()))?;
    let summary = serde_json::from_str(&data)
        .with_context(|| format!("parse baseline {}", path.display()))?;
    Ok(Some(summary))
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
    })
}

pub fn render_markdown(
    summary: &AutomationSummary,
    diagnosis: &DiagnosisReport,
    corpus: &CorpusArtifacts,
    delta: Option<&SummaryDelta>,
    insights: Option<&AutomationDecisionInsights>,
) -> String {
    let mut out = String::new();
    out.push_str("# Fission NIR Automation Summary\n\n");
    out.push_str(&format!("- Lane: `{}`\n", summary.lane));
    out.push_str(&format!("- Run: `{}`\n", summary.run_id));
    out.push_str(&format!("- Generated at: `{}`\n", summary.generated_at));
    out.push_str(&format!(
        "- Recommended next patch: `{}`\n\n",
        summary
            .aggregate
            .recommended_next_patch
            .as_deref()
            .unwrap_or("none")
    ));

    out.push_str("## Aggregate Counts\n\n");
    out.push_str(&format!(
        "- direct_success_count: `{}`\n- nir_failure_count: `{}`\n- explicit_fact_nonzero_count: `{}`\n- strict_explicit_candidate_count: `{}`\n- inventory_surface_gap_count: `{}`\n",
        summary.aggregate.direct_success_count,
        summary.aggregate.nir_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count,
        summary.aggregate.inventory_surface_gap_count,
    ));
    out.push_str(&format!(
        "- source_presence_counts: `{:?}`\n- provenance_surface_totals: `{:?}`\n",
        summary.aggregate.source_presence_counts, summary.aggregate.provenance_surface_totals
    ));
    out.push_str(&format!(
        "- diagnosis_bucket_counts: `{:?}`\n- nir_block_signature_counts: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n\n",
        summary.aggregate.diagnosis_bucket_counts,
        summary.aggregate.nir_block_signature_counts,
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts,
        summary.aggregate.recovery_structuring_mode_counts,
    ));
    let quality = build_quality_measurement(summary);
    out.push_str("## Output Quality\n\n");
    out.push_str(&format!(
        "- nir_output_class_counts: `{:?}`\n- structured_ratio_all_rows: `{:.2}%`\n- structured_ratio_success_rows: `{:.2}%`\n- linear_fallback_ratio_all_rows: `{:.2}%`\n- linear_fallback_ratio_success_rows: `{:.2}%`\n- top_build_stats: `{:?}`\n\n",
        quality.nir_output_class_counts,
        quality.structured_ratio_all_rows * 100.0,
        quality.structured_ratio_success_rows * 100.0,
        quality.linear_fallback_ratio_all_rows * 100.0,
        quality.linear_fallback_ratio_success_rows * 100.0,
        quality.top_build_stats,
    ));

    if let Some(delta) = delta {
        out.push_str("## Baseline Delta\n\n");
        out.push_str(&format!(
            "- direct_success_count: `{:+}`\n- nir_failure_count: `{:+}`\n- explicit_fact_nonzero_count: `{:+}`\n- strict_explicit_candidate_count: `{:+}`\n- inventory_surface_gap_count: `{:+}`\n- pdb_nonzero_rows: `{:+}`\n- region_linearized_count: `{:+}`\n- forced_linear_count: `{:+}`\n- conditional_tail_exit_mismatch_count: `{:+}`\n- body_lowering_failed_count: `{:+}`\n- successor_inline_rejected_count: `{:+}`\n- revisit_cycle_count: `{:+}`\n- unsupported_terminator_count: `{:+}`\n\n",
            delta.direct_success_count,
            delta.nir_failure_count,
            delta.explicit_fact_nonzero_count,
            delta.strict_explicit_candidate_count,
            delta.inventory_surface_gap_count,
            delta.pdb_nonzero_rows,
            delta.region_linearized_count,
            delta.forced_linear_count,
            delta.conditional_tail_exit_mismatch_count,
            delta.body_lowering_failed_count,
            delta.successor_inline_rejected_count,
            delta.revisit_cycle_count,
            delta.unsupported_terminator_count,
        ));
    }

    if let Some(insights) = insights {
        out.push_str("## Conditional-Tail Decision Insights\n\n");
        out.push_str(&format!(
            "- changed_row_count: `{}`\n- go_stop_gate: `{}`\n- rationale: {}\n\n",
            insights.changed_row_count,
            insights.go_stop_gate.decision,
            insights.go_stop_gate.rationale,
        ));
        out.push_str("### mismatch_subtype_ranking\n\n");
        for (name, count) in &insights.mismatch_subtype_ranking {
            out.push_str(&format!("- `{}`: `{}`\n", name, count));
        }
        out.push_str("\n### top_mismatch_rows\n\n");
        for row in insights.top_mismatch_rows.iter().take(8) {
            out.push_str(&format!(
                "- `{}` `{}` mismatch={} failed={} mode={:?} class={:?} subtype={:?}\n",
                row.binary,
                row.address,
                row.mismatch_count,
                row.body_lowering_failed_count,
                row.recovery_structuring_mode,
                row.nir_output_class,
                row.subtype_counts,
            ));
        }
        out.push_str("\n### mismatch_row_deltas\n\n");
        for row in insights.mismatch_row_deltas.iter().take(12) {
            out.push_str(&format!(
                "- `{}` `{}` `{}` baseline={} current={} delta={:+}\n",
                row.binary,
                row.address,
                row.name,
                row.baseline_mismatch_count,
                row.current_mismatch_count,
                row.mismatch_delta,
            ));
        }
        out.push('\n');
    }

    out.push_str("## Per-Binary Highlights\n\n");
    for entry in &diagnosis.binaries {
        out.push_str(&format!("### {}\n\n", entry.binary));
        out.push_str(&format!(
            "- diagnosis: `{}`\n- next_action: `{}`\n- explicit_nonzero_rows: `{}`\n- strict_explicit_candidate_count: `{}`\n- nir_block_signatures: `{:?}`\n- nir_output_class_counts: `{:?}`\n- top_build_stats: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n\n",
            entry.diagnosis_bucket,
            entry.next_action,
            entry.derived_metrics.explicit_nonzero_rows,
            entry.inventory_summary.strict_explicit_candidate_count,
            entry.derived_metrics.blocked_nir_block_signature_counts,
            entry.inventory_summary.nir_output_class_counts,
            top_build_stats(&entry.inventory_summary.nir_build_stats_totals, 6),
            entry.inventory_summary.recovery_strategy_attempted_counts,
            entry.inventory_summary.recovery_outcome_counts,
            entry.inventory_summary.recovery_structuring_mode_counts,
            entry.inventory_summary.recovery_quality_flag_counts,
        ));
    }

    out.push_str("## Suggested Changelog Bullets\n\n");
    out.push_str(&format!(
        "- `fission-automation` lane `{}` aggregated `{}` binaries into a canonical local quality run.\n",
        summary.lane,
        summary.binaries.len()
    ));
    out.push_str(&format!(
        "- aggregate explicit surfacing reached `explicit_fact_nonzero_count = {}` with `strict_explicit_candidate_count = {}`.\n",
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count
    ));
    out.push_str(&format!(
        "- dominant diagnosis is `{:?}` and the current recommended next patch is `{:?}`.\n",
        diagnosis.aggregate.dominant_diagnosis, diagnosis.aggregate.recommended_next_patch
    ));
    out.push_str(&format!(
        "- corpus outputs now include `{}` explicit seeds, `{}` heuristic seeds, and `{}` blocked explicit candidates.\n",
        corpus.quality_explicit_facts.len(),
        corpus.quality_heuristic_surface.len(),
        corpus.blocked_explicit_candidates.len()
    ));
    out
}

pub fn load_baseline_candidates(summary_path: &Path) -> Result<Option<Vec<InventoryRow>>> {
    let Some(parent) = summary_path.parent() else {
        return Ok(None);
    };
    let path = parent.join("nir_quality_candidates.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))?;
    let candidates_value = value
        .get("candidates")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if candidates_value.is_null() {
        return Ok(Some(Vec::new()));
    }
    let candidates: Vec<InventoryRow> = serde_json::from_value(candidates_value)
        .with_context(|| format!("decode candidates {}", path.display()))?;
    Ok(Some(candidates))
}

fn mismatch_subtype_counts_from_stats(stats: &NirBuildStats) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    counts.insert(
        "no_common_follow_in_window".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count,
    );
    counts.insert(
        "follow_beyond_window".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count,
    );
    counts.insert(
        "side_entry_or_exit".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count,
    );
    counts.insert(
        "complex_arm_shape".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count,
    );
    counts.insert(
        "arm_body_lowering_failed".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count,
    );
    counts.insert(
        "ambiguous_multiple_follows".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
    );
    counts
}

pub fn build_decision_insights(
    summary: &AutomationSummary,
    candidates: &[InventoryRow],
    baseline_summary: Option<&AutomationSummary>,
    baseline_candidates: Option<&[InventoryRow]>,
) -> AutomationDecisionInsights {
    let mut subtype_ranking =
        mismatch_subtype_counts_from_stats(&summary.aggregate.nir_build_stats_totals)
            .into_iter()
            .collect::<Vec<_>>();
    subtype_ranking.retain(|(_, count)| *count > 0);
    subtype_ranking.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut top_rows = candidates
        .iter()
        .filter_map(|row| {
            let stats = row.nir_build_stats.as_ref()?;
            let mismatch =
                stats.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count;
            if mismatch == 0 {
                return None;
            }
            Some(MismatchRowSnapshot {
                binary: row.binary.clone(),
                address: row.address.clone(),
                name: row.name.clone(),
                mismatch_count: mismatch,
                body_lowering_failed_count: stats
                    .region_linearize_rejected_body_lowering_failed_count,
                recovery_structuring_mode: row.recovery_structuring_mode.clone(),
                nir_output_class: row.nir_output_class.clone(),
                subtype_counts: mismatch_subtype_counts_from_stats(stats),
            })
        })
        .collect::<Vec<_>>();
    top_rows.sort_by(|a, b| {
        b.mismatch_count
            .cmp(&a.mismatch_count)
            .then_with(|| {
                b.body_lowering_failed_count
                    .cmp(&a.body_lowering_failed_count)
            })
            .then_with(|| a.binary.cmp(&b.binary))
            .then_with(|| a.address.cmp(&b.address))
    });

    let mut baseline_map = BTreeMap::new();
    if let Some(rows) = baseline_candidates {
        for row in rows {
            let mismatch = row
                .nir_build_stats
                .as_ref()
                .map(|s| {
                    s.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
                })
                .unwrap_or(0);
            baseline_map.insert((row.binary.clone(), row.address.clone()), mismatch);
        }
    }

    let mut current_map = BTreeMap::new();
    for row in candidates {
        let mismatch = row
            .nir_build_stats
            .as_ref()
            .map(|s| s.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count)
            .unwrap_or(0);
        current_map.insert(
            (row.binary.clone(), row.address.clone()),
            (row.name.clone(), mismatch),
        );
    }

    let mut row_deltas = Vec::new();
    let mut changed = 0usize;
    for (key, (name, current_mismatch)) in &current_map {
        let baseline_mismatch = *baseline_map.get(key).unwrap_or(&0);
        let delta = *current_mismatch as isize - baseline_mismatch as isize;
        if delta != 0 {
            changed += 1;
        }
        if *current_mismatch > 0 || baseline_mismatch > 0 {
            row_deltas.push(MismatchRowDelta {
                binary: key.0.clone(),
                address: key.1.clone(),
                name: name.clone(),
                baseline_mismatch_count: baseline_mismatch,
                current_mismatch_count: *current_mismatch,
                mismatch_delta: delta,
            });
        }
    }
    row_deltas.sort_by(|a, b| {
        b.current_mismatch_count
            .cmp(&a.current_mismatch_count)
            .then_with(|| b.mismatch_delta.cmp(&a.mismatch_delta))
            .then_with(|| a.binary.cmp(&b.binary))
            .then_with(|| a.address.cmp(&b.address))
    });

    let gate = if let Some(baseline) = baseline_summary {
        let mismatch_delta = summary
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
            as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
                as isize;
        let migration = summary
            .aggregate
            .nir_build_stats_totals
            .region_linearize_rejected_body_lowering_failed_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .region_linearize_rejected_body_lowering_failed_count as isize;
        let dominant_subtype = subtype_ranking.first().map(|(name, _)| name.as_str());
        let safe_dominant = matches!(
            dominant_subtype,
            Some("follow_beyond_window")
                | Some("side_entry_or_exit")
                | Some("no_common_follow_in_window")
        );
        if mismatch_delta < 0 && migration <= 0 && safe_dominant {
            GoStopDecisionGate {
                decision: "go_p5h3g_candidate".to_string(),
                rationale:
                    "mismatch decreased and dominant subtype indicates safely-virtualizable local follow pressure"
                        .to_string(),
            }
        } else {
            GoStopDecisionGate {
                decision: "stop_hold_p5h3f".to_string(),
                rationale: "no mismatch reduction or counter-migration/safety signal is insufficient for P5H3G"
                    .to_string(),
            }
        }
    } else {
        GoStopDecisionGate {
            decision: "stop_no_baseline".to_string(),
            rationale:
                "baseline summary/candidates unavailable; cannot compute stable go/stop gate"
                    .to_string(),
        }
    };

    AutomationDecisionInsights {
        mismatch_subtype_ranking: subtype_ranking,
        top_mismatch_rows: top_rows.into_iter().take(12).collect(),
        mismatch_row_deltas: row_deltas.into_iter().take(32).collect(),
        changed_row_count: changed,
        go_stop_gate: gate,
    }
}

pub fn print_terminal_summary(summary: &AutomationSummary, diagnosis: &DiagnosisReport) {
    let quality = build_quality_measurement(summary);
    println!("[fission-automation] lane={}", summary.lane);
    println!(
        "  direct_success={} nir_failure={} explicit_nonzero={} strict_explicit={}",
        summary.aggregate.direct_success_count,
        summary.aggregate.nir_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count
    );
    println!(
        "  inventory_surface_gap={} pdb_nonzero_rows={}",
        summary.aggregate.inventory_surface_gap_count,
        summary.aggregate.provenance_surface_totals.pdb_nonzero_rows
    );
    println!(
        "  structured_ratio={:.1}% linear_fallback_ratio={:.1}% output_classes={:?}",
        quality.structured_ratio_all_rows * 100.0,
        quality.linear_fallback_ratio_all_rows * 100.0,
        quality.nir_output_class_counts
    );
    println!(
        "  dominant_diagnosis={:?} next_patch={:?}",
        diagnosis.aggregate.dominant_diagnosis, diagnosis.aggregate.recommended_next_patch
    );
    println!(
        "  nir_block_signatures={:?}",
        diagnosis.aggregate.nir_block_signature_counts
    );
    println!(
        "  recovery_attempted={:?} recovery_outcome={:?} recovery_quality_flags={:?}",
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts
    );
    println!("  top_build_stats={:?}", quality.top_build_stats);
}

pub fn update_latest(run_dir: &Path, latest_dir: &Path) -> Result<()> {
    if latest_dir.exists() {
        fs::remove_dir_all(latest_dir)
            .with_context(|| format!("remove {}", latest_dir.display()))?;
    }
    copy_dir_all(run_dir, latest_dir)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_summary(mismatch: usize, failed: usize) -> AutomationSummary {
        AutomationSummary {
            generated_at: "now".to_string(),
            lane: "nir".to_string(),
            run_id: "run".to_string(),
            binaries: Vec::new(),
            aggregate: AggregateSnapshot {
                rows_emitted: 1,
                direct_success_count: 1,
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
                nir_build_stats_totals: NirBuildStats {
                    forced_linear_structuring_count: 2,
                    region_linearize_structuring_count: 1,
                    region_linearize_rejected_body_lowering_failed_count: failed,
                    region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count:
                        mismatch,
                    region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count:
                        3,
                    ..NirBuildStats::default()
                },
            },
        }
    }

    #[test]
    fn decision_insights_marks_stop_when_mismatch_not_improved() {
        let mut row = InventoryRow::default();
        row.binary = "bin".to_string();
        row.address = "0x1000".to_string();
        row.name = "fn".to_string();
        row.recovery_structuring_mode = Some("forced_linear".to_string());
        row.nir_output_class = Some("linear_fallback".to_string());
        row.nir_build_stats = Some(NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 5,
            region_linearize_rejected_body_lowering_failed_count: 1,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 2,
            ..NirBuildStats::default()
        });

        let insights = build_decision_insights(
            &make_summary(5, 1),
            &[row.clone()],
            Some(&make_summary(5, 1)),
            Some(&[row]),
        );

        assert_eq!(insights.go_stop_gate.decision, "stop_hold_p5h3f");
        assert_eq!(insights.changed_row_count, 0);
        assert!(!insights.mismatch_subtype_ranking.is_empty());
    }

    #[test]
    fn decision_insights_marks_go_when_safe_subtype_and_mismatch_drop() {
        let mut current_row = InventoryRow::default();
        current_row.binary = "bin".to_string();
        current_row.address = "0x1000".to_string();
        current_row.name = "fn".to_string();
        current_row.nir_build_stats = Some(NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 2,
            region_linearize_rejected_body_lowering_failed_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 2,
            ..NirBuildStats::default()
        });

        let mut baseline_row = current_row.clone();
        baseline_row.nir_build_stats = Some(NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 6,
            region_linearize_rejected_body_lowering_failed_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 1,
            ..NirBuildStats::default()
        });

        let insights = build_decision_insights(
            &make_summary(2, 0),
            &[current_row],
            Some(&make_summary(6, 0)),
            Some(&[baseline_row]),
        );

        assert_eq!(insights.go_stop_gate.decision, "go_p5h3g_candidate");
        assert_eq!(insights.changed_row_count, 1);
    }
}
