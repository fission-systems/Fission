//! Phase 1 demo: decompile a real corpus function and differentially
//! interpret its DIR and HIR snapshots over the default boundary-value
//! sample set. No solver, no emulator -- proving the concrete evaluator +
//! diff harness works end-to-end against real production decompiler output,
//! not just hand-built fixtures.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use fission_verify::report::VerifyOutcome;
use fission_verify::{decompile_one, default_samples, diff_dir_hir};
use std::path::PathBuf;

fn corpus_binary(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/c")
        .join(name)
}

#[test]
fn count_bits_dir_and_hir_are_equivalent() {
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
    let samples = default_samples(pair.hir.params.len());
    let outcome = diff_dir_hir(&pair.dir, &pair.hir, &samples);

    // `count_bits`'s DIR-vs-HIR diff was already hand-confirmed benign this
    // session (a `while(1){if(!c)break;...}` -> `while(c){...}` structural
    // fold, per `scripts/quality/dir_hir_check.py`'s docstring) -- this
    // tier, unlike that Python text-diff heuristic, actually *evaluates*
    // both sides and should agree they're semantically equivalent, not just
    // structurally different.
    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(checked > 0, "expected at least one comparable sample");
        }
        other => panic!("expected count_bits DIR/HIR to evaluate equivalent, got {other:?}"),
    }
}
