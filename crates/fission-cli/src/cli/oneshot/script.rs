//! Rhai `script check` / `script run` commands.

use crate::cli::args::{ScriptCmd, ScriptInvocation};
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_script::{ScriptLimits, ScriptRunStatus};
use std::fs;

pub fn execute_script(invocation: ScriptInvocation) -> Result<()> {
    match invocation.cmd {
        ScriptCmd::Check { script } => {
            anyhow::ensure!(
                script.exists(),
                "script path does not exist: {}",
                script.display()
            );
            let source = fs::read_to_string(&script)
                .with_context(|| format!("failed to read script `{}`", script.display()))?;
            fission_script::check_script(&source).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            if invocation.verbose {
                eprintln!(
                    "[ok] Rhai script `{}` passes compile check",
                    script.display()
                );
            }
            Ok(())
        }
        ScriptCmd::Run {
            binary,
            script,
            json,
        } => {
            anyhow::ensure!(
                binary.exists(),
                "binary path does not exist: {}",
                binary.display()
            );
            anyhow::ensure!(
                script.exists(),
                "script path does not exist: {}",
                script.display()
            );

            if invocation.verbose {
                eprintln!(
                    "[*] Running script `{}` on {}",
                    script.display(),
                    binary.display()
                );
            }

            let binary_data = fs::read(&binary)
                .with_context(|| format!("failed to read binary `{}`", binary.display()))?;

            let loaded_raw =
                LoadedBinary::from_bytes(binary_data, binary.to_string_lossy().to_string())
                    .with_context(|| format!("failed to parse binary `{}`", binary.display()))?;

            let loaded = fission_script::prepare_binary_for_script(loaded_raw);

            let source = fs::read_to_string(&script)
                .with_context(|| format!("failed to read script `{}`", script.display()))?;

            let result = fission_script::run_script(
                &loaded,
                &source,
                &script.display().to_string(),
                ScriptLimits::default(),
            );

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("status: {:?}", result.status);
                println!("findings: {}", result.findings.len());
                for d in &result.diagnostics {
                    eprintln!("[{}] {}", d.severity, d.message);
                }
            }

            match result.status {
                ScriptRunStatus::Ok => Ok(()),
                ScriptRunStatus::Error | ScriptRunStatus::Timeout => Err(anyhow::anyhow!(
                    "script finished with status {:?}",
                    result.status
                )),
            }
        }
    }
}
