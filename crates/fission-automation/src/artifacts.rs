//! Run directory artifact emission (`summary.json`, candidate JSON, Markdown).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::corpus::CorpusArtifacts;
use crate::diagnosis::DiagnosisReport;
use crate::report::{
    AutomationDecisionInsights, AutomationSummary, SummaryDelta, build_quality_measurement,
    render_markdown,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ArtifactOptions {
    pub emit_legacy_preview_aliases: bool,
}

pub struct ArtifactWriter {
    base_output_dir: PathBuf,
    options: ArtifactOptions,
}

impl ArtifactWriter {
    pub fn new(base_output_dir: PathBuf, options: ArtifactOptions) -> Self {
        Self {
            base_output_dir,
            options,
        }
    }

    /// Writes all lane artifacts with finalized timings on `summary` (single pass).
    pub fn write_run_artifacts(
        &self,
        summary: &mut AutomationSummary,
        diagnosis: &DiagnosisReport,
        corpus: &CorpusArtifacts,
        delta: Option<&SummaryDelta>,
        insights: &AutomationDecisionInsights,
        inventory_elapsed_ms: u64,
        diagnosis_elapsed_ms: u64,
        write_started: Instant,
        run_started: Instant,
    ) -> Result<()> {
        summary.inventory_elapsed_ms = inventory_elapsed_ms;
        summary.diagnosis_elapsed_ms = diagnosis_elapsed_ms;

        std::fs::create_dir_all(&self.base_output_dir)
            .with_context(|| format!("create {}", self.base_output_dir.display()))?;

        write_json_pretty(self.base_output_dir.join("diagnosis.json"), diagnosis)?;
        write_json_pretty(
            self.base_output_dir.join("decision_insights.json"),
            insights,
        )?;
        write_json_pretty(
            self.base_output_dir.join("corpus.json"),
            &serde_json::json!({
                "timeout_rescue": corpus.timeout_rescue,
                "quality_explicit_facts": corpus.quality_explicit_facts,
            }),
        )?;
        write_json_pretty(
            self.base_output_dir.join("nir_quality_candidates.json"),
            &serde_json::json!({ "candidates": corpus.candidates }),
        )?;
        write_json_pretty(
            self.base_output_dir
                .join("nir_explicit_blocked_candidates.json"),
            &serde_json::json!({
                "blocked_candidates": corpus.blocked_explicit_candidates,
                "block_reason_counts": corpus.block_reason_counts,
                "inventory_summary_totals": corpus.inventory_summary_totals,
            }),
        )?;
        write_json_pretty(
            self.base_output_dir
                .join("nir_explicit_aligned_candidate_report.json"),
            &serde_json::json!({ "aligned_candidates": corpus.aligned_explicit_candidates }),
        )?;

        if self.options.emit_legacy_preview_aliases {
            write_json_pretty(
                self.base_output_dir.join("preview_quality_candidates.json"),
                &serde_json::json!({ "candidates": corpus.candidates }),
            )?;
            write_json_pretty(
                self.base_output_dir
                    .join("preview_explicit_blocked_candidates.json"),
                &serde_json::json!({
                    "blocked_candidates": corpus.blocked_explicit_candidates,
                    "block_reason_counts": corpus.block_reason_counts,
                    "inventory_summary_totals": corpus.inventory_summary_totals,
                }),
            )?;
            write_json_pretty(
                self.base_output_dir
                    .join("preview_explicit_aligned_candidate_report.json"),
                &serde_json::json!({ "aligned_candidates": corpus.aligned_explicit_candidates }),
            )?;
        }

        std::fs::write(
            self.base_output_dir.join("diagnosis.md"),
            render_diagnosis_markdown(diagnosis),
        )
        .with_context(|| {
            format!(
                "write {}",
                self.base_output_dir.join("diagnosis.md").display()
            )
        })?;

        summary.write_outputs_elapsed_ms = write_started.elapsed().as_millis() as u64;
        summary.total_elapsed_ms = run_started.elapsed().as_millis() as u64;

        write_json_pretty(self.base_output_dir.join("summary.json"), summary)?;
        write_json_pretty(
            self.base_output_dir.join("quality_measurement.json"),
            &build_quality_measurement(summary),
        )?;

        let markdown = render_markdown(summary, diagnosis, corpus, delta, Some(insights));
        std::fs::write(self.base_output_dir.join("summary.md"), markdown).with_context(|| {
            format!(
                "write {}",
                self.base_output_dir.join("summary.md").display()
            )
        })?;

        Ok(())
    }
}

pub fn automation_artifact_root(root: &Path) -> PathBuf {
    root.join("benchmark").join("artifacts").join("automation")
}

pub fn render_diagnosis_markdown(diagnosis: &DiagnosisReport) -> String {
    let mut lines = Vec::new();
    lines.push("# Inventory Diagnosis".to_string());
    lines.push(String::new());
    lines.push("## Aggregate".to_string());
    lines.push(String::new());
    lines.push(format!(
        "- Dominant diagnosis: `{:?}`",
        diagnosis.aggregate.dominant_diagnosis
    ));
    lines.push(format!(
        "- Recommended next patch: `{:?}`",
        diagnosis.aggregate.recommended_next_patch
    ));
    lines.push(format!(
        "- Diagnosis bucket counts: `{:?}`",
        diagnosis.aggregate.diagnosis_bucket_counts
    ));
    lines.push(format!(
        "- Fission NIR block signatures: `{:?}`",
        diagnosis.aggregate.nir_block_signature_counts
    ));
    lines.push(String::new());
    lines.push("## Binaries".to_string());
    lines.push(String::new());
    for entry in &diagnosis.binaries {
        lines.push(format!("### {}", entry.binary));
        lines.push(String::new());
        lines.push(format!("- Diagnosis: `{}`", entry.diagnosis_bucket));
        lines.push(format!("- Next action: `{}`", entry.next_action));
        lines.push(format!("- Rationale: {}", entry.diagnosis_rationale));
        lines.push(format!(
            "- Source-present rows: `{}`, explicit-nonzero rows: `{}`, surface-gap rows: `{}`",
            entry.derived_metrics.source_present_rows,
            entry.derived_metrics.explicit_nonzero_rows,
            entry.derived_metrics.inventory_surface_gap_count
        ));
        lines.push(format!(
            "- Source presence counts: `{:?}`, provenance surface totals: `{:?}`",
            entry.derived_metrics.source_presence_counts,
            entry.derived_metrics.provenance_surface_totals
        ));
        lines.push(format!(
            "- Blocked admission stages: `{:?}`",
            entry.derived_metrics.blocked_admission_stage_counts
        ));
        lines.push(format!(
            "- Fission NIR block signatures: `{:?}`",
            entry.derived_metrics.blocked_nir_block_signature_counts
        ));
        lines.push(String::new());
    }
    lines.join("\n")
}

pub fn write_json_pretty(path: PathBuf, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    std::fs::write(&path, data).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
