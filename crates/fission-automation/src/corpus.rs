use crate::model::{
    AlignedExplicitCandidate, BlockedExplicitCandidate, CuratedQualityEntry,
    ExplicitBreakdownTotals, FactSourcesPresent, InventoryRow, InventorySummary, SourceMeta,
    SourcePresenceCounts,
};
use fission_pcode::{IndirectControlClassification, NirBuildStats};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct InventorySummaryTotals {
    pub functions_total: usize,
    pub rows_emitted: usize,
    pub direct_success_count: usize,
    pub nir_failure_count: usize,
    pub panic_recovered_count: usize,
    pub explicit_fact_nonzero_count: usize,
    pub strict_explicit_candidate_count: usize,
    pub heuristic_surface_candidate_count: usize,
    pub inventory_surface_gap_count: usize,
    pub aligned_with_zero_explicit_count: usize,
    pub source_presence_counts: SourcePresenceCounts,
    pub explicit_breakdown_totals: ExplicitBreakdownTotals,
    pub failure_kind_counts: BTreeMap<String, usize>,
    pub row_error_kind_counts: BTreeMap<String, usize>,
}

impl Default for InventorySummaryTotals {
    fn default() -> Self {
        Self {
            functions_total: 0,
            rows_emitted: 0,
            direct_success_count: 0,
            nir_failure_count: 0,
            panic_recovered_count: 0,
            explicit_fact_nonzero_count: 0,
            strict_explicit_candidate_count: 0,
            heuristic_surface_candidate_count: 0,
            inventory_surface_gap_count: 0,
            aligned_with_zero_explicit_count: 0,
            source_presence_counts: SourcePresenceCounts::default(),
            explicit_breakdown_totals: ExplicitBreakdownTotals::default(),
            failure_kind_counts: BTreeMap::new(),
            row_error_kind_counts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CorpusArtifacts {
    pub candidates: Vec<InventoryRow>,
    pub quality_explicit_facts: Vec<CuratedQualityEntry>,
    pub quality_heuristic_surface: Vec<CuratedQualityEntry>,
    pub blocked_explicit_candidates: Vec<BlockedExplicitCandidate>,
    pub aligned_explicit_candidates: Vec<AlignedExplicitCandidate>,
    pub inventory_summary_totals: InventorySummaryTotals,
    pub block_reason_counts: BTreeMap<String, usize>,
    pub timeout_rescue: BTreeMap<String, Vec<String>>,
}

pub fn explicit_fact_total(entry: &InventoryRow) -> usize {
    entry.explicit_fact_total
}

fn normalize_address(value: &str) -> String {
    let mut text = value.trim().to_ascii_lowercase();
    if let Some(stripped) = text.strip_prefix("0x") {
        text = stripped.to_string();
    }
    let trimmed = text.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn canonical_indirect_classification(
    stats: Option<&NirBuildStats>,
) -> IndirectControlClassification {
    IndirectControlClassification::from_stats_only(stats)
}

pub fn source_is_nir_aligned(source_meta: Option<&SourceMeta>) -> bool {
    let Some(source_meta) = source_meta else {
        return true;
    };
    if let Some(alignment) = source_meta.admission_alignment.as_deref() {
        return alignment == "aligned";
    }
    if matches!(source_meta.expected_nir_supported, Some(false)) {
        return false;
    }
    !matches!(
        source_meta.observed_nir_failure_kind.as_deref(),
        Some(
            "nir_architecture_unsupported"
                | "nir_format_unsupported"
                | "preview_architecture_unsupported"
                | "preview_format_unsupported"
        )
    )
}

pub fn candidate_passes_explicit_quality_prefilter(
    entry: &InventoryRow,
    source_meta: Option<&SourceMeta>,
) -> bool {
    let indirect = canonical_indirect_classification(entry.nir_build_stats.as_ref());
    source_is_nir_aligned(source_meta)
        && explicit_fact_total(entry) >= 2
        && entry.nir_direct_success
        && indirect.allows_strict_explicit_candidate(entry.pcode_op_count)
}

pub fn candidate_passes_heuristic_quality_prefilter(entry: &InventoryRow) -> bool {
    entry.heuristic_surface_candidate
}

pub fn aligned_explicit_candidate_entry(
    entry: &InventoryRow,
    source_meta: Option<&SourceMeta>,
) -> AlignedExplicitCandidate {
    AlignedExplicitCandidate {
        binary: entry.binary.clone(),
        path: entry.binary_path.clone(),
        address: format!("0x{}", normalize_address(&entry.address)),
        name: entry.name.clone(),
        explicit_fact_total: explicit_fact_total(entry),
        fact_density_score: entry.fact_density_score,
        nir_direct_success: entry.nir_direct_success,
        nir_fallback_kind_refined: entry.nir_fallback_kind_refined.clone(),
        pcode_op_count: entry.pcode_op_count,
        nir_surface_kind: entry.nir_surface_kind.clone(),
        fact_sources_present: FactSourcesPresent {
            dwarf: entry.fact_sources_present.dwarf,
            pdb: entry.fact_sources_present.pdb,
            loader: entry.fact_sources_present.loader,
            native_inferred: entry.fact_sources_present.native_inferred,
        },
        explicit_fact_breakdown: entry.explicit_fact_breakdown.clone(),
        provenance_fact_breakdown: entry.provenance_fact_breakdown.clone(),
        admission_block_stage: entry.admission_block_stage.clone(),
        inventory_surface_gap: entry.inventory_surface_gap,
        reason_tags: entry.reason_tags.clone(),
        source_binary: source_meta
            .map(|meta| meta.binary.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| entry.binary.clone()),
        source_admission_alignment: source_meta.and_then(|meta| meta.admission_alignment.clone()),
    }
}

pub fn blocked_explicit_candidate_entry(
    entry: &InventoryRow,
    source_meta: Option<&SourceMeta>,
) -> BlockedExplicitCandidate {
    BlockedExplicitCandidate {
        binary: entry.binary.clone(),
        path: entry.binary_path.clone(),
        address: format!("0x{}", normalize_address(&entry.address)),
        name: entry.name.clone(),
        explicit_fact_total: explicit_fact_total(entry),
        fact_density_score: entry.fact_density_score,
        nir_direct_success: entry.nir_direct_success,
        block_reason: entry
            .row_error_kind
            .clone()
            .or_else(|| entry.nir_fallback_kind_refined.clone())
            .unwrap_or_else(|| "strict_filter_reject".to_string()),
        pcode_op_count: entry.pcode_op_count,
        has_indirect_control_flow: entry.has_indirect_control_flow,
        fact_sources_present: entry.fact_sources_present.clone(),
        explicit_fact_breakdown: entry.explicit_fact_breakdown.clone(),
        provenance_fact_breakdown: entry.provenance_fact_breakdown.clone(),
        admission_block_stage: entry.admission_block_stage.clone(),
        inventory_surface_gap: entry.inventory_surface_gap,
        reason_tags: entry.reason_tags.clone(),
        source_admission_alignment: source_meta.and_then(|meta| meta.admission_alignment.clone()),
    }
}

pub fn curated_quality_entry(entry: &InventoryRow) -> CuratedQualityEntry {
    CuratedQualityEntry {
        binary: entry.binary.clone(),
        address: format!("0x{}", normalize_address(&entry.address)),
        name: entry.name.clone(),
        fact_density_score: entry.fact_density_score,
        quality_potential_score: entry.fact_density_score,
        reason_tags: entry.reason_tags.clone(),
    }
}

fn dedupe_by_key<T, F>(entries: Vec<T>, key_fn: F) -> Vec<T>
where
    F: Fn(&T) -> (String, String),
{
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for entry in entries {
        let key = key_fn(&entry);
        if seen.insert(key) {
            deduped.push(entry);
        }
    }
    deduped
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (key, value) in source {
        *target.entry(key.clone()).or_default() += *value;
    }
}

fn update_totals(totals: &mut InventorySummaryTotals, summary: &InventorySummary) {
    totals.functions_total += summary.functions_total;
    totals.rows_emitted += summary.rows_emitted;
    totals.direct_success_count += summary.direct_success_count;
    totals.nir_failure_count += summary.nir_failure_count;
    totals.panic_recovered_count += summary.panic_recovered_count;
    totals.explicit_fact_nonzero_count += summary.explicit_fact_nonzero_count;
    totals.strict_explicit_candidate_count += summary.strict_explicit_candidate_count;
    totals.heuristic_surface_candidate_count += summary.heuristic_surface_candidate_count;
    totals.inventory_surface_gap_count += summary.inventory_surface_gap_count;
    totals.aligned_with_zero_explicit_count += summary.aligned_with_zero_explicit_count;
    totals.source_presence_counts.dwarf += summary.source_presence_counts.dwarf;
    totals.source_presence_counts.pdb += summary.source_presence_counts.pdb;
    totals.source_presence_counts.loader += summary.source_presence_counts.loader;
    totals.source_presence_counts.native_inferred += summary.source_presence_counts.native_inferred;
    totals.explicit_breakdown_totals.param_count += summary.explicit_breakdown_totals.param_count;
    totals.explicit_breakdown_totals.local_count += summary.explicit_breakdown_totals.local_count;
    totals.explicit_breakdown_totals.return_count += summary.explicit_breakdown_totals.return_count;
    totals.explicit_breakdown_totals.pdb_type_count +=
        summary.explicit_breakdown_totals.pdb_type_count;
    totals.explicit_breakdown_totals.native_type_count +=
        summary.explicit_breakdown_totals.native_type_count;
    merge_counts(
        &mut totals.failure_kind_counts,
        &summary.failure_kind_counts,
    );
    merge_counts(
        &mut totals.row_error_kind_counts,
        &summary.row_error_kind_counts,
    );
}

pub fn load_timeout_rescue(root: &Path) -> BTreeMap<String, Vec<String>> {
    let path = root
        .join("crates")
        .join("fission-automation")
        .join("config")
        .join("timeout_rescue.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    let Ok(timeout_rescue) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(&data) else {
        return BTreeMap::new();
    };
    timeout_rescue
}

pub fn build_corpus_artifacts(
    root: &Path,
    datasets: &[(
        crate::model::LaneTarget,
        InventorySummary,
        Vec<InventoryRow>,
        Option<SourceMeta>,
    )],
) -> CorpusArtifacts {
    let mut all_candidates = Vec::new();
    let mut explicit_entries = Vec::new();
    let mut heuristic_entries = Vec::new();
    let mut blocked_entries = Vec::new();
    let mut aligned_entries = Vec::new();
    let mut totals = InventorySummaryTotals::default();

    for (_lane_target, summary, rows, source_meta) in datasets {
        update_totals(&mut totals, summary);
        for row in rows {
            all_candidates.push(row.clone());
            if candidate_passes_explicit_quality_prefilter(row, source_meta.as_ref()) {
                explicit_entries.push(curated_quality_entry(row));
            }
            if candidate_passes_heuristic_quality_prefilter(row) {
                heuristic_entries.push(curated_quality_entry(row));
            }
            if source_meta
                .as_ref()
                .and_then(|meta| meta.admission_alignment.as_deref())
                == Some("aligned")
            {
                aligned_entries.push(aligned_explicit_candidate_entry(row, source_meta.as_ref()));
            }
            if explicit_fact_total(row) > 0
                && !candidate_passes_explicit_quality_prefilter(row, source_meta.as_ref())
            {
                blocked_entries.push(blocked_explicit_candidate_entry(row, source_meta.as_ref()));
            }
        }
    }

    let mut explicit_entries = dedupe_by_key(explicit_entries, |entry| {
        (entry.binary.clone(), entry.address.clone())
    });
    explicit_entries.sort_by(|a, b| {
        (b.fact_density_score, b.quality_potential_score, &a.address).cmp(&(
            a.fact_density_score,
            a.quality_potential_score,
            &b.address,
        ))
    });
    let explicit_keys: BTreeSet<(String, String)> = explicit_entries
        .iter()
        .map(|entry| (entry.binary.clone(), entry.address.clone()))
        .collect();

    let mut heuristic_entries = dedupe_by_key(heuristic_entries, |entry| {
        (entry.binary.clone(), entry.address.clone())
    });
    heuristic_entries
        .retain(|entry| !explicit_keys.contains(&(entry.binary.clone(), entry.address.clone())));
    heuristic_entries.sort_by(|a, b| {
        (b.fact_density_score, b.quality_potential_score, &a.address).cmp(&(
            a.fact_density_score,
            a.quality_potential_score,
            &b.address,
        ))
    });

    let mut blocked_entries = dedupe_by_key(blocked_entries, |entry| {
        (entry.binary.clone(), entry.address.clone())
    });
    blocked_entries.sort_by(|a, b| {
        candidate_sort_tuple(
            b.explicit_fact_total,
            b.fact_density_score,
            b.pcode_op_count,
        )
        .cmp(&candidate_sort_tuple(
            a.explicit_fact_total,
            a.fact_density_score,
            a.pcode_op_count,
        ))
    });

    let mut aligned_entries = dedupe_by_key(aligned_entries, |entry| {
        (entry.binary.clone(), entry.address.clone())
    });
    aligned_entries.sort_by(|a, b| {
        candidate_sort_tuple(
            b.explicit_fact_total,
            b.fact_density_score,
            b.pcode_op_count,
        )
        .cmp(&candidate_sort_tuple(
            a.explicit_fact_total,
            a.fact_density_score,
            a.pcode_op_count,
        ))
    });

    let mut block_reason_counts = BTreeMap::new();
    for entry in &blocked_entries {
        *block_reason_counts
            .entry(entry.block_reason.clone())
            .or_default() += 1;
    }

    CorpusArtifacts {
        candidates: all_candidates,
        quality_explicit_facts: explicit_entries,
        quality_heuristic_surface: heuristic_entries,
        blocked_explicit_candidates: blocked_entries,
        aligned_explicit_candidates: aligned_entries,
        inventory_summary_totals: totals,
        block_reason_counts,
        timeout_rescue: load_timeout_rescue(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_prefer_over_raw_flags_for_explicit_quality_prefilter() {
        let classification = canonical_indirect_classification(Some(&NirBuildStats {
            dispatcher_shape_recovered_count: 1,
            ..Default::default()
        }));

        assert!(classification.has_indirect_control);
        assert!(classification.has_dispatcher_recovery);
        assert!(!classification.has_unresolved_unsupported_indirect);
    }

    #[test]
    fn missing_stats_fail_closed_for_indirect_classification() {
        let classification = canonical_indirect_classification(None);

        assert!(!classification.has_indirect_control);
        assert!(!classification.has_preserved_indirect_surface);
        assert!(!classification.has_unresolved_unsupported_indirect);
    }
}

fn candidate_sort_tuple(
    explicit_fact_total: usize,
    fact_density_score: i32,
    pcode_op_count: usize,
) -> (usize, i32, i64) {
    (
        explicit_fact_total,
        fact_density_score,
        -(pcode_op_count as i64),
    )
}
