//! Source semantic quality lane wrapper.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::artifacts::{automation_artifact_root, write_json_pretty};
use crate::cli::{RunProfile, SourceSemanticCheckArgs};
use crate::inventory::ensure_fission_cli;
use crate::report::update_latest;

const LANE_NAME: &str = "source-semantic";

pub fn run(args: SourceSemanticCheckArgs) -> Result<()> {
    let run_started = Instant::now();
    let root = repo_root();
    let run_id = unix_run_id();
    let manifest = args
        .manifest
        .clone()
        .unwrap_or_else(|| default_source_semantic_manifest(&root));
    let base_output_dir = args.output_dir.clone().unwrap_or_else(|| {
        automation_artifact_root(&root).join(format!(
            "{LANE_NAME}-{}-{run_id}",
            args.run_profile.as_str()
        ))
    });
    let source_semantic_dir = base_output_dir.join("source_semantic");
    fs::create_dir_all(&source_semantic_dir)
        .with_context(|| format!("create {}", source_semantic_dir.display()))?;

    let fission_bin = ensure_fission_cli(
        &root,
        args.release,
        args.no_build,
        args.fission_bin.as_deref(),
    )?;
    let jobs = args.jobs.unwrap_or_else(|| default_jobs(args.run_profile));
    let timeout_sec = args
        .timeout_sec
        .unwrap_or_else(|| default_timeout_sec(args.run_profile));
    let command_parts = build_source_semantic_command(
        &root,
        &manifest,
        &fission_bin,
        &source_semantic_dir,
        timeout_sec,
        jobs,
        &args,
    );
    let runner_started = Instant::now();
    let output = run_command(&root, &command_parts)?;
    let runner_elapsed_ms = runner_started.elapsed().as_millis() as u64;
    let runner_status = RunnerStatus {
        command: command_parts.clone(),
        exit_code: output.status.code(),
        success: output.status.success(),
        elapsed_ms: runner_elapsed_ms,
    };
    fs::write(base_output_dir.join("runner_stdout.txt"), &output.stdout)
        .with_context(|| "write source semantic runner stdout")?;
    fs::write(base_output_dir.join("runner_stderr.txt"), &output.stderr)
        .with_context(|| "write source semantic runner stderr")?;
    write_json_pretty(base_output_dir.join("runner_status.json"), &runner_status)?;
    if !runner_status.success {
        bail!(
            "source semantic runner failed with exit code {:?}; see {}",
            runner_status.exit_code,
            base_output_dir.join("runner_stderr.txt").display()
        );
    }

    let summary_path = source_semantic_dir.join("source_semantic_summary.json");
    let rows_path = source_semantic_dir.join("source_semantic_rows.json");
    let comparison_path = source_semantic_dir.join("source_semantic_comparison.json");
    let ghidra_summary_path = source_semantic_dir.join("ghidra_source_semantic_summary.json");
    let ghidra_rows_path = source_semantic_dir.join("ghidra_source_semantic_rows.json");
    let ghidra_comparison_path = source_semantic_dir.join("ghidra_source_semantic_comparison.json");

    let source_summary = load_json(&summary_path)?;
    let rows = load_json(&rows_path)?;
    let comparison = optional_json(&comparison_path)?;
    let ghidra_summary = optional_json(&ghidra_summary_path)?;
    let ghidra_rows = optional_json(&ghidra_rows_path)?;
    let ghidra_comparison = optional_json(&ghidra_comparison_path)?;
    let metrics = SourceSemanticMetrics::from_summary(&source_summary, comparison.as_ref());
    let decision = decide_gate(&metrics, comparison.as_ref());
    let top_timeout_rows = top_rows_by_behavior_status(&rows, "candidate_run_timeout", 8);
    let top_behavior_failures = top_behavior_failure_rows(&rows, 8);
    let top_regressions = comparison
        .as_ref()
        .and_then(|value| value.get("top_regressions"))
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let top_missing_features = top_missing_features(&rows, 12);

    let total_elapsed_ms = run_started.elapsed().as_millis() as u64;
    let summary = SourceSemanticAutomationSummary {
        lane: LANE_NAME.to_string(),
        run_profile: args.run_profile.as_str().to_string(),
        run_id,
        generated_at: isoish_now(),
        manifest: rel(&root, &manifest),
        fission_bin: rel(&root, &fission_bin),
        source_semantic_dir: rel(&root, &source_semantic_dir),
        runner_status,
        metrics: metrics.clone(),
        go_stop_gate: decision.clone(),
        artifact_paths: SourceSemanticArtifactPaths {
            source_semantic_summary: rel(&root, &summary_path),
            source_semantic_rows: rel(&root, &rows_path),
            source_semantic_comparison: comparison.as_ref().map(|_| rel(&root, &comparison_path)),
            ghidra_source_semantic_summary: ghidra_summary
                .as_ref()
                .map(|_| rel(&root, &ghidra_summary_path)),
            ghidra_source_semantic_rows: ghidra_rows
                .as_ref()
                .map(|_| rel(&root, &ghidra_rows_path)),
            ghidra_source_semantic_comparison: ghidra_comparison
                .as_ref()
                .map(|_| rel(&root, &ghidra_comparison_path)),
        },
        elapsed_ms: SourceSemanticElapsedMs {
            runner: runner_elapsed_ms,
            total: total_elapsed_ms,
        },
    };
    let insights = SourceSemanticDecisionInsights {
        go_stop_gate: decision.clone(),
        metrics,
        top_timeout_rows,
        top_behavior_failures,
        top_regressions,
        top_missing_features,
        comparison_outcome: source_summary.get("comparison_outcome").cloned(),
        ghidra_reference: source_summary.get("ghidra_reference").cloned(),
    };

    write_json_pretty(base_output_dir.join("summary.json"), &summary)?;
    write_json_pretty(base_output_dir.join("decision_insights.json"), &insights)?;
    write_json_pretty(base_output_dir.join("source_semantic_rows.json"), &rows)?;
    if let Some(comparison) = comparison.as_ref() {
        write_json_pretty(
            base_output_dir.join("source_semantic_comparison.json"),
            comparison,
        )?;
    }
    fs::write(
        base_output_dir.join("summary.md"),
        render_source_semantic_markdown(&summary, &insights),
    )
    .with_context(|| format!("write {}", base_output_dir.join("summary.md").display()))?;
    update_latest(
        &base_output_dir,
        &automation_artifact_root(&root)
            .join("latest")
            .join(LANE_NAME),
    )?;

    print_terminal_summary(&summary, &insights);
    if args.fail_on_stop && !decision.decision.starts_with("go_") {
        bail!(
            "source semantic gate is `{}` (rationale: {}); --fail-on-stop requested",
            decision.decision,
            decision.rationale
        );
    }
    Ok(())
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunnerStatus {
    command: Vec<String>,
    exit_code: Option<i32>,
    success: bool,
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticAutomationSummary {
    lane: String,
    run_profile: String,
    run_id: String,
    generated_at: String,
    manifest: String,
    fission_bin: String,
    source_semantic_dir: String,
    runner_status: RunnerStatus,
    metrics: SourceSemanticMetrics,
    go_stop_gate: SourceSemanticGate,
    artifact_paths: SourceSemanticArtifactPaths,
    elapsed_ms: SourceSemanticElapsedMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticArtifactPaths {
    source_semantic_summary: String,
    source_semantic_rows: String,
    source_semantic_comparison: Option<String>,
    ghidra_source_semantic_summary: Option<String>,
    ghidra_source_semantic_rows: Option<String>,
    ghidra_source_semantic_comparison: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticElapsedMs {
    runner: u64,
    total: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SourceSemanticMetrics {
    row_count: usize,
    behavior_pass_rows: usize,
    behavior_pass_rate: f64,
    behavior_case_pass_count: usize,
    behavior_case_count: usize,
    behavior_case_pass_rate: f64,
    weighted_semantic_similarity_percent: f64,
    static_semantic_score_percent: f64,
    candidate_run_timeout_rows: usize,
    candidate_run_failed_rows: usize,
    behavior_nonpass_rows: usize,
    candidate_output_mismatch_rows: usize,
    behavior_improved_row_count: usize,
    behavior_regressed_row_count: usize,
    improved_row_count: usize,
    regressed_row_count: usize,
    weighted_semantic_similarity_percent_delta: Option<f64>,
    behavior_pass_rate_delta: Option<f64>,
    behavior_case_pass_rate_delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticGate {
    decision: String,
    rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticDecisionInsights {
    go_stop_gate: SourceSemanticGate,
    metrics: SourceSemanticMetrics,
    top_timeout_rows: Vec<SourceSemanticRowBrief>,
    top_behavior_failures: Vec<SourceSemanticRowBrief>,
    top_regressions: Value,
    top_missing_features: Vec<(String, usize)>,
    comparison_outcome: Option<Value>,
    ghidra_reference: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SourceSemanticRowBrief {
    function_name: String,
    address: String,
    behavior_status: String,
    case_pass_count: usize,
    case_fail_count: usize,
    semantic_score_percent: f64,
    static_semantic_score_percent: f64,
    top_missing_features: Vec<(String, usize)>,
}

impl SourceSemanticMetrics {
    fn from_summary(summary: &Value, comparison: Option<&Value>) -> Self {
        let counts = summary.pointer("/admission_gate_metrics/counts");
        let behavior_case = summary.get("behavior_case_metrics");
        let behavior_status_counts = summary.get("behavior_status_counts");
        let static_score = first_f64(summary, &["static_semantic_score_percent"])
            .or_else(|| {
                summary
                    .pointer("/score_component_metrics/static_score_distribution/avg")
                    .and_then(Value::as_f64)
                    .map(|score| score * 100.0)
            })
            .unwrap_or(0.0);
        let metric_deltas = comparison.and_then(|value| value.get("metric_deltas"));
        Self {
            row_count: first_usize(summary, &["row_count"])
                .or_else(|| counts.and_then(|v| value_usize(v.get("manifest_rows"))))
                .unwrap_or(0),
            behavior_pass_rows: counts
                .and_then(|v| value_usize(v.get("behavior_pass_rows")))
                .unwrap_or(0),
            behavior_pass_rate: first_f64(summary, &["behavior_pass_rate"]).unwrap_or(0.0),
            behavior_case_pass_count: behavior_case
                .and_then(|v| value_usize(v.get("case_pass_count")))
                .unwrap_or(0),
            behavior_case_count: behavior_case
                .and_then(|v| value_usize(v.get("case_count")))
                .unwrap_or(0),
            behavior_case_pass_rate: behavior_case
                .and_then(|v| value_f64(v.get("case_pass_rate")))
                .unwrap_or(0.0),
            weighted_semantic_similarity_percent: first_f64(
                summary,
                &["weighted_semantic_similarity_percent"],
            )
            .unwrap_or(0.0),
            static_semantic_score_percent: static_score,
            candidate_run_timeout_rows: behavior_status_counts
                .and_then(|v| value_usize(v.get("candidate_run_timeout")))
                .unwrap_or(0),
            candidate_run_failed_rows: behavior_status_counts
                .and_then(|v| value_usize(v.get("candidate_run_failed")))
                .unwrap_or(0),
            behavior_nonpass_rows: behavior_status_counts
                .and_then(|v| {
                    v.as_object().map(|map| {
                        map.iter()
                            .filter(|(key, _)| key.as_str() != "pass")
                            .filter_map(|(_, value)| value_usize(Some(value)))
                            .sum()
                    })
                })
                .unwrap_or(0),
            candidate_output_mismatch_rows: behavior_status_counts
                .and_then(|v| {
                    v.as_object().map(|map| {
                        map.iter()
                            .filter(|(key, _)| key.contains("mismatch"))
                            .filter_map(|(_, value)| value_usize(Some(value)))
                            .sum()
                    })
                })
                .unwrap_or(0),
            behavior_improved_row_count: comparison
                .and_then(|v| value_usize(v.get("behavior_improved_row_count")))
                .unwrap_or(0),
            behavior_regressed_row_count: comparison
                .and_then(|v| value_usize(v.get("behavior_regressed_row_count")))
                .unwrap_or(0),
            improved_row_count: comparison
                .and_then(|v| value_usize(v.get("improved_row_count")))
                .unwrap_or(0),
            regressed_row_count: comparison
                .and_then(|v| value_usize(v.get("regressed_row_count")))
                .unwrap_or(0),
            weighted_semantic_similarity_percent_delta: metric_delta(
                metric_deltas,
                "weighted_semantic_similarity_percent",
            ),
            behavior_pass_rate_delta: metric_delta(metric_deltas, "behavior_pass_rate"),
            behavior_case_pass_rate_delta: metric_delta(metric_deltas, "behavior_case_pass_rate"),
        }
    }
}

fn decide_gate(metrics: &SourceSemanticMetrics, comparison: Option<&Value>) -> SourceSemanticGate {
    if comparison.is_none() {
        return SourceSemanticGate {
            decision: "stop_no_baseline".to_string(),
            rationale: "source semantic comparison is unavailable".to_string(),
        };
    }
    if metrics.behavior_regressed_row_count > 0
        || metrics
            .behavior_pass_rate_delta
            .is_some_and(|delta| delta < 0.0)
        || metrics
            .behavior_case_pass_rate_delta
            .is_some_and(|delta| delta < 0.0)
    {
        return SourceSemanticGate {
            decision: "stop_behavior_regression".to_string(),
            rationale: format!(
                "behavior regressed rows={} behavior_pass_rate_delta={:?} behavior_case_pass_rate_delta={:?}",
                metrics.behavior_regressed_row_count,
                metrics.behavior_pass_rate_delta,
                metrics.behavior_case_pass_rate_delta
            ),
        };
    }
    if metrics
        .weighted_semantic_similarity_percent_delta
        .is_some_and(|delta| delta < 0.0)
    {
        return SourceSemanticGate {
            decision: "stop_weighted_regression".to_string(),
            rationale: format!(
                "weighted semantic similarity delta={:.3}%",
                metrics
                    .weighted_semantic_similarity_percent_delta
                    .unwrap_or_default()
            ),
        };
    }
    if metrics.behavior_improved_row_count > 0
        || metrics.behavior_pass_rate_delta.unwrap_or(0.0) > 0.0
    {
        return SourceSemanticGate {
            decision: "go_behavior_improved".to_string(),
            rationale: format!(
                "behavior improved rows={} behavior_pass_rate_delta={:?}",
                metrics.behavior_improved_row_count, metrics.behavior_pass_rate_delta
            ),
        };
    }
    if metrics
        .weighted_semantic_similarity_percent_delta
        .is_some_and(|delta| delta > 0.0)
    {
        return SourceSemanticGate {
            decision: "go_static_only".to_string(),
            rationale: format!(
                "weighted/static score improved without behavior row improvement; weighted delta={:.3}%",
                metrics
                    .weighted_semantic_similarity_percent_delta
                    .unwrap_or_default()
            ),
        };
    }
    SourceSemanticGate {
        decision: "go_no_regression".to_string(),
        rationale: "source semantic comparison is available and no regression was detected"
            .to_string(),
    }
}

fn build_source_semantic_command(
    root: &Path,
    manifest: &Path,
    fission_bin: &Path,
    output_dir: &Path,
    timeout_sec: u64,
    jobs: usize,
    args: &SourceSemanticCheckArgs,
) -> Vec<String> {
    let mut command = vec![
        "python3".to_string(),
        rel_path_for_command(root, &source_semantic_runner(root)),
        "--manifest".to_string(),
        rel_path_for_command(root, manifest),
        "--fission-bin".to_string(),
        rel_path_for_command(root, fission_bin),
        "--output-dir".to_string(),
        rel_path_for_command(root, output_dir),
        "--timeout-sec".to_string(),
        timeout_sec.to_string(),
        "--jobs".to_string(),
        jobs.max(1).to_string(),
    ];
    if let Some(baseline_dir) = args.baseline_dir.as_ref() {
        command.push("--baseline-dir".to_string());
        command.push(rel_path_for_command(root, baseline_dir));
    }
    if args.include_debug_decomp {
        command.push("--include-debug-decomp".to_string());
    }
    if args.no_decomp_cache {
        command.push("--no-decomp-cache".to_string());
    }
    if args.no_behavior_cache {
        command.push("--no-behavior-cache".to_string());
    }
    if args.include_ghidra_reference {
        command.push("--include-ghidra-reference".to_string());
        if let Some(ghidra_home) = args.ghidra_home.as_ref() {
            command.push("--ghidra-home".to_string());
            command.push(rel_path_for_command(root, ghidra_home));
        }
    }
    for entry_id in &args.entry_ids {
        command.push("--entry-id".to_string());
        command.push(entry_id.clone());
    }
    for tag in &args.tags {
        command.push("--tag".to_string());
        command.push(tag.clone());
    }
    for function_name in &args.function_names {
        command.push("--function-name".to_string());
        command.push(function_name.clone());
    }
    command
}

fn run_command(root: &Path, command_parts: &[String]) -> Result<std::process::Output> {
    let (program, args) = command_parts
        .split_first()
        .context("source semantic command is empty")?;
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("run source semantic command: {}", shell_join(command_parts)))
}

fn render_source_semantic_markdown(
    summary: &SourceSemanticAutomationSummary,
    insights: &SourceSemanticDecisionInsights,
) -> String {
    let mut out = String::new();
    out.push_str("# Fission Source Semantic Automation Summary\n\n");
    out.push_str(&format!("- Lane: `{}`\n", summary.lane));
    out.push_str(&format!("- Run profile: `{}`\n", summary.run_profile));
    out.push_str(&format!("- Run: `{}`\n", summary.run_id));
    out.push_str(&format!("- Manifest: `{}`\n", summary.manifest));
    out.push_str(&format!(
        "- Source semantic artifacts: `{}`\n",
        summary.source_semantic_dir
    ));
    out.push_str(&format!(
        "- Timings(ms): runner=`{}`, total=`{}`\n",
        summary.elapsed_ms.runner, summary.elapsed_ms.total
    ));
    out.push_str(&format!(
        "- Go/stop gate: `{}` ({})\n\n",
        summary.go_stop_gate.decision, summary.go_stop_gate.rationale
    ));
    out.push_str("## Metrics\n\n");
    out.push_str(&format!(
        "- Weighted semantic similarity: `{:.3}%` (delta: `{}`)\n",
        summary.metrics.weighted_semantic_similarity_percent,
        fmt_opt_delta(summary.metrics.weighted_semantic_similarity_percent_delta)
    ));
    out.push_str(&format!(
        "- Behavior pass rows: `{}` / `{}` (rate `{:.3}` delta `{}`)\n",
        summary.metrics.behavior_pass_rows,
        summary.metrics.row_count,
        summary.metrics.behavior_pass_rate,
        fmt_opt_delta(summary.metrics.behavior_pass_rate_delta)
    ));
    out.push_str(&format!(
        "- Behavior cases: `{}` / `{}` (rate `{:.3}` delta `{}`)\n",
        summary.metrics.behavior_case_pass_count,
        summary.metrics.behavior_case_count,
        summary.metrics.behavior_case_pass_rate,
        fmt_opt_delta(summary.metrics.behavior_case_pass_rate_delta)
    ));
    out.push_str(&format!(
        "- Static score: `{:.3}%`; timeouts: `{}`; run failures: `{}`; mismatches: `{}`\n\n",
        summary.metrics.static_semantic_score_percent,
        summary.metrics.candidate_run_timeout_rows,
        summary.metrics.candidate_run_failed_rows,
        summary.metrics.candidate_output_mismatch_rows
    ));
    out.push_str("## Top Timeout Rows\n\n");
    if insights.top_timeout_rows.is_empty() {
        out.push_str("- none\n\n");
    } else {
        for row in &insights.top_timeout_rows {
            out.push_str(&format!(
                "- `{}` `{}` status=`{}` cases={}/{} semantic={:.3}% static={:.3}% missing={:?}\n",
                row.function_name,
                row.address,
                row.behavior_status,
                row.case_pass_count,
                row.case_pass_count + row.case_fail_count,
                row.semantic_score_percent,
                row.static_semantic_score_percent,
                row.top_missing_features,
            ));
        }
        out.push('\n');
    }
    out.push_str("## Top Behavior Failures\n\n");
    if insights.top_behavior_failures.is_empty() {
        out.push_str("- none\n\n");
    } else {
        for row in &insights.top_behavior_failures {
            out.push_str(&format!(
                "- `{}` `{}` status=`{}` cases={}/{} semantic={:.3}% static={:.3}% missing={:?}\n",
                row.function_name,
                row.address,
                row.behavior_status,
                row.case_pass_count,
                row.case_pass_count + row.case_fail_count,
                row.semantic_score_percent,
                row.static_semantic_score_percent,
                row.top_missing_features,
            ));
        }
        out.push('\n');
    }
    out.push_str("## Static Missing Features\n\n");
    if insights.top_missing_features.is_empty() {
        out.push_str("- none\n\n");
    } else {
        for (feature, count) in &insights.top_missing_features {
            out.push_str(&format!("- `{feature}`: `{count}`\n"));
        }
        out.push('\n');
    }
    out.push_str("## Suggested Changelog Bullets\n\n");
    out.push_str(&format!(
        "- `fission-automation` source semantic lane ran `{}` rows with weighted semantic similarity `{:.3}%`.\n",
        summary.metrics.row_count, summary.metrics.weighted_semantic_similarity_percent
    ));
    out.push_str(&format!(
        "- behavior cases passed `{}/{}`; gate decision is `{}`.\n",
        summary.metrics.behavior_case_pass_count,
        summary.metrics.behavior_case_count,
        summary.go_stop_gate.decision
    ));
    out
}

fn print_terminal_summary(
    summary: &SourceSemanticAutomationSummary,
    insights: &SourceSemanticDecisionInsights,
) {
    println!("[fission-automation] lane={}", summary.lane);
    println!(
        "  weighted={:.3}% behavior_rows={}/{} behavior_cases={}/{} static={:.3}%",
        summary.metrics.weighted_semantic_similarity_percent,
        summary.metrics.behavior_pass_rows,
        summary.metrics.row_count,
        summary.metrics.behavior_case_pass_count,
        summary.metrics.behavior_case_count,
        summary.metrics.static_semantic_score_percent,
    );
    println!(
        "  timeouts={} run_failures={} mismatches={} gate={} changed_rows=+{}/-{}",
        summary.metrics.candidate_run_timeout_rows,
        summary.metrics.candidate_run_failed_rows,
        summary.metrics.candidate_output_mismatch_rows,
        summary.go_stop_gate.decision,
        summary.metrics.improved_row_count,
        summary.metrics.regressed_row_count,
    );
    if let Some(row) = insights.top_timeout_rows.first() {
        println!(
            "  top_timeout={} {} cases={}/{}",
            row.function_name,
            row.address,
            row.case_pass_count,
            row.case_pass_count + row.case_fail_count
        );
    }
    if let Some(row) = insights.top_behavior_failures.first() {
        println!(
            "  top_behavior_failure={} {} status={} cases={}/{}",
            row.function_name,
            row.address,
            row.behavior_status,
            row.case_pass_count,
            row.case_pass_count + row.case_fail_count
        );
    }
    println!(
        "[fission-automation] wrote outputs to {}",
        summary.source_semantic_dir
    );
}

fn top_behavior_failure_rows(rows: &Value, limit: usize) -> Vec<SourceSemanticRowBrief> {
    let mut out = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            let status = row.pointer("/behavior/status").and_then(Value::as_str);
            status.is_some_and(|status| status != "pass")
        })
        .map(row_brief)
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.case_fail_count
            .cmp(&a.case_fail_count)
            .then_with(|| a.function_name.cmp(&b.function_name))
            .then_with(|| a.address.cmp(&b.address))
    });
    out.truncate(limit);
    out
}

fn top_rows_by_behavior_status(
    rows: &Value,
    behavior_status: &str,
    limit: usize,
) -> Vec<SourceSemanticRowBrief> {
    let mut out = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            row.pointer("/behavior/status").and_then(Value::as_str) == Some(behavior_status)
        })
        .map(row_brief)
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.case_pass_count
            .cmp(&a.case_pass_count)
            .then_with(|| a.function_name.cmp(&b.function_name))
            .then_with(|| a.address.cmp(&b.address))
    });
    out.truncate(limit);
    out
}

fn row_brief(row: &Value) -> SourceSemanticRowBrief {
    SourceSemanticRowBrief {
        function_name: string_field(row, "function_name"),
        address: string_field(row, "address"),
        behavior_status: row
            .pointer("/behavior/status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        case_pass_count: row
            .pointer("/behavior/case_pass_count")
            .and_then(|v| value_usize(Some(v)))
            .unwrap_or(0),
        case_fail_count: row
            .pointer("/behavior/case_fail_count")
            .and_then(|v| value_usize(Some(v)))
            .unwrap_or(0),
        semantic_score_percent: value_f64(row.get("semantic_score_percent")).unwrap_or(0.0),
        static_semantic_score_percent: value_f64(row.get("static_semantic_score_percent"))
            .unwrap_or(0.0),
        top_missing_features: missing_features_array(row)
            .into_iter()
            .flatten()
            .take(6)
            .filter_map(feature_count)
            .collect(),
    }
}

fn top_missing_features(rows: &Value, limit: usize) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for row in rows.as_array().into_iter().flatten() {
        for item in missing_features_array(row).into_iter().flatten() {
            if let Some((feature, count)) = feature_count(item) {
                *counts.entry(feature).or_default() += count;
            }
        }
    }
    let mut out = counts.into_iter().collect::<Vec<_>>();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(limit);
    out
}

fn missing_features_array(value: &Value) -> Option<&Vec<Value>> {
    value
        .get("top_missing_features")
        .and_then(Value::as_array)
        .or_else(|| {
            value
                .pointer("/static_similarity_gaps/top_missing_features")
                .and_then(Value::as_array)
        })
}

fn feature_count(value: &Value) -> Option<(String, usize)> {
    Some((
        value.get("feature")?.as_str()?.to_string(),
        value_usize(value.get("count")).unwrap_or(0),
    ))
}

fn default_source_semantic_manifest(root: &Path) -> PathBuf {
    root.join("benchmark")
        .join("source_semantic_benchmark")
        .join("manifests")
        .join("smoke_windows_small_c.json")
}

fn source_semantic_runner(root: &Path) -> PathBuf {
    root.join("benchmark")
        .join("source_semantic_benchmark")
        .join("run_source_semantic_benchmark.py")
}

fn default_timeout_sec(profile: RunProfile) -> u64 {
    match profile {
        RunProfile::Fast => 20,
        RunProfile::Mid => 30,
        RunProfile::Full => 45,
    }
}

fn default_jobs(_profile: RunProfile) -> usize {
    1
}

fn load_json(path: &Path) -> Result<Value> {
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

fn optional_json(path: &Path) -> Result<Option<Value>> {
    if path.exists() {
        load_json(path).map(Some)
    } else {
        Ok(None)
    }
}

fn metric_delta(metric_deltas: Option<&Value>, key: &str) -> Option<f64> {
    metric_deltas?
        .get(key)?
        .get("delta")
        .and_then(Value::as_f64)
}

fn first_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| value_f64(Some(v))))
}

fn first_usize(value: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| value_usize(Some(v))))
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_usize(value: Option<&Value>) -> Option<usize> {
    match value? {
        Value::Number(number) => number.as_u64().map(|v| v as usize),
        Value::String(text) => text.parse::<usize>().ok(),
        _ => None,
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn rel_path_for_command(root: &Path, path: &Path) -> String {
    rel(root, path)
}

fn fmt_opt_delta(value: Option<f64>) -> String {
    value
        .map(|delta| format!("{delta:+.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| {
            if part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
            {
                part.clone()
            } else {
                format!("'{}'", part.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SourceSemanticCheckArgs;

    fn args() -> SourceSemanticCheckArgs {
        SourceSemanticCheckArgs {
            release: false,
            no_build: true,
            fission_bin: None,
            manifest: None,
            output_dir: None,
            baseline_dir: None,
            jobs: None,
            timeout_sec: None,
            function_names: Vec::new(),
            entry_ids: Vec::new(),
            tags: Vec::new(),
            include_debug_decomp: true,
            include_ghidra_reference: false,
            ghidra_home: None,
            fail_on_stop: false,
            no_decomp_cache: false,
            no_behavior_cache: false,
            run_profile: RunProfile::Mid,
        }
    }

    #[test]
    fn source_semantic_command_includes_core_flags() {
        let root = Path::new("/repo");
        let mut args = args();
        args.function_names.push("fibonacci".to_string());
        args.baseline_dir = Some(PathBuf::from(
            "/repo/benchmark/artifacts/source_semantic_baseline",
        ));
        args.include_ghidra_reference = true;
        args.ghidra_home = Some(PathBuf::from("/repo/vendor/ghidra/ghidra_12.0.4_PUBLIC"));
        args.no_decomp_cache = true;
        args.no_behavior_cache = true;
        let command = build_source_semantic_command(
            root,
            Path::new(
                "/repo/benchmark/source_semantic_benchmark/manifests/smoke_windows_small_c.json",
            ),
            Path::new("/repo/target/release/fission_cli"),
            Path::new("/repo/benchmark/artifacts/automation/source-semantic/source_semantic"),
            30,
            1,
            &args,
        );
        assert_eq!(command[0], "python3");
        assert!(command.contains(&"--include-debug-decomp".to_string()));
        assert!(command.contains(&"--baseline-dir".to_string()));
        assert!(command.contains(&"benchmark/artifacts/source_semantic_baseline".to_string()));
        assert!(command.contains(&"--include-ghidra-reference".to_string()));
        assert!(command.contains(&"--ghidra-home".to_string()));
        assert!(command.contains(&"vendor/ghidra/ghidra_12.0.4_PUBLIC".to_string()));
        assert!(command.contains(&"--no-decomp-cache".to_string()));
        assert!(command.contains(&"--no-behavior-cache".to_string()));
        assert!(command.contains(&"--function-name".to_string()));
        assert!(command.contains(&"fibonacci".to_string()));
        assert!(command.contains(&"--jobs".to_string()));
        assert!(command.contains(&"1".to_string()));
    }

    #[test]
    fn source_semantic_metrics_extracts_summary_values() {
        let summary = serde_json::json!({
            "row_count": 2,
            "behavior_pass_rate": 0.5,
            "weighted_semantic_similarity_percent": 42.5,
            "static_semantic_score_percent": 50.0,
            "behavior_case_metrics": {
                "case_pass_count": 3,
                "case_count": 6,
                "case_pass_rate": 0.5
            },
            "behavior_status_counts": {
                "candidate_run_timeout": 1,
                "candidate_output_mismatch": 1
            },
            "admission_gate_metrics": {
                "counts": {
                    "behavior_pass_rows": 1
                }
            }
        });
        let comparison = serde_json::json!({
            "behavior_improved_row_count": 1,
            "behavior_regressed_row_count": 0,
            "improved_row_count": 1,
            "regressed_row_count": 0,
            "metric_deltas": {
                "weighted_semantic_similarity_percent": {"delta": 2.5},
                "behavior_pass_rate": {"delta": 0.5},
                "behavior_case_pass_rate": {"delta": 0.5}
            }
        });
        let metrics = SourceSemanticMetrics::from_summary(&summary, Some(&comparison));
        assert_eq!(metrics.row_count, 2);
        assert_eq!(metrics.behavior_pass_rows, 1);
        assert_eq!(metrics.candidate_run_timeout_rows, 1);
        assert_eq!(metrics.candidate_output_mismatch_rows, 1);
        assert_eq!(
            metrics.weighted_semantic_similarity_percent_delta,
            Some(2.5)
        );
    }

    #[test]
    fn source_semantic_gate_prioritizes_behavior_regression() {
        let metrics = SourceSemanticMetrics {
            behavior_regressed_row_count: 1,
            weighted_semantic_similarity_percent_delta: Some(3.0),
            ..Default::default()
        };
        let decision = decide_gate(&metrics, Some(&serde_json::json!({})));
        assert_eq!(decision.decision, "stop_behavior_regression");
    }

    #[test]
    fn source_semantic_gate_accepts_static_only_improvement() {
        let metrics = SourceSemanticMetrics {
            weighted_semantic_similarity_percent_delta: Some(0.1),
            ..Default::default()
        };
        let decision = decide_gate(&metrics, Some(&serde_json::json!({})));
        assert_eq!(decision.decision, "go_static_only");
    }

    #[test]
    fn source_semantic_gate_stops_without_comparison() {
        let decision = decide_gate(&SourceSemanticMetrics::default(), None);
        assert_eq!(decision.decision, "stop_no_baseline");
    }

    #[test]
    fn source_semantic_markdown_mentions_core_metrics() {
        let summary = SourceSemanticAutomationSummary {
            lane: LANE_NAME.to_string(),
            run_profile: "mid".to_string(),
            run_id: "run".to_string(),
            generated_at: "now".to_string(),
            manifest: "manifest.json".to_string(),
            fission_bin: "target/release/fission_cli".to_string(),
            source_semantic_dir: "artifacts/source_semantic".to_string(),
            runner_status: RunnerStatus {
                command: vec!["python3".to_string()],
                exit_code: Some(0),
                success: true,
                elapsed_ms: 1,
            },
            metrics: SourceSemanticMetrics {
                row_count: 1,
                behavior_case_count: 6,
                behavior_case_pass_count: 4,
                weighted_semantic_similarity_percent: 1.0,
                ..Default::default()
            },
            go_stop_gate: SourceSemanticGate {
                decision: "go_no_regression".to_string(),
                rationale: "ok".to_string(),
            },
            artifact_paths: SourceSemanticArtifactPaths {
                source_semantic_summary: "summary.json".to_string(),
                source_semantic_rows: "rows.json".to_string(),
                source_semantic_comparison: None,
                ghidra_source_semantic_summary: None,
                ghidra_source_semantic_rows: None,
                ghidra_source_semantic_comparison: None,
            },
            elapsed_ms: SourceSemanticElapsedMs {
                runner: 1,
                total: 2,
            },
        };
        let insights = SourceSemanticDecisionInsights {
            go_stop_gate: summary.go_stop_gate.clone(),
            metrics: summary.metrics.clone(),
            top_timeout_rows: Vec::new(),
            top_behavior_failures: Vec::new(),
            top_regressions: Value::Array(Vec::new()),
            top_missing_features: vec![("call:fibonacci".to_string(), 1)],
            comparison_outcome: None,
            ghidra_reference: None,
        };
        let markdown = render_source_semantic_markdown(&summary, &insights);
        assert!(markdown.contains("Weighted semantic similarity"));
        assert!(markdown.contains("call:fibonacci"));
    }
}
