//! Emulator sandbox quality lane: run checked-in fixtures via `fission_cli sandbox`
//! and gate on unimplemented-opcode budget.

use crate::inventory::ensure_fission_cli;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxFixture {
    pub name: String,
    pub path: String,
    pub max_inst: u64,
    pub max_unimpl_events: u64,
    pub max_unimpl_kinds: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRowResult {
    pub name: String,
    pub path: String,
    pub ok: bool,
    pub budget_ok: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub metrics_path: Option<String>,
    pub report: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxCheckSummary {
    pub generated_at_unix: u64,
    pub fission_bin: String,
    pub fixtures: Vec<SandboxRowResult>,
    pub passed: usize,
    pub failed: usize,
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug)]
pub struct SandboxCheckArgs {
    pub release: bool,
    pub no_build: bool,
    pub fission_bin: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub fail_on_stop: bool,
    pub dry_run: bool,
}

fn default_fixtures(root: &Path) -> Vec<SandboxFixture> {
    let base = root.join("crates/fission-emulator/testdata");
    vec![
        SandboxFixture {
            name: "linux_x64_hello_sys".into(),
            path: base
                .join("linux_x64_hello_sys.elf")
                .to_string_lossy()
                .into(),
            max_inst: 64,
            max_unimpl_events: 0,
            max_unimpl_kinds: 0,
        },
        SandboxFixture {
            name: "win_x64_exit".into(),
            path: base.join("win_x64_exit.exe").to_string_lossy().into(),
            max_inst: 5_000,
            max_unimpl_events: 64,
            max_unimpl_kinds: 8,
        },
        SandboxFixture {
            name: "win_x64_write".into(),
            path: base.join("win_x64_write.exe").to_string_lossy().into(),
            max_inst: 10_000,
            max_unimpl_events: 64,
            max_unimpl_kinds: 8,
        },
    ]
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

pub fn run(args: SandboxCheckArgs) -> Result<()> {
    let root = repo_root();
    let fission_bin = ensure_fission_cli(
        &root,
        args.release,
        args.no_build,
        args.fission_bin.as_deref(),
    )?;

    let run_id = now_unix();
    let out_dir = args.output_dir.unwrap_or_else(|| {
        root.join("benchmark/artifacts/automation")
            .join(format!("sandbox-check-{run_id}"))
    });
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    let fixtures = default_fixtures(&root);
    let mut rows = Vec::new();

    for fix in &fixtures {
        let bin_path = PathBuf::from(&fix.path);
        if !bin_path.is_file() {
            rows.push(SandboxRowResult {
                name: fix.name.clone(),
                path: fix.path.clone(),
                ok: false,
                budget_ok: false,
                exit_code: None,
                error: Some(format!("fixture missing: {}", fix.path)),
                metrics_path: None,
                report: None,
            });
            continue;
        }

        if args.dry_run {
            rows.push(SandboxRowResult {
                name: fix.name.clone(),
                path: fix.path.clone(),
                ok: true,
                budget_ok: true,
                exit_code: Some(0),
                error: None,
                metrics_path: None,
                report: None,
            });
            continue;
        }

        let metrics_path = out_dir.join(format!("{}.metrics.json", fix.name));
        let status = Command::new(&fission_bin)
            .current_dir(&root)
            .arg("sandbox")
            .arg(&bin_path)
            .arg("--max-inst")
            .arg(fix.max_inst.to_string())
            .arg("--metrics-out")
            .arg(&metrics_path)
            .arg("--max-unimpl-events")
            .arg(fix.max_unimpl_events.to_string())
            .arg("--max-unimpl-kinds")
            .arg(fix.max_unimpl_kinds.to_string())
            .arg("--fail-on-budget")
            .status()
            .with_context(|| format!("run sandbox for {}", fix.name))?;

        let exit_code = status.code();
        let report = if metrics_path.is_file() {
            let text = fs::read_to_string(&metrics_path).ok();
            text.and_then(|t| serde_json::from_str(&t).ok())
        } else {
            None
        };

        let budget_ok = report
            .as_ref()
            .and_then(|r: &serde_json::Value| r.get("budget"))
            .and_then(|b| b.get("ok"))
            .and_then(|v| v.as_bool())
            .unwrap_or(status.success());

        let ok = status.success() && budget_ok;
        rows.push(SandboxRowResult {
            name: fix.name.clone(),
            path: fix.path.clone(),
            ok,
            budget_ok,
            exit_code,
            error: if ok {
                None
            } else {
                Some(format!(
                    "sandbox failed exit={exit_code:?} budget_ok={budget_ok}"
                ))
            },
            metrics_path: Some(metrics_path.display().to_string()),
            report,
        });
    }

    let passed = rows.iter().filter(|r| r.ok).count();
    let failed = rows.len().saturating_sub(passed);
    let (decision, rationale) = if failed == 0 {
        (
            "go_sandbox_budget".to_string(),
            format!("all {passed} sandbox fixtures passed budget gates"),
        )
    } else {
        (
            "stop_sandbox_budget".to_string(),
            format!("{failed}/{total} sandbox fixtures failed", total = rows.len()),
        )
    };

    let summary = SandboxCheckSummary {
        generated_at_unix: now_unix(),
        fission_bin: fission_bin.display().to_string(),
        fixtures: rows,
        passed,
        failed,
        decision: decision.clone(),
        rationale: rationale.clone(),
    };

    let summary_path = out_dir.join("summary.json");
    let json = serde_json::to_string_pretty(&summary).context("serialize sandbox summary")?;
    fs::write(&summary_path, &json)
        .with_context(|| format!("write {}", summary_path.display()))?;

    println!("sandbox-check: {decision} — {rationale}");
    println!("artifacts: {}", out_dir.display());
    println!("summary: {}", summary_path.display());

    if args.fail_on_stop && !decision.starts_with("go_") {
        bail!("sandbox-check stopped: {rationale}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_fixtures_point_under_emulator_testdata() {
        let root = repo_root();
        let fixtures = default_fixtures(&root);
        assert_eq!(fixtures.len(), 3);
        assert!(fixtures.iter().all(|f| f.path.contains("fission-emulator")));
    }
}
