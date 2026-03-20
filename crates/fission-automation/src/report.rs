use crate::corpus::{CorpusArtifacts, InventorySummaryTotals};
use crate::diagnosis::{DiagnosisAggregate, DiagnosisReport};
use crate::model::{InventorySummary, ProvenanceSurfaceTotals, SourcePresenceCounts};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinarySnapshot {
    pub binary: String,
    pub direct_success_count: usize,
    pub preview_failure_count: usize,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateSnapshot {
    pub direct_success_count: usize,
    pub preview_failure_count: usize,
    pub explicit_fact_nonzero_count: usize,
    pub strict_explicit_candidate_count: usize,
    pub inventory_surface_gap_count: usize,
    pub source_presence_counts: SourcePresenceCounts,
    pub provenance_surface_totals: ProvenanceSurfaceTotals,
    pub diagnosis_bucket_counts: BTreeMap<String, usize>,
    pub preview_block_signature_counts: BTreeMap<String, usize>,
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
    pub preview_failure_count: isize,
    pub explicit_fact_nonzero_count: isize,
    pub strict_explicit_candidate_count: isize,
    pub inventory_surface_gap_count: isize,
    pub pdb_nonzero_rows: isize,
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
        binaries.push(BinarySnapshot {
            binary: summary.binary.clone(),
            direct_success_count: summary.direct_success_count,
            preview_failure_count: summary.preview_failure_count,
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
        });
    }
    AutomationSummary {
        generated_at,
        lane: lane.to_string(),
        run_id: run_id.to_string(),
        binaries,
        aggregate: AggregateSnapshot {
            direct_success_count: totals.direct_success_count,
            preview_failure_count: totals.preview_failure_count,
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
            preview_block_signature_counts: diagnosis.preview_block_signature_counts.clone(),
            recommended_next_patch: diagnosis.recommended_next_patch.clone(),
            recovery_strategy_attempted_counts: aggregate_recovery_strategy_attempted_counts,
            recovery_strategy_applied_counts: aggregate_recovery_strategy_applied_counts,
            recovery_outcome_counts: aggregate_recovery_outcome_counts,
            recovery_quality_flag_counts: aggregate_recovery_quality_flag_counts,
            recovery_structuring_mode_counts: aggregate_recovery_structuring_mode_counts,
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
        preview_failure_count: current.aggregate.preview_failure_count as isize
            - baseline.aggregate.preview_failure_count as isize,
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
    })
}

pub fn render_markdown(
    summary: &AutomationSummary,
    diagnosis: &DiagnosisReport,
    corpus: &CorpusArtifacts,
    delta: Option<&SummaryDelta>,
) -> String {
    let mut out = String::new();
    out.push_str("# Fission Automation Summary\n\n");
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
        "- direct_success_count: `{}`\n- preview_failure_count: `{}`\n- explicit_fact_nonzero_count: `{}`\n- strict_explicit_candidate_count: `{}`\n- inventory_surface_gap_count: `{}`\n",
        summary.aggregate.direct_success_count,
        summary.aggregate.preview_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count,
        summary.aggregate.inventory_surface_gap_count,
    ));
    out.push_str(&format!(
        "- source_presence_counts: `{:?}`\n- provenance_surface_totals: `{:?}`\n",
        summary.aggregate.source_presence_counts, summary.aggregate.provenance_surface_totals
    ));
    out.push_str(&format!(
        "- diagnosis_bucket_counts: `{:?}`\n- preview_block_signature_counts: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n\n",
        summary.aggregate.diagnosis_bucket_counts,
        summary.aggregate.preview_block_signature_counts,
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts,
        summary.aggregate.recovery_structuring_mode_counts,
    ));

    if let Some(delta) = delta {
        out.push_str("## Baseline Delta\n\n");
        out.push_str(&format!(
            "- direct_success_count: `{:+}`\n- preview_failure_count: `{:+}`\n- explicit_fact_nonzero_count: `{:+}`\n- strict_explicit_candidate_count: `{:+}`\n- inventory_surface_gap_count: `{:+}`\n- pdb_nonzero_rows: `{:+}`\n\n",
            delta.direct_success_count,
            delta.preview_failure_count,
            delta.explicit_fact_nonzero_count,
            delta.strict_explicit_candidate_count,
            delta.inventory_surface_gap_count,
            delta.pdb_nonzero_rows
        ));
    }

    out.push_str("## Per-Binary Highlights\n\n");
    for entry in &diagnosis.binaries {
        out.push_str(&format!("### {}\n\n", entry.binary));
        out.push_str(&format!(
            "- diagnosis: `{}`\n- next_action: `{}`\n- explicit_nonzero_rows: `{}`\n- strict_explicit_candidate_count: `{}`\n- preview_block_signatures: `{:?}`\n- recovery_attempted_counts: `{:?}`\n- recovery_outcome_counts: `{:?}`\n- recovery_structuring_mode_counts: `{:?}`\n- recovery_quality_flag_counts: `{:?}`\n\n",
            entry.diagnosis_bucket,
            entry.next_action,
            entry.derived_metrics.explicit_nonzero_rows,
            entry.inventory_summary.strict_explicit_candidate_count,
            entry.derived_metrics.blocked_preview_block_signature_counts,
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

pub fn print_terminal_summary(summary: &AutomationSummary, diagnosis: &DiagnosisReport) {
    println!("[fission-automation] lane={}", summary.lane);
    println!(
        "  direct_success={} preview_failure={} explicit_nonzero={} strict_explicit={}",
        summary.aggregate.direct_success_count,
        summary.aggregate.preview_failure_count,
        summary.aggregate.explicit_fact_nonzero_count,
        summary.aggregate.strict_explicit_candidate_count
    );
    println!(
        "  inventory_surface_gap={} pdb_nonzero_rows={}",
        summary.aggregate.inventory_surface_gap_count,
        summary.aggregate.provenance_surface_totals.pdb_nonzero_rows
    );
    println!(
        "  dominant_diagnosis={:?} next_patch={:?}",
        diagnosis.aggregate.dominant_diagnosis, diagnosis.aggregate.recommended_next_patch
    );
    println!(
        "  preview_block_signatures={:?}",
        diagnosis.aggregate.preview_block_signature_counts
    );
    println!(
        "  recovery_attempted={:?} recovery_outcome={:?} recovery_quality_flags={:?}",
        summary.aggregate.recovery_strategy_attempted_counts,
        summary.aggregate.recovery_outcome_counts,
        summary.aggregate.recovery_quality_flag_counts
    );
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
