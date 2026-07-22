//! Decompile-one-function + snapshot-capture helper shared by every
//! verification tier. Wraps the same `decompile_with_rust_sleigh_with_facts`
//! + immediate `take_last_dir_snapshot`/`take_last_hir_function_snapshot`
//! pattern `fission-cli`'s `decomp --dir` uses (see
//! `crates/fission-cli/src/cli/oneshot/rust_decomp/mod.rs`) -- the snapshot
//! is a single-slot thread-local overwritten on every decompile call, so it
//! must be read immediately after this call and before any other decompile
//! runs on this thread.

use fission_decompiler::{DirFunction, HirFunction, RustSleighDecompileConfig};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;

/// Both IR snapshots for one function, captured from the same real
/// production decompile pass every other Fission entry point (CLI, tests)
/// goes through -- not a separate/parallel decompile path.
pub struct DirHirPair {
    pub dir: DirFunction,
    pub hir: HirFunction,
}

#[derive(Debug, thiserror::Error)]
pub enum DecompileError {
    #[error("decompile failed: {0}")]
    Decompile(String),
    #[error("decompile succeeded but DIR snapshot was not captured")]
    MissingDirSnapshot,
    #[error("decompile succeeded but HIR snapshot was not captured")]
    MissingHirSnapshot,
}

/// Decompile `func` in `binary` and return its DIR (pre-structuring) and HIR
/// (final structured) snapshots. A missing snapshot after a successful
/// decompile is treated as an error, not silently skipped -- every real
/// production decompile of a function produces both.
pub fn decompile_one(
    binary: &LoadedBinary,
    facts: &FactStore,
    func: &FunctionInfo,
) -> Result<DirHirPair, DecompileError> {
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
    let dir = fission_decompiler::take_last_dir_snapshot().ok_or(DecompileError::MissingDirSnapshot)?;
    let hir = fission_decompiler::take_last_hir_function_snapshot()
        .ok_or(DecompileError::MissingHirSnapshot)?;
    Ok(DirHirPair { dir, hir })
}
