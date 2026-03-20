mod corpus;
mod diagnosis;
mod inventory;
mod lanes;
mod model;
mod report;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use corpus::build_corpus_artifacts;
use diagnosis::{DiagnosisReport, aggregate_diagnosis, diagnosis_entry};
use inventory::{ensure_fission_cli, run_inventory_emit};
use lanes::{
    default_manifest_path, default_source_inventory_path, load_source_inventory,
    normalize_lane_name, resolve_lane_targets, resolve_source_meta,
};
use model::{InventoryRow, InventorySummary, SourceMeta};
use report::{
    AutomationSummary, build_quality_measurement, build_summary, compute_delta,
    enrich_summary_with_provenance, load_baseline, print_terminal_summary, render_markdown,
    update_latest,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(name = "fission-automation")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Canonical automation runner for Fission quality pipelines")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    NirCheck(NirCheckArgs),
}

#[derive(Parser, Debug)]
struct NirCheckArgs {
    #[arg(long, default_value = "pdb")]
    lane: String,
    #[arg(long)]
    release: bool,
    #[arg(long)]
    no_build: bool,
    #[arg(long)]
    fission_bin: Option<PathBuf>,
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    baseline: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    update_latest: bool,
    #[arg(long)]
    timeout_ms: Option<u64>,
    #[arg(long)]
    functions_limit: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::NirCheck(args) => run_nir_check(args),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn run_nir_check(args: NirCheckArgs) -> Result<()> {
    let root = repo_root();
    let (canonical_lane, deprecated_preview_lane) = normalize_lane_name(&args.lane);
    let manifest_path = args
        .manifest
        .unwrap_or_else(|| default_manifest_path(&root));
    let source_inventory_path = default_source_inventory_path(&root);
    let source_inventory = match source_inventory_path.as_ref() {
        Some(path) => load_source_inventory(path)
            .with_context(|| format!("load source inventory {}", path.display()))?,
        None => Default::default(),
    };
    let targets = resolve_lane_targets(
        &root,
        &manifest_path,
        canonical_lane,
        source_inventory_path.as_ref().map(|_| &source_inventory),
    )?;
    if targets.is_empty() {
        anyhow::bail!("lane `{}` resolved no targets", args.lane);
    }

    let fission_bin = ensure_fission_cli(
        &root,
        args.release,
        args.no_build,
        args.fission_bin.as_deref(),
    )?;
    let run_id = unix_run_id();
    let base_output_dir = args.output_dir.unwrap_or_else(|| {
        root.join("artifacts")
            .join("fission-automation")
            .join(&run_id)
    });
    let per_binary_dir = base_output_dir.join("per_binary");
    fs::create_dir_all(&per_binary_dir)
        .with_context(|| format!("create {}", per_binary_dir.display()))?;

    let mut datasets: Vec<(InventorySummary, Vec<InventoryRow>, Option<SourceMeta>)> = Vec::new();
    let mut inventory_summaries = Vec::new();
    let mut failed_targets = Vec::new();

    for target in &targets {
        let file_slug = sanitize_file_stem(&target.binary);
        let rows_path = per_binary_dir.join(format!("{file_slug}.inventory.rows.jsonl"));
        let summary_path = per_binary_dir.join(format!("{file_slug}.inventory.summary.json"));
        let functions_limit = args.functions_limit.or(target.default_functions_limit);
        let timeout_ms = args
            .timeout_ms
            .or(target.default_timeout_ms)
            .unwrap_or(10_000);
        let inventory_result = run_inventory_emit(
            &root,
            &fission_bin,
            &target.path,
            &rows_path,
            &summary_path,
            functions_limit,
            timeout_ms,
        );
        let (rows, summary) = match inventory_result {
            Ok(result) => result,
            Err(error) => {
                eprintln!(
                    "[fission-automation] inventory failed for {}: {error:#}",
                    target.path.display()
                );
                failed_targets.push(target.binary.clone());
                continue;
            }
        };
        let source_meta = resolve_source_meta(&source_inventory, &target.path).cloned();
        inventory_summaries.push(summary.clone());
        datasets.push((summary, rows, source_meta));
    }
    if datasets.is_empty() {
        anyhow::bail!(
            "lane `{}` produced no successful inventory runs (failed: {})",
            canonical_lane,
            failed_targets.join(", ")
        );
    }

    let corpus_artifacts = build_corpus_artifacts(&root, &datasets);
    let diagnosis_entries = datasets
        .iter()
        .zip(targets.iter())
        .map(|((summary, rows, source_meta), target)| {
            diagnosis_entry(&target.path, rows, summary, source_meta.as_ref())
        })
        .collect::<Vec<_>>();
    let diagnosis_report = DiagnosisReport {
        generated_at: isoish_now(),
        source_inventory_file: source_inventory_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        aggregate: aggregate_diagnosis(&diagnosis_entries),
        binaries: diagnosis_entries,
    };

    let mut automation_summary = build_summary(
        isoish_now(),
        canonical_lane,
        &run_id,
        &inventory_summaries,
        &corpus_artifacts.inventory_summary_totals,
        &diagnosis_report.aggregate,
    );
    enrich_summary_with_provenance(&mut automation_summary, &diagnosis_report);

    let latest_dir = root
        .join("artifacts")
        .join("fission-automation")
        .join("latest")
        .join(canonical_lane);
    let baseline_path = args
        .baseline
        .unwrap_or_else(|| latest_dir.join("summary.json"));
    let baseline = load_baseline(&baseline_path)?;
    let delta = compute_delta(&automation_summary, baseline.as_ref());

    write_outputs(
        &base_output_dir,
        &automation_summary,
        &diagnosis_report,
        &corpus_artifacts,
        delta.as_ref(),
    )?;
    if args.update_latest {
        update_latest(&base_output_dir, &latest_dir)?;
    }
    print_terminal_summary(&automation_summary, &diagnosis_report);
    if deprecated_preview_lane {
        eprintln!("[fission-automation] '--lane preview' is deprecated; use '--lane nir'");
    }
    if !failed_targets.is_empty() {
        eprintln!(
            "[fission-automation] skipped failed targets: {}",
            failed_targets.join(", ")
        );
    }
    println!(
        "[fission-automation] wrote outputs to {}",
        base_output_dir.display()
    );
    Ok(())
}

fn write_outputs(
    base_output_dir: &Path,
    summary: &AutomationSummary,
    diagnosis: &DiagnosisReport,
    corpus: &corpus::CorpusArtifacts,
    delta: Option<&report::SummaryDelta>,
) -> Result<()> {
    fs::create_dir_all(base_output_dir)
        .with_context(|| format!("create {}", base_output_dir.display()))?;

    write_json_pretty(base_output_dir.join("summary.json"), summary)?;
    write_json_pretty(
        base_output_dir.join("quality_measurement.json"),
        &build_quality_measurement(summary),
    )?;
    write_json_pretty(base_output_dir.join("diagnosis.json"), diagnosis)?;
    write_json_pretty(
        base_output_dir.join("corpus.json"),
        &serde_json::json!({
            "timeout_rescue": corpus.timeout_rescue,
            "quality_explicit_facts": corpus.quality_explicit_facts,
            "quality_heuristic_surface": corpus.quality_heuristic_surface,
        }),
    )?;
    write_json_pretty(
        base_output_dir.join("nir_quality_candidates.json"),
        &serde_json::json!({ "candidates": corpus.candidates }),
    )?;
    write_json_pretty(
        base_output_dir.join("nir_explicit_blocked_candidates.json"),
        &serde_json::json!({
            "blocked_candidates": corpus.blocked_explicit_candidates,
            "block_reason_counts": corpus.block_reason_counts,
            "inventory_summary_totals": corpus.inventory_summary_totals,
        }),
    )?;
    write_json_pretty(
        base_output_dir.join("nir_explicit_aligned_candidate_report.json"),
        &serde_json::json!({ "aligned_candidates": corpus.aligned_explicit_candidates }),
    )?;
    write_json_pretty(
        base_output_dir.join("preview_quality_candidates.json"),
        &serde_json::json!({ "candidates": corpus.candidates }),
    )?;
    write_json_pretty(
        base_output_dir.join("preview_explicit_blocked_candidates.json"),
        &serde_json::json!({
            "blocked_candidates": corpus.blocked_explicit_candidates,
            "block_reason_counts": corpus.block_reason_counts,
            "inventory_summary_totals": corpus.inventory_summary_totals,
        }),
    )?;
    write_json_pretty(
        base_output_dir.join("preview_explicit_aligned_candidate_report.json"),
        &serde_json::json!({ "aligned_candidates": corpus.aligned_explicit_candidates }),
    )?;

    let markdown = render_markdown(summary, diagnosis, corpus, delta);
    fs::write(base_output_dir.join("summary.md"), markdown)
        .with_context(|| format!("write {}", base_output_dir.join("summary.md").display()))?;
    fs::write(
        base_output_dir.join("diagnosis.md"),
        render_diagnosis_markdown(diagnosis),
    )
    .with_context(|| format!("write {}", base_output_dir.join("diagnosis.md").display()))?;
    Ok(())
}

fn render_diagnosis_markdown(diagnosis: &DiagnosisReport) -> String {
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

fn write_json_pretty(path: PathBuf, value: &impl serde::Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(&path, data).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn unix_run_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("{}-{:09}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|_| "run".to_string())
}

fn isoish_now() -> String {
    format!("{}", unix_run_id())
}

fn sanitize_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}
