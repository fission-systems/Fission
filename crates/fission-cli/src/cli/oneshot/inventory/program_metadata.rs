use crate::cli::args::OneShotArgs;
use anyhow::{Context, Result};
use fission_analysis_db::{ProgramSnapshot, SnapshotPublication};
use fission_loader::LoadedBinary;
use std::fs;

/// The snapshot describes the binary as `list` and `decomp --all` see it: the
/// one-shot entry point has already run function discovery on `binary`.
pub(crate) fn emit_program_metadata(cli: &OneShotArgs, binary: &LoadedBinary) -> Result<()> {
    let outcome = ProgramSnapshot::publish_loaded_binary_quarantining_rows(binary);
    let snapshot = match outcome.publication {
        SnapshotPublication::Valid(snapshot) => snapshot,
        SnapshotPublication::Partial(partial) => anyhow::bail!(
            "canonical program metadata failed integrity validation: {:?}",
            partial.integrity_report().issues
        ),
    };
    let mut value =
        serde_json::to_value(&snapshot).context("serialize canonical program metadata snapshot")?;
    // Malformed loader rows are left out of the snapshot, never repaired;
    // list them (with reasons) instead of dropping them silently.
    if !outcome.rejected_rows.is_empty() {
        eprintln!(
            "warning: {} malformed loader row(s) left out of the program metadata; see `rejected_rows`",
            outcome.rejected_rows.len()
        );
        value["rejected_rows"] = serde_json::to_value(&outcome.rejected_rows)
            .context("serialize rejected program metadata rows")?;
    }
    let json = serde_json::to_string_pretty(&value)
        .context("serialize canonical program metadata snapshot")?;
    if let Some(path) = &cli.output {
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("write program metadata to {}", path.display()))?;
    } else {
        println!("{json}");
    }
    Ok(())
}
