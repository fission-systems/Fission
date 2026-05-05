//! NIR quality lane: manifest resolution, inventory emit, reports, gates.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::artifacts::{ArtifactOptions, ArtifactWriter, automation_artifact_root};
use crate::cli::NirCheckArgs;
use crate::cli::RunProfile;
use crate::corpus::build_corpus_artifacts;
use crate::diagnosis::{DiagnosisReport, aggregate_diagnosis, diagnosis_entry};
use crate::gates::{enforce_fail_on_stop, enforce_perf_regression};
use crate::inventory::{ensure_fission_cli, run_inventory_emit};
use crate::lanes::{
    default_manifest_path, default_source_inventory_path, load_source_inventory,
    normalize_lane_name, resolve_lane_targets, resolve_source_meta, validate_lane_target_paths,
};
use crate::model::{InventoryRow, InventorySummary, LaneTarget, SourceMeta};
use crate::report::{
    build_decision_insights, build_summary, compute_delta, enrich_summary_with_provenance,
    load_baseline, load_baseline_candidates, print_terminal_summary, update_latest,
};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

pub fn automation_run_dir(
    root: &Path,
    lane: &str,
    run_profile: RunProfile,
    run_id: &str,
) -> PathBuf {
    automation_artifact_root(root).join(format!("{}-{}-{}", lane, run_profile.as_str(), run_id))
}

pub fn init_automation_logging(root: &Path) {
    let log_path = automation_artifact_root(root)
        .join("logs")
        .join("fission-automation.log");
    let options = fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging)
        .with_file_path(log_path);
    fission_core::logging::init_with_options(options);
    fission_core::logging::info("initialized fission-automation logging");
}

pub fn run(args: NirCheckArgs) -> Result<()> {
    let run_started = Instant::now();
    let root = repo_root();
    let (canonical_lane, deprecated_preview_lane) = normalize_lane_name(&args.lane);
    let manifest_path = args
        .manifest
        .clone()
        .unwrap_or_else(default_manifest_path);
    let source_inventory_path = default_source_inventory_path();
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
    validate_lane_target_paths(&targets)?;

    let run_id = unix_run_id();
    let base_output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| automation_run_dir(&root, canonical_lane, args.run_profile, &run_id));
    let per_binary_dir = base_output_dir.join("per_binary");

    if args.dry_run {
        let fission_display = match args.fission_bin.as_deref() {
            Some(p) => p.display().to_string(),
            None => root
                .join("target")
                .join(if args.release { "release" } else { "debug" })
                .join("fission_cli")
                .display()
                .to_string(),
        };
        println!("[fission-automation] dry-run");
        println!("  lane: {} (canonical: {canonical_lane})", args.lane);
        println!("  manifest: {}", manifest_path.display());
        println!("  fission_cli (expected): {fission_display}");
        println!("  output_dir: {}", base_output_dir.display());
        println!("  targets ({}):", targets.len());
        for t in &targets {
            println!("    - {} ({})", t.binary, t.path.display());
        }
        println!(
            "  per_binary_dir (would create): {}",
            per_binary_dir.display()
        );
        return Ok(());
    }

    let fission_bin = ensure_fission_cli(
        &root,
        args.release,
        args.no_build,
        args.fission_bin.as_deref(),
    )?;
    fs::create_dir_all(&per_binary_dir)
        .with_context(|| format!("create {}", per_binary_dir.display()))?;

    let latest_dir = automation_artifact_root(&root)
        .join("latest")
        .join(canonical_lane);
    let baseline_path = args
        .baseline
        .clone()
        .unwrap_or_else(|| latest_dir.join("summary.json"));
    let baseline = load_baseline(&baseline_path)?;
    let baseline_candidates = load_baseline_candidates(&baseline_path)?;

    if let Some(top_n) = args.focus_top_mismatch
        && let Some(candidates) = baseline_candidates.as_deref()
    {
        let focus_binaries = pick_focus_binaries_from_baseline(candidates, top_n);
        if !focus_binaries.is_empty() {
            targets
                .retain(|target| focus_binaries.contains(&normalize_binary_label(&target.binary)));
        }
    }
    if targets.is_empty() {
        anyhow::bail!("run_profile/filter resolved no targets");
    }

    let inventory_started = Instant::now();
    type DatasetEntry = (
        LaneTarget,
        InventorySummary,
        Vec<InventoryRow>,
        Option<SourceMeta>,
    );
    let mut datasets: Vec<DatasetEntry> = Vec::new();
    let mut inventory_summaries = Vec::new();
    let mut failed_targets = Vec::new();

    let run_target = |target: &LaneTarget| -> Result<DatasetEntry> {
        let file_slug = sanitize_file_stem(&target.binary);
        let rows_path = per_binary_dir.join(format!("{file_slug}.inventory.rows.jsonl"));
        let summary_path = per_binary_dir.join(format!("{file_slug}.inventory.summary.json"));
        let base_functions_limit = args.functions_limit.or(target.default_functions_limit);
        let functions_limit = args
            .run_profile
            .adjust_functions_limit(base_functions_limit);
        let timeout_ms = args
            .timeout_ms
            .or(target.default_timeout_ms)
            .unwrap_or(10_000);
        let timeout_ms = args.run_profile.adjust_timeout_ms(timeout_ms);
        let (rows, summary) = run_inventory_emit(
            &root,
            &fission_bin,
            &target.path,
            &rows_path,
            &summary_path,
            functions_limit,
            timeout_ms,
        )?;
        let source_meta = resolve_source_meta(&source_inventory, &target.path).cloned();
        Ok((target.clone(), summary, rows, source_meta))
    };

    let jobs = args.jobs.max(1);
    if jobs == 1 {
        for target in &targets {
            match run_target(target) {
                Ok(entry) => {
                    let (_, ref summary, _, _) = entry;
                    inventory_summaries.push(summary.clone());
                    datasets.push(entry);
                }
                Err(error) => {
                    eprintln!(
                        "[fission-automation] inventory failed for {}: {error:#}",
                        target.path.display()
                    );
                    failed_targets.push(target.binary.clone());
                }
            }
        }
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .context("build inventory thread pool")?;
        let results: Vec<Result<DatasetEntry, anyhow::Error>> = pool.install(|| {
            targets
                .par_iter()
                .map(|target| run_target(target))
                .collect()
        });
        for (target, result) in targets.iter().zip(results) {
            match result {
                Ok(entry) => {
                    let (_, ref summary, _, _) = entry;
                    inventory_summaries.push(summary.clone());
                    datasets.push(entry);
                }
                Err(error) => {
                    eprintln!(
                        "[fission-automation] inventory failed for {}: {error:#}",
                        target.path.display()
                    );
                    failed_targets.push(target.binary.clone());
                }
            }
        }
    }
    if datasets.is_empty() {
        anyhow::bail!(
            "lane `{}` produced no successful inventory runs (failed: {})",
            canonical_lane,
            failed_targets.join(", ")
        );
    }
    let inventory_elapsed_ms = inventory_started.elapsed().as_millis() as u64;
    metrics::histogram!("fission.automation.nir_check.inventory_ms")
        .record(inventory_elapsed_ms as f64);

    let corpus_artifacts = build_corpus_artifacts(&root, &datasets);
    let diagnosis_started = Instant::now();
    let diagnosis_entries = datasets
        .iter()
        .map(|(target, summary, rows, source_meta)| {
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
    metrics::histogram!("fission.automation.nir_check.diagnosis_ms")
        .record(diagnosis_elapsed_ms as f64);

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
    let writer = ArtifactWriter::new(
        base_output_dir.clone(),
        ArtifactOptions {
            emit_legacy_preview_aliases: args.emit_legacy_preview_artifacts,
        },
    );
    writer.write_run_artifacts(
        &mut automation_summary,
        &diagnosis_report,
        &corpus_artifacts,
        delta.as_ref(),
        &decision_insights,
        inventory_elapsed_ms,
        diagnosis_elapsed_ms,
        write_started,
        run_started,
    )?;

    metrics::histogram!("fission.automation.nir_check.write_outputs_ms")
        .record(automation_summary.write_outputs_elapsed_ms as f64);
    metrics::histogram!("fission.automation.nir_check.total_ms")
        .record(automation_summary.total_elapsed_ms as f64);
    metrics::counter!("fission.automation.nir_check.targets_total")
        .increment(automation_summary.target_count as u64);
    metrics::counter!("fission.automation.nir_check.changed_rows_total")
        .increment(decision_insights.changed_row_count as u64);

    if args.update_latest && !args.no_update_latest {
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

    enforce_fail_on_stop(&decision_insights, args.fail_on_stop)?;

    if let Some(base) = baseline.as_ref() {
        enforce_perf_regression(&automation_summary, base)?;
    }

    Ok(())
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

fn unix_run_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("{}-{:09}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|_| "run".to_string())
}

fn isoish_now() -> String {
    unix_run_id()
}

pub(crate) fn sanitize_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize_file_stem;

    #[test]
    fn sanitize_file_stem_replaces_unsafe_chars() {
        assert_eq!(sanitize_file_stem("a b"), "a_b");
        assert_eq!(sanitize_file_stem("foo.exe"), "foo.exe");
    }
}
