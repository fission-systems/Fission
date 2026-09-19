//! Decompile-one-function + snapshot-capture helper shared by every
//! verification tier. Wraps the same `decompile_with_rust_sleigh_with_facts`
//! + `last_prehir_snapshot`/`last_hir_function_snapshot` capture used by
//!   `fission-cli`'s `decomp --prehir` path -- the observations are cloned by
//!   readers and reset when the next render starts.

use fission_decompiler::{HirFunction, PreHirFunction, RustSleighDecompileConfig};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;

/// Both IR snapshots for one function, captured from the same real
/// production decompile pass every other Fission entry point (CLI, tests)
/// goes through -- not a separate/parallel decompile path.
pub struct PreHirHirPair {
    pub prehir: PreHirFunction,
    pub hir: HirFunction,
}

#[derive(Debug, thiserror::Error)]
pub enum DecompileError {
    #[error("decompile failed: {0}")]
    Decompile(String),
    #[error("decompile succeeded but PreHIR snapshot was not captured")]
    MissingPreHirSnapshot,
    #[error("decompile succeeded but HIR snapshot was not captured")]
    MissingHirSnapshot,
}

/// Decompile `func` in `binary` and return its PreHIR and HIR
/// (final structured) snapshots. A missing snapshot after a successful
/// decompile is treated as an error, not silently skipped -- every real
/// production decompile of a function produces both.
pub fn decompile_one(
    binary: &LoadedBinary,
    facts: &FactStore,
    func: &FunctionInfo,
) -> Result<PreHirHirPair, DecompileError> {
    let config = RustSleighDecompileConfig::cli_defaults();
    fission_decompiler::decompile_with_rust_sleigh_with_facts(
        binary,
        facts,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(DecompileError::Decompile)?;

    // Must read both immediately after the call above -- see this module's
    // own doc comment.
    let prehir =
        fission_decompiler::last_prehir_snapshot().ok_or(DecompileError::MissingPreHirSnapshot)?;
    let hir = fission_decompiler::last_hir_function_snapshot()
        .ok_or(DecompileError::MissingHirSnapshot)?;
    Ok(PreHirHirPair { prehir, hir })
}
