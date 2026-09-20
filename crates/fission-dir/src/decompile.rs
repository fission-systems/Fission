//! Decompile-one-function + snapshot-capture helper shared by every
//! verification tier. Wraps the same `decompile_with_rust_sleigh_with_facts`
//! call used by every other verification tier and consumes the typed render
//! artifacts returned by that call.

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
    #[error("decompile succeeded but typed render output was not captured")]
    MissingRenderOutput,
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
    let result = fission_decompiler::decompile_with_rust_sleigh_with_facts(
        binary,
        facts,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(DecompileError::Decompile)?;

    let output = result
        .render_output
        .ok_or(DecompileError::MissingRenderOutput)?;
    let prehir = output.prehir.ok_or(DecompileError::MissingPreHirSnapshot)?;
    let hir = output
        .hir_function
        .ok_or(DecompileError::MissingHirSnapshot)?;
    Ok(PreHirHirPair { prehir, hir })
}
