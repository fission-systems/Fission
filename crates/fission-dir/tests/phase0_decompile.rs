//! Phase 0 demo: decompile one real corpus function through the same
//! production pipeline `fission-cli` uses, and confirm both the PreHIR and HIR
//! snapshots come back populated. This is pure scaffolding -- no solver, no
//! emulator, no evaluation -- just proving the snapshot-capture hook works
//! from a crate other than `fission-cli`.

use fission_dir::decompile_one;
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
fn decompiles_count_bits_and_captures_both_snapshots() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "count_bits".to_string(),
        address: entry_of(&binary, "count_bits"),
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    assert!(
        !pair.prehir.body.is_empty(),
        "PreHIR body should not be empty"
    );
    assert!(!pair.hir.body.is_empty(), "HIR body should not be empty");
}
