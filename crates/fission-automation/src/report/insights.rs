//! Go/stop gate and mismatch-focused decision helpers.

use crate::model::InventoryRow;
use crate::report::quality::build_stat_families;
use crate::report::snapshot::AutomationSummary;
use fission_pcode::NirBuildStats;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub struct QualityDeltaFamily {
    pub family: String,
    pub baseline: usize,
    pub current: usize,
    pub delta: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDecisionInsights {
    pub mismatch_subtype_ranking: Vec<(String, usize)>,
    pub top_mismatch_rows: Vec<MismatchRowSnapshot>,
    pub mismatch_row_deltas: Vec<MismatchRowDelta>,
    pub quality_delta_vector: Vec<QualityDeltaFamily>,
    pub changed_row_count: usize,
    pub go_stop_gate: GoStopDecisionGate,
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
        "depth_or_budget_exhausted".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count,
    );
    counts.insert(
        "one_arm_body_lowering_failed".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count,
    );
    counts.insert(
        "both_arms_body_lowering_failed".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count,
    );
    counts.insert(
        "follow_tail_lowering_failed".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count,
    );
    counts.insert(
        "ambiguous_multiple_follows".to_string(),
        stats.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count,
    );
    counts
}

fn structuring_family_counts_from_stats(stats: &NirBuildStats) -> BTreeMap<String, usize> {
    BTreeMap::from([
        (
            "region_legality".to_string(),
            stats.structuring_reason_region_legality_count,
        ),
        (
            "follow_failure".to_string(),
            stats.structuring_reason_follow_failure_count,
        ),
        (
            "irreducible".to_string(),
            stats.structuring_reason_irreducible_count,
        ),
        (
            "loop_exit".to_string(),
            stats.structuring_reason_loop_exit_count,
        ),
        (
            "switch_shape".to_string(),
            stats.structuring_reason_switch_shape_count,
        ),
        ("budget".to_string(), stats.structuring_reason_budget_count),
    ])
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
        let family_map = |stats: &NirBuildStats| {
            build_stat_families(stats)
                .into_iter()
                .collect::<BTreeMap<_, _>>()
        };
        let baseline_families = family_map(&baseline.aggregate.nir_build_stats_totals);
        let current_families = family_map(&summary.aggregate.nir_build_stats_totals);
        let structuring_delta = current_families.get("structuring").copied().unwrap_or(0) as isize
            - baseline_families.get("structuring").copied().unwrap_or(0) as isize;
        let abi_delta = current_families.get("abi").copied().unwrap_or(0) as isize
            - baseline_families.get("abi").copied().unwrap_or(0) as isize;
        let variadic_delta = current_families.get("variadic").copied().unwrap_or(0) as isize
            - baseline_families.get("variadic").copied().unwrap_or(0) as isize;
        let call_signature_delta = current_families.get("call_signature").copied().unwrap_or(0)
            as isize
            - baseline_families
                .get("call_signature")
                .copied()
                .unwrap_or(0) as isize;
        let security_delta = current_families.get("security").copied().unwrap_or(0) as isize
            - baseline_families.get("security").copied().unwrap_or(0) as isize;
        let has_material_improvement = structuring_delta < 0
            || abi_delta > 0
            || variadic_delta > 0
            || call_signature_delta > 0
            || security_delta > 0;
        let family_ranking =
            structuring_family_counts_from_stats(&summary.aggregate.nir_build_stats_totals)
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .collect::<Vec<_>>();
        let dominant_family = family_ranking
            .iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(name, _)| name.as_str());
        let safe_dominant = matches!(
            dominant_family,
            Some("follow_failure") | Some("loop_exit") | Some("region_legality")
        );
        let irreducible_scc_delta = summary
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_scc_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_scc_count as isize;
        let irreducible_header_delta = summary
            .aggregate
            .nir_build_stats_totals
            .structuring_irreducible_header_count as isize
            - baseline
                .aggregate
                .nir_build_stats_totals
                .structuring_irreducible_header_count as isize;
        let has_any_row_regression = row_deltas.iter().any(|d| d.mismatch_delta > 0);

        if has_material_improvement
            && structuring_delta <= 0
            && safe_dominant
            && irreducible_scc_delta <= 0
            && irreducible_header_delta <= 0
            && abi_delta >= 0
            && variadic_delta >= 0
            && call_signature_delta >= 0
            && security_delta >= 0
            && !has_any_row_regression
        {
            GoStopDecisionGate {
                decision: "go_p5h3g_candidate".to_string(),
                rationale: "semantic family deltas are non-regressive and no row-level regressions detected"
                    .to_string(),
            }
        } else if has_any_row_regression {
            GoStopDecisionGate {
                decision: "stop_row_level_regression".to_string(),
                rationale: "row-level regression detected (zero-tolerance policy)".to_string(),
            }
        } else {
            GoStopDecisionGate {
                decision: "stop_hold_p5h3f".to_string(),
                rationale: "semantic family delta vector is regressive or structuring safety signals are insufficient"
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

    let quality_delta_vector = if let Some(baseline) = baseline_summary {
        let baseline_families = build_stat_families(&baseline.aggregate.nir_build_stats_totals)
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let current_families = build_stat_families(&summary.aggregate.nir_build_stats_totals)
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let mut family_names = baseline_families
            .keys()
            .chain(current_families.keys())
            .cloned()
            .collect::<Vec<_>>();
        family_names.sort();
        family_names.dedup();
        family_names
            .into_iter()
            .map(|family| {
                let baseline = baseline_families.get(&family).copied().unwrap_or(0);
                let current = current_families.get(&family).copied().unwrap_or(0);
                QualityDeltaFamily {
                    family,
                    baseline,
                    current,
                    delta: current as isize - baseline as isize,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    AutomationDecisionInsights {
        mismatch_subtype_ranking: subtype_ranking,
        top_mismatch_rows: top_rows.into_iter().take(12).collect(),
        mismatch_row_deltas: row_deltas.into_iter().take(32).collect(),
        quality_delta_vector,
        changed_row_count: changed,
        go_stop_gate: gate,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProvenanceSurfaceTotals, SourcePresenceCounts};
    use crate::report::snapshot::{AggregateSnapshot, AutomationSummary};

    fn make_summary(mismatch: usize, failed: usize) -> AutomationSummary {
        let mut stats = NirBuildStats {
            forced_linear_structuring_count: 2,
            region_linearize_structuring_count: 1,
            region_linearize_rejected_body_lowering_failed_count: failed,
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: mismatch,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 3,
            ..NirBuildStats::default()
        };
        stats.refresh_structuring_reason_families();
        AutomationSummary {
            generated_at: "now".to_string(),
            lane: "nir".to_string(),
            run_id: "run".to_string(),
            run_profile: "mid".to_string(),
            target_count: 1,
            inventory_elapsed_ms: 0,
            diagnosis_elapsed_ms: 0,
            write_outputs_elapsed_ms: 0,
            total_elapsed_ms: 0,
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
                nir_build_stats_totals: stats,
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
        let mut row_stats = NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 5,
            region_linearize_rejected_body_lowering_failed_count: 1,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 2,
            ..NirBuildStats::default()
        };
        row_stats.refresh_structuring_reason_families();
        row.nir_build_stats = Some(row_stats);

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
        let mut current_stats = NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 2,
            region_linearize_rejected_body_lowering_failed_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 2,
            ..NirBuildStats::default()
        };
        current_stats.refresh_structuring_reason_families();
        current_row.nir_build_stats = Some(current_stats);

        let mut baseline_row = current_row.clone();
        let mut baseline_stats = NirBuildStats {
            region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: 6,
            region_linearize_rejected_body_lowering_failed_count: 0,
            region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: 1,
            ..NirBuildStats::default()
        };
        baseline_stats.refresh_structuring_reason_families();
        baseline_row.nir_build_stats = Some(baseline_stats);

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
