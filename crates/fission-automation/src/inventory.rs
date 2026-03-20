use crate::model::{InventoryRow, InventorySummary};
use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

pub fn ensure_fission_cli(
    root: &Path,
    release: bool,
    no_build: bool,
    explicit_bin: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(bin) = explicit_bin {
        if bin.exists() {
            return Ok(bin.to_path_buf());
        }
        bail!("fission binary not found: {}", bin.display());
    }

    let profile_dir = if release { "release" } else { "debug" };
    let bin = root.join("target").join(profile_dir).join("fission_cli");
    if bin.exists() {
        return Ok(bin);
    }
    if no_build {
        bail!("fission_cli does not exist at {}", bin.display());
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .arg("build")
        .arg("-p")
        .arg("fission-cli")
        .arg("--features")
        .arg("native_decomp");
    if release {
        cmd.arg("--release");
    }
    let status = cmd.status().context("run cargo build for fission-cli")?;
    if !status.success() {
        bail!("cargo build -p fission-cli failed");
    }
    if !bin.exists() {
        bail!("expected built fission_cli at {}", bin.display());
    }
    Ok(bin)
}

pub fn run_inventory_emit(
    root: &Path,
    fission_bin: &Path,
    binary_path: &Path,
    output_jsonl: &Path,
    summary_json: &Path,
    functions_limit: Option<usize>,
    timeout_ms: u64,
) -> Result<(Vec<InventoryRow>, InventorySummary)> {
    if let Some(parent) = output_jsonl.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output dir {}", parent.display()))?;
    }
    if let Some(parent) = summary_json.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create summary dir {}", parent.display()))?;
    }

    let mut cmd = Command::new(fission_bin);
    cmd.current_dir(root)
        .arg(binary_path)
        .arg("--emit-function-facts-inventory")
        .arg("--output-jsonl")
        .arg(output_jsonl)
        .arg("--summary-json")
        .arg(summary_json)
        .arg("--chunk-size")
        .arg("100")
        .arg("--timeout-ms")
        .arg(timeout_ms.to_string())
        .arg("--quiet-batch-errors");
    if let Some(limit) = functions_limit {
        cmd.arg("--functions-limit").arg(limit.to_string());
    }
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "run function facts inventory for {}",
            binary_path.display()
        )
    })?;
    let expected_functions = functions_limit.unwrap_or(100).max(1) as u64;
    let hard_timeout_ms = timeout_ms
        .saturating_mul(expected_functions)
        .saturating_mul(2)
        .clamp(30_000, 600_000);
    let hard_timeout = Duration::from_millis(hard_timeout_ms);
    let status = wait_with_timeout(&mut child, hard_timeout).with_context(|| {
        format!(
            "run function facts inventory for {}",
            binary_path.display()
        )
    })?;
    if !status.success() {
        bail!(
            "function facts inventory failed for {}",
            binary_path.display()
        );
    }
    let rows = load_rows(output_jsonl)?;
    let summary = load_summary(summary_json)?;
    Ok((rows, summary))
}

fn load_rows(path: &Path) -> Result<Vec<InventoryRow>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for line in reader.lines() {
        let line = line.with_context(|| format!("read line from {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str::<InventoryRow>(&line)
                .with_context(|| format!("parse inventory row from {}", path.display()))?,
        );
    }
    Ok(rows)
}

fn load_summary(path: &Path) -> Result<InventorySummary> {
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse inventory summary {}", path.display()))
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("poll inventory process")? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let pid = child.id();
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "inventory process timed out after {:.1}s (pid {})",
                timeout.as_secs_f64(),
                pid
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}
