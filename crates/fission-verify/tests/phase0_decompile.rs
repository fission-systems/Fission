//! Phase 0 demo: decompile one real corpus function through the same
//! production pipeline `fission-cli` uses, and confirm both the DIR and HIR
//! snapshots come back populated. This is pure scaffolding -- no solver, no
//! emulator, no evaluation -- just proving the snapshot-capture hook works
//! from a crate other than `fission-cli`.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use fission_verify::decompile_one;
use std::path::PathBuf;

fn corpus_binary(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/c")
        .join(name)
}

#[test]
fn decompiles_count_bits_and_captures_both_snapshots() {
    let path = corpus_binary("control_flow_gcc_O0.exe");
    if !path.exists() {
        eprintln!("skipping: corpus binary not found at {}", path.display());
        return;
    }
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "count_bits".to_string(),
        address: 0x140001530,
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    assert!(!pair.dir.body.is_empty(), "DIR body should not be empty");
    assert!(!pair.hir.body.is_empty(), "HIR body should not be empty");
}
