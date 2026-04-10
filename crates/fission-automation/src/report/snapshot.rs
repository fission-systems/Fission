//! Aggregate automation snapshot types.

use crate::model::{ProvenanceSurfaceTotals, SourcePresenceCounts};
use fission_pcode::NirBuildStats;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(default)]
    pub run_profile: String,
    #[serde(default)]
    pub target_count: usize,
    #[serde(default)]
    pub inventory_elapsed_ms: u64,
    #[serde(default)]
    pub diagnosis_elapsed_ms: u64,
    #[serde(default)]
    pub write_outputs_elapsed_ms: u64,
    #[serde(default)]
    pub total_elapsed_ms: u64,
    pub binaries: Vec<BinarySnapshot>,
    pub aggregate: AggregateSnapshot,
}

#[cfg(test)]
mod tests {
    use super::{AggregateSnapshot, AutomationSummary, BinarySnapshot};
    use crate::model::{ProvenanceSurfaceTotals, SourcePresenceCounts};
    use fission_pcode::NirBuildStats;
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[test]
    fn nir_build_stats_json_shape_matches_canonical_type() {
        let stats = NirBuildStats::default();
        let direct: Value = serde_json::to_value(&stats).expect("serialize NirBuildStats");
        let snap = BinarySnapshot {
            binary: "test".into(),
            rows_emitted: 0,
            direct_success_count: 0,
            nir_failure_count: 0,
            explicit_fact_nonzero_count: 0,
            strict_explicit_candidate_count: 0,
            inventory_surface_gap_count: 0,
            source_presence_counts: SourcePresenceCounts::default(),
            provenance_surface_totals: ProvenanceSurfaceTotals::default(),
            recovery_strategy_attempted_counts: BTreeMap::new(),
            recovery_strategy_applied_counts: BTreeMap::new(),
            recovery_outcome_counts: BTreeMap::new(),
            recovery_quality_flag_counts: BTreeMap::new(),
            recovery_structuring_mode_counts: BTreeMap::new(),
            nir_output_class_counts: BTreeMap::new(),
            nir_build_stats_totals: NirBuildStats::default(),
        };
        let embedded = serde_json::to_value(&snap)
            .expect("serialize BinarySnapshot")
            .get("nir_build_stats_totals")
            .expect("nir_build_stats_totals field")
            .clone();
        assert_eq!(
            direct, embedded,
            "BinarySnapshot.nir_build_stats_totals must serialize identically to NirBuildStats; update automation when NirBuildStats changes"
        );
    }

    #[test]
    fn automation_summary_aggregate_includes_nir_build_stats_field() {
        let summary = AutomationSummary {
            generated_at: "t".into(),
            lane: "nir".into(),
            run_id: "r".into(),
            run_profile: "mid".into(),
            target_count: 0,
            inventory_elapsed_ms: 0,
            diagnosis_elapsed_ms: 0,
            write_outputs_elapsed_ms: 0,
            total_elapsed_ms: 0,
            binaries: vec![],
            aggregate: AggregateSnapshot {
                rows_emitted: 0,
                direct_success_count: 0,
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
        };
        let v = serde_json::to_value(&summary).expect("serialize AutomationSummary");
        assert!(
            v.pointer("/aggregate/nir_build_stats_totals").is_some(),
            "summary.json aggregate must expose nir_build_stats_totals"
        );
    }
}
