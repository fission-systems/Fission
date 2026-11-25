//! Phase 1 demo: decompile a real corpus function and differentially
//! interpret its PreHIR and HIR snapshots over the default boundary-value
//! sample set. No solver, no emulator -- proving the concrete evaluator +
//! diff harness works end-to-end against real production decompiler output,
//! not just hand-built fixtures.

use fission_dir::report::VerifyOutcome;
use fission_dir::{decompile_one, default_samples, diff_prehir_hir};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

/// A MinGW x64 PE built from `fission-emulator/testdata/src/
/// mingw_control_flow.c`. These tests read `control_flow_gcc_O0.exe` from a
/// local dev corpus and skipped when it was absent -- which was everywhere,
/// CI included, so none of them had run.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-emulator/testdata/x64_mingw_control_flow.exe")
}

/// A function's entry, from the fixture's COFF symbols.
fn entry_of(binary: &LoadedBinary, name: &str) -> u64 {
    binary
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} in the fixture's symbol table"))
        .address
}

#[test]
fn count_bits_dir_and_hir_are_equivalent() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "count_bits".to_string(),
        address: entry_of(&binary, "count_bits"),
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    let samples = default_samples(pair.hir.params.len());
    let outcome = diff_prehir_hir(&pair.prehir, &pair.hir, &samples);

    // `count_bits`'s PreHIR-vs-HIR diff was already hand-confirmed benign this
    // session (a `while(1){if(!c)break;...}` -> `while(c){...}` structural
    // fold, per `scripts/quality/prehir_hir_check.py`'s docstring) -- this
    // tier, unlike that Python text-diff heuristic, actually *evaluates*
    // both sides and should agree they're semantically equivalent, not just
    // structurally different.
    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(checked > 0, "expected at least one comparable sample");
        }
        other => panic!("expected count_bits PreHIR/HIR to evaluate equivalent, got {other:?}"),
    }
}
