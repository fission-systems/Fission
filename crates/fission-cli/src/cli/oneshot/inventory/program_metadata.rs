use crate::cli::args::OneShotArgs;
use anyhow::{Context, Result};
use fission_analysis_db::ProgramSnapshot;
use fission_loader::LoadedBinary;
use std::fs;

pub(crate) fn emit_program_metadata(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let snapshot = ProgramSnapshot::from_loaded_binary(binary);
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
