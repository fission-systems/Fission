use crate::corpus::{
    aligned_explicit_candidate_entry, blocked_explicit_candidate_entry,
    candidate_passes_explicit_quality_prefilter, explicit_fact_total,
};
use crate::model::{
    AlignedExplicitCandidate, BlockedExplicitCandidate, InventoryRow, InventorySummary, SourceMeta,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct SourceMetaView {
    pub admission_alignment: Option<String>,
    pub rescan_priority: Option<String>,
    pub expected_nir_supported: Option<bool>,
    pub observed_nir_supported: Option<bool>,
    pub observed_nir_failure_kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosisDerivedMetrics {
    pub source_present_rows: usize,
    pub explicit_nonzero_rows: usize,
    pub inventory_surface_gap_count: usize,
    pub aligned_with_zero_explicit_count: usize,
    pub aligned_candidate_count: usize,
    pub blocked_candidate_count: usize,
    pub blocked_admission_stage_counts: BTreeMap<String, usize>,
    pub blocked_nir_block_signature_counts: BTreeMap<String, usize>,
    pub source_presence_counts: crate::model::SourcePresenceCounts,
    pub provenance_surface_totals: crate::model::ProvenanceSurfaceTotals,
    pub pdb_source_without_pdb_surface: bool,
}

#[derive(Debug, Serialize)]
pub struct DiagnosisBinaryEntry {
    pub binary: String,
    pub binary_path: String,
    pub source_meta: SourceMetaView,
    pub inventory_summary: InventorySummary,
    pub derived_metrics: DiagnosisDerivedMetrics,
    pub diagnosis_bucket: String,
    pub next_action: String,
    pub diagnosis_rationale: String,
    pub top_aligned_candidates: Vec<AlignedExplicitCandidate>,
    pub blocked_candidates: Vec<BlockedExplicitCandidate>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosisAggregate {
    pub diagnosis_bucket_counts: BTreeMap<String, usize>,
    pub next_action_counts: BTreeMap<String, usize>,
    pub nir_block_signature_counts: BTreeMap<String, usize>,
    pub dominant_diagnosis: Option<String>,
    pub recommended_next_patch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosisReport {
    pub generated_at: String,
    pub source_inventory_file: Option<String>,
    pub binaries: Vec<DiagnosisBinaryEntry>,
    pub aggregate: DiagnosisAggregate,
}

fn count_rows_with_any_source(rows: &[InventoryRow]) -> usize {
    rows.iter()
        .filter(|row| {
            let s = &row.fact_sources_present;
            s.dwarf || s.pdb || s.loader || s.native_inferred
        })
        .count()
}

fn stage_counts(entries: &[BlockedExplicitCandidate]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        *counts
            .entry(if entry.admission_block_stage.is_empty() {
                "none".to_string()
            } else {
                entry.admission_block_stage.clone()
            })
            .or_default() += 1;
    }
    counts
}

fn nir_block_signature_counts(rows: &[InventoryRow]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        if let Some(signature) = row.nir_block_signature.as_ref() {
            *counts.entry(signature.clone()).or_default() += 1;
        }
    }
    counts
}

fn classify_diagnosis(
    rows_emitted: usize,
    source_present_rows: usize,
    explicit_nonzero_rows: usize,
    inventory_surface_gap_count: usize,
    blocked_stage_counts: &BTreeMap<String, usize>,
    source_presence_counts: &crate::model::SourcePresenceCounts,
    provenance_surface_totals: &crate::model::ProvenanceSurfaceTotals,
) -> (String, String, String) {
    let rows = rows_emitted.max(1);
    let source_presence_ratio = source_present_rows as f64 / rows as f64;
    let explicit_nonzero_ratio = explicit_nonzero_rows as f64 / rows as f64;
    let surface_gap_ratio = inventory_surface_gap_count as f64 / rows as f64;
    let preview_stage_blocks = blocked_stage_counts.get("preview").copied().unwrap_or(0)
        + blocked_stage_counts.get("admission").copied().unwrap_or(0);
    let pdb_sources = source_presence_counts.pdb;
    let pdb_surface_rows = provenance_surface_totals.pdb_nonzero_rows;
    let native_surface_rows = provenance_surface_totals.native_nonzero_rows;

    if pdb_sources > 0 && pdb_surface_rows == 0 && native_surface_rows > 0 {
        return (
            "mixed_or_inconclusive".to_string(),
            "factstore_inventory_patch".to_string(),
            "PDB source presence is visible, but surfaced explicit rows are still being supplied by native inferred facts instead of PDB-derived facts".to_string(),
        );
    }
    if explicit_nonzero_rows > 0 && preview_stage_blocks > 0 {
        return (
            "preview_stage_block".to_string(),
            "preview_side_patch".to_string(),
            "explicit facts exist, but blocked candidates concentrate in preview/admission stages"
                .to_string(),
        );
    }
    if source_present_rows > 0 && explicit_nonzero_rows == 0 && inventory_surface_gap_count > 0 {
        return (
            "factstore_or_inventory_surface_gap".to_string(),
            "factstore_inventory_patch".to_string(),
            "source provenance is present while explicit rows stay at zero and inventory surface gaps are observed".to_string(),
        );
    }
    if source_present_rows == 0 && explicit_nonzero_rows == 0 && inventory_surface_gap_count == 0 {
        return (
            "source_facts_absent".to_string(),
            "source_expansion".to_string(),
            "no source provenance or explicit rows were observed in the inventory".to_string(),
        );
    }
    if source_presence_ratio <= 0.05 && explicit_nonzero_ratio == 0.0 && surface_gap_ratio == 0.0 {
        return (
            "source_facts_absent".to_string(),
            "source_expansion".to_string(),
            "source provenance coverage is negligible and explicit rows remain absent".to_string(),
        );
    }
    if inventory_surface_gap_count > 0 && explicit_nonzero_rows == 0 {
        return (
            "factstore_or_inventory_surface_gap".to_string(),
            "factstore_inventory_patch".to_string(),
            "provenance reaches inventory rows, but explicit surfacing is still absent".to_string(),
        );
    }
    if preview_stage_blocks > 0 {
        return (
            "preview_stage_block".to_string(),
            "preview_side_patch".to_string(),
            "blocked explicit candidates cluster in preview/admission stages".to_string(),
        );
    }
    (
        "mixed_or_inconclusive".to_string(),
        if source_present_rows == 0 {
            "source_expansion".to_string()
        } else {
            "factstore_inventory_patch".to_string()
        },
        "inventory does not show a single dominant bottleneck yet".to_string(),
    )
}

pub fn diagnosis_entry(
    binary_path: &Path,
    rows: &[InventoryRow],
    summary: &InventorySummary,
    source_meta: Option<&SourceMeta>,
) -> DiagnosisBinaryEntry {
    let mut aligned_candidates = Vec::new();
    let mut blocked_candidates = Vec::new();
    for row in rows {
        if source_meta.and_then(|meta| meta.admission_alignment.as_deref()) == Some("aligned") {
            aligned_candidates.push(aligned_explicit_candidate_entry(row, source_meta));
        }
        if explicit_fact_total(row) > 0
            && !candidate_passes_explicit_quality_prefilter(row, source_meta)
        {
            blocked_candidates.push(blocked_explicit_candidate_entry(row, source_meta));
        }
    }
    dedupe_candidate_vec(&mut aligned_candidates, |v| {
        (v.binary.clone(), v.address.clone())
    });
    dedupe_candidate_vec(&mut blocked_candidates, |v| {
        (v.binary.clone(), v.address.clone())
    });
    aligned_candidates.sort_by(|a, b| {
        (
            b.explicit_fact_total,
            b.fact_density_score,
            -(b.pcode_op_count as i64),
        )
            .cmp(&(
                a.explicit_fact_total,
                a.fact_density_score,
                -(a.pcode_op_count as i64),
            ))
    });
    blocked_candidates.sort_by(|a, b| {
        (
            b.explicit_fact_total,
            b.fact_density_score,
            -(b.pcode_op_count as i64),
        )
            .cmp(&(
                a.explicit_fact_total,
                a.fact_density_score,
                -(a.pcode_op_count as i64),
            ))
    });

    let rows_emitted = if summary.rows_emitted == 0 {
        rows.len()
    } else {
        summary.rows_emitted
    };
    let source_present_rows = count_rows_with_any_source(rows);
    let blocked_admission_stage_counts = stage_counts(&blocked_candidates);
    let blocked_preview_rows: Vec<InventoryRow> = rows
        .iter()
        .filter(|row| row.admission_block_stage == "preview" || row.admission_block_stage == "nir")
        .cloned()
        .collect();
    let blocked_nir_block_signature_counts = nir_block_signature_counts(&blocked_preview_rows);
    let (diagnosis_bucket, next_action, rationale) = classify_diagnosis(
        rows_emitted,
        source_present_rows,
        summary.explicit_fact_nonzero_count,
        summary.inventory_surface_gap_count,
        &blocked_admission_stage_counts,
        &summary.source_presence_counts,
        &summary.provenance_surface_totals,
    );

    DiagnosisBinaryEntry {
        binary: binary_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string(),
        binary_path: binary_path.to_string_lossy().to_string(),
        source_meta: SourceMetaView {
            admission_alignment: source_meta.and_then(|meta| meta.admission_alignment.clone()),
            rescan_priority: source_meta.and_then(|meta| meta.rescan_priority.clone()),
            expected_nir_supported: source_meta.and_then(|meta| meta.expected_nir_supported),
            observed_nir_supported: source_meta.and_then(|meta| meta.observed_nir_supported),
            observed_nir_failure_kind: source_meta
                .and_then(|meta| meta.observed_nir_failure_kind.clone()),
        },
        inventory_summary: summary.clone(),
        derived_metrics: DiagnosisDerivedMetrics {
            source_present_rows,
            explicit_nonzero_rows: summary.explicit_fact_nonzero_count,
            inventory_surface_gap_count: summary.inventory_surface_gap_count,
            aligned_with_zero_explicit_count: summary.aligned_with_zero_explicit_count,
            aligned_candidate_count: aligned_candidates.len(),
            blocked_candidate_count: blocked_candidates.len(),
            blocked_admission_stage_counts,
            blocked_nir_block_signature_counts,
            source_presence_counts: summary.source_presence_counts.clone(),
            provenance_surface_totals: summary.provenance_surface_totals.clone(),
            pdb_source_without_pdb_surface: summary.source_presence_counts.pdb > 0
                && summary.provenance_surface_totals.pdb_nonzero_rows == 0,
        },
        diagnosis_bucket,
        next_action,
        diagnosis_rationale: rationale,
        top_aligned_candidates: aligned_candidates.into_iter().take(5).collect(),
        blocked_candidates: blocked_candidates.into_iter().take(5).collect(),
    }
}

pub fn aggregate_diagnosis(entries: &[DiagnosisBinaryEntry]) -> DiagnosisAggregate {
    let mut bucket_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut next_action_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut nir_block_signature_counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        *bucket_counts
            .entry(entry.diagnosis_bucket.clone())
            .or_default() += 1;
        *next_action_counts
            .entry(entry.next_action.clone())
            .or_default() += 1;
        for (signature, count) in &entry.derived_metrics.blocked_nir_block_signature_counts {
            *nir_block_signature_counts
                .entry(signature.clone())
                .or_default() += *count;
        }
    }
    let dominant_diagnosis = bucket_counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(key, _)| key.clone());
    let recommended_next_patch = next_action_counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(key, _)| key.clone());

    DiagnosisAggregate {
        diagnosis_bucket_counts: bucket_counts,
        next_action_counts,
        nir_block_signature_counts,
        dominant_diagnosis,
        recommended_next_patch,
    }
}

fn dedupe_candidate_vec<T, F>(items: &mut Vec<T>, key_fn: F)
where
    F: Fn(&T) -> (String, String),
{
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(key_fn(item)));
}
