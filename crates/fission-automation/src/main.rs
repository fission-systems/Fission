mod corpus;
mod diagnosis;
mod inventory;
mod lanes;
mod model;
mod report;

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    feature = "allocator-jemallocator",
    not(feature = "allocator-mimalloc")
))]
#[global_allocator]
static GLOBAL_ALLOCATOR: jemallocator::Jemalloc = jemallocator::Jemalloc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use corpus::build_corpus_artifacts;
use diagnosis::{aggregate_diagnosis, diagnosis_entry, DiagnosisReport};
use inventory::{ensure_fission_cli, run_inventory_emit};
use lanes::{
    default_manifest_path, default_source_inventory_path, load_source_inventory,
    normalize_lane_name, resolve_lane_targets, resolve_source_meta,
};
use model::{InventoryRow, InventorySummary, SourceMeta};
use report::{
    build_decision_insights, build_quality_measurement, build_summary, compute_delta,
    enrich_summary_with_provenance, load_baseline, load_baseline_candidates,
    print_terminal_summary, render_markdown, update_latest, AutomationDecisionInsights,
    AutomationSummary,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
    #[arg(long, value_enum, default_value_t = RunProfile::Mid)]
    run_profile: RunProfile,
    #[arg(long)]
    focus_top_mismatch: Option<usize>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RunProfile {
    Fast,
    Mid,
    Full,
}

impl RunProfile {
    fn as_str(self) -> &'static str {
        match self {
            RunProfile::Fast => "fast",
            RunProfile::Mid => "mid",
            RunProfile::Full => "full",
        }
    }

    fn adjust_functions_limit(self, base: Option<usize>) -> Option<usize> {
        match (self, base) {
            (RunProfile::Fast, Some(v)) => Some(v.min(10).max(1)),
            (RunProfile::Fast, None) => Some(10),
            (RunProfile::Mid, v) => v,
            (RunProfile::Full, Some(v)) => Some(v.max(40)),
            (RunProfile::Full, None) => Some(40),
        }
    }

    fn adjust_timeout_ms(self, base: u64) -> u64 {
        match self {
            RunProfile::Fast => base.min(1_500).max(500),
            RunProfile::Mid => base,
            RunProfile::Full => base.max(10_000),
        }
    }
}

fn normalize_binary_label(value: &str) -> String {
    value.trim().trim_end_matches(".exe").to_ascii_lowercase()
}

fn pick_focus_binaries_from_baseline(rows: &[InventoryRow], top_n: usize) -> BTreeSet<String> {
    let mut ranked = rows
        .iter()
        .filter_map(|row| {
            let stats = row.nir_build_stats.as_ref()?;
            let mismatch =
                stats.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count;
            if mismatch == 0 {
                return None;
            }
            Some((
                mismatch,
                stats.region_linearize_rejected_body_lowering_failed_count,
                row.binary.clone(),
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    ranked
        .into_iter()
        .take(top_n.max(1))
        .map(|(_, _, binary)| normalize_binary_label(&binary))
        .collect()
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
    let run_started = Instant::now();
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
    let mut targets = resolve_lane_targets(
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

    let latest_dir = root
        .join("artifacts")
        .join("fission-automation")
        .join("latest")
        .join(canonical_lane);
    let baseline_path = args
        .baseline
        .unwrap_or_else(|| latest_dir.join("summary.json"));
    let baseline = load_baseline(&baseline_path)?;
    let baseline_candidates = load_baseline_candidates(&baseline_path)?;

    if let Some(top_n) = args.focus_top_mismatch
        && let Some(candidates) = baseline_candidates.as_deref()
    {
        let focus_binaries = pick_focus_binaries_from_baseline(candidates, top_n);
        if !focus_binaries.is_empty() {
            targets.retain(|target| focus_binaries.contains(&normalize_binary_label(&target.binary)));
        }
    }
    if targets.is_empty() {
        anyhow::bail!("run_profile/filter resolved no targets");
    }

    let inventory_started = Instant::now();
    let mut datasets: Vec<(InventorySummary, Vec<InventoryRow>, Option<SourceMeta>)> = Vec::new();
    let mut inventory_summaries = Vec::new();
    let mut failed_targets = Vec::new();

    for target in &targets {
        let file_slug = sanitize_file_stem(&target.binary);
        let rows_path = per_binary_dir.join(format!("{file_slug}.inventory.rows.jsonl"));
        let summary_path = per_binary_dir.join(format!("{file_slug}.inventory.summary.json"));
        let base_functions_limit = args.functions_limit.or(target.default_functions_limit);
        let functions_limit = args.run_profile.adjust_functions_limit(base_functions_limit);
        let timeout_ms = args
            .timeout_ms
            .or(target.default_timeout_ms)
            .unwrap_or(10_000);
        let timeout_ms = args.run_profile.adjust_timeout_ms(timeout_ms);
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
    let inventory_elapsed_ms = inventory_started.elapsed().as_millis() as u64;

    let corpus_artifacts = build_corpus_artifacts(&root, &datasets);
    let diagnosis_started = Instant::now();
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
    let diagnosis_elapsed_ms = diagnosis_started.elapsed().as_millis() as u64;

    let mut automation_summary = build_summary(
        isoish_now(),
        canonical_lane,
        &run_id,
        args.run_profile.as_str(),
        targets.len(),
        &inventory_summaries,
        &corpus_artifacts.inventory_summary_totals,
        &diagnosis_report.aggregate,
    );
    enrich_summary_with_provenance(&mut automation_summary, &diagnosis_report);
    let delta = compute_delta(&automation_summary, baseline.as_ref());
    let decision_insights = build_decision_insights(
        &automation_summary,
        &corpus_artifacts.candidates,
        baseline.as_ref(),
        baseline_candidates.as_deref(),
    );

    let write_started = Instant::now();
    write_outputs(
        &base_output_dir,
        &automation_summary,
        &diagnosis_report,
        &corpus_artifacts,
        delta.as_ref(),
        &decision_insights,
    )?;
    automation_summary.inventory_elapsed_ms = inventory_elapsed_ms;
    automation_summary.diagnosis_elapsed_ms = diagnosis_elapsed_ms;
    automation_summary.write_outputs_elapsed_ms = write_started.elapsed().as_millis() as u64;
    automation_summary.total_elapsed_ms = run_started.elapsed().as_millis() as u64;
    write_json_pretty(base_output_dir.join("summary.json"), &automation_summary)?;
    let markdown = render_markdown(
        &automation_summary,
        &diagnosis_report,
        &corpus_artifacts,
        delta.as_ref(),
        Some(&decision_insights),
    );
    fs::write(base_output_dir.join("summary.md"), markdown)
        .with_context(|| format!("write {}", base_output_dir.join("summary.md").display()))?;
    if args.update_latest {
        update_latest(&base_output_dir, &latest_dir)?;
    }
    print_terminal_summary(&automation_summary, &diagnosis_report);
    println!(
        "  profile={} targets={} timings(ms): inventory={} diagnosis={} write={} total={} go_stop_gate={} changed_rows={}",
        automation_summary.run_profile,
        automation_summary.target_count,
        automation_summary.inventory_elapsed_ms,
        automation_summary.diagnosis_elapsed_ms,
        automation_summary.write_outputs_elapsed_ms,
        automation_summary.total_elapsed_ms,
        decision_insights.go_stop_gate.decision,
        decision_insights.changed_row_count
    );
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

    // Performance regression gate
    if let Some(baseline) = baseline.as_ref() {
        for (pass_name, current_agg) in &automation_summary.aggregate.nir_build_stats_totals.pass_metrics {
            if let Some(base_agg) = baseline.aggregate.nir_build_stats_totals.pass_metrics.get(pass_name) {
                // Ignore passes that take negligible time to avoid noise (e.g., < 10ms)
                if base_agg.total_time_ms > 10.0 {
                    let ratio = current_agg.total_time_ms / base_agg.total_time_ms;
                    if ratio > 1.25 {
                        anyhow::bail!(
                            "performance regression detected in pass '{}': {:.1}ms -> {:.1}ms ({:.1}x increase)",
                            pass_name,
                            base_agg.total_time_ms,
                            current_agg.total_time_ms,
                            ratio
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn write_outputs(
    base_output_dir: &Path,
    summary: &AutomationSummary,
    diagnosis: &DiagnosisReport,
    corpus: &corpus::CorpusArtifacts,
    delta: Option<&report::SummaryDelta>,
    insights: &AutomationDecisionInsights,
) -> Result<()> {
    fs::create_dir_all(base_output_dir)
        .with_context(|| format!("create {}", base_output_dir.display()))?;

    write_json_pretty(base_output_dir.join("summary.json"), summary)?;
    write_json_pretty(
        base_output_dir.join("quality_measurement.json"),
        &build_quality_measurement(summary),
    )?;
    write_json_pretty(base_output_dir.join("diagnosis.json"), diagnosis)?;
    write_json_pretty(base_output_dir.join("decision_insights.json"), insights)?;
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

    let markdown = render_markdown(summary, diagnosis, corpus, delta, Some(insights));
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
