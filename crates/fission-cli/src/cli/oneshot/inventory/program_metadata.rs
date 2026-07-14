use crate::cli::args::OneShotArgs;
use anyhow::{Context, Result};
use fission_analysis_db::ProgramSnapshot;
use fission_loader::LoadedBinary;
use fission_static::analysis::{FunctionDiscoveryProfile, discover_functions_with_runtime};
use std::fs;

pub(crate) fn emit_program_metadata(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let mut analyzed = binary.clone();
    discover_functions_with_runtime(&mut analyzed, FunctionDiscoveryProfile::Conservative);
    let snapshot = ProgramSnapshot::try_from_loaded_binary(&analyzed).map_err(|report| {
        anyhow::anyhow!(
            "canonical program metadata failed integrity validation: {:?}",
            report.issues
        )
    })?;
    let json = serde_json::to_string_pretty(&snapshot)
        .context("serialize canonical program metadata snapshot")?;
    if let Some(path) = &cli.output {
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("write program metadata to {}", path.display()))?;
    } else {
        println!("{json}");
    }
    Ok(())
}
