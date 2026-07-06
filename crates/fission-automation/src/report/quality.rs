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

fn stats_to_map(stats: &NirBuildStats) -> BTreeMap<String, usize> {
    let value = serde_json::to_value(stats).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = value {
        map.into_iter()
            .filter_map(|(k, v)| v.as_u64().map(|num| (k, num as usize)))
            .collect()
    } else {
        BTreeMap::new()
    }
}

pub(crate) fn build_stat_families(stats: &NirBuildStats) -> Vec<(String, usize)> {
    let map = stats_to_map(stats);
    let mut families: BTreeMap<String, usize> = BTreeMap::new();
    
    for (k, v) in map {
        if v == 0 { continue; }
        
        let family = if k.contains("abi_") || k.contains("param_promotion_spill") || k.contains("home_slot_") {
            "abi"
        } else if k.contains("object_shape") || k.contains("object_root") || k.contains("surface_binding") || k.contains("surface_fact") || k == "call_artifact_removed_count" {
            "memory_shape"
        } else if k.contains("variadic") || k.contains("va_start") {
            "variadic"
        } else if k.contains("call_signature") || k.contains("prototype_summary") || k.contains("call_effect") || k.contains("wrapper_summary") || k.contains("interproc_signature") || k.contains("unsupported_indirect_call") || k.contains("indirect_target_set") {
            "call_signature"
        } else if k.starts_with("cleanup_family_binding_init") {
            "cleanup_binding_init"
        } else if k.starts_with("cleanup_family_stmt_canonical") {
            "cleanup_stmt_canonical"
        } else if k.starts_with("cleanup_stmt_fold") {
            "cleanup_stmt_fold"
        } else if k.starts_with("cleanup_boundary_label") {
            "cleanup_boundary_label"
        } else if k.starts_with("cleanup_loopish_rewrite") {
            "cleanup_loopish_rewrite"
        } else if k.starts_with("cleanup_family_dead_binding") {
            "cleanup_dead_binding"
        } else if k.starts_with("cleanup_budget") {
            "cleanup_budget"
        } else if k.starts_with("dispatcher_") || k == "switch_emit_ready_failed_count" || k.contains("unsupported_indirect_control") || k.contains("unsupported_external_target") || k.contains("indirect_surface_preserved") {
            "dispatcher"
        } else if k.contains("security_cookie") {
            "security"
        } else if k.contains("guarded_tail_replacement_plan_rejected") || k.starts_with("guarded_tail_rejected_") || k.contains("ambiguous_follow") || k == "region_emit_ready_failed_count" {
            "canonical_rewrite_conflict"
        } else if k.contains("structuring") || k.contains("region_") || k.contains("promotion_") || k.contains("promoted_region") {
            "structuring"
        } else {
            "other"
        };
        
        *families.entry(family.to_string()).or_insert(0) += v;
    }
    
    let mut pairs: Vec<_> = families.into_iter().filter(|(k, v)| *v > 0 && k != "other").collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs
}

pub(crate) fn top_build_stats(stats: &NirBuildStats, limit: usize) -> Vec<(String, usize)> {
    let mut pairs: Vec<_> = stats_to_map(stats)
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(limit);
    pairs
}

pub(crate) fn structuring_fallback_reasons(stats: &NirBuildStats) -> Vec<(String, usize)> {
    let mut pairs = Vec::new();
    let map = stats_to_map(stats);
    for (k, v) in map {
        if v == 0 {
            continue;
        }
        if k.starts_with("region_linearize_rejected_") {
            let mut reason = k.strip_prefix("region_linearize_rejected_").unwrap_or(&k).strip_suffix("_count").unwrap_or(&k).to_string();
            reason = reason.replace("body_lowering_conditional_tail_", "cond_tail_");
            reason = reason.replace("body_lowering_", "");
            pairs.push((reason, v));
        }
    }
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
