//! Phase 2 demo: decompile a real corpus function, then drive the real
//! `fission-emulator::Emulator` to actually call the compiled function with
//! concrete arguments and compare its real return value against both DIR's
//! and HIR's concrete evaluation. This is the tier `scripts/quality/
//! dir_hir_check.py` and Phase 1's `diff_dir_hir` can't provide on their
//! own -- a real, non-decompiler-derived ground truth.

use fission_midend_core::ir::HirStmt;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use fission_verify::report::VerifyOutcome;
use fission_verify::{EmulatorHarness, check_ground_truth, decompile_one, default_samples};
use std::path::PathBuf;

fn corpus_binary(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/c")
        .join(name)
}

#[test]
fn count_bits_matches_real_machine_code() {
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

    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let outcome = check_ground_truth(&mut harness, func.address, &pair.dir, &pair.hir, &samples);

    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(checked > 0, "expected at least one sample groundable against the emulator");
        }
        other => panic!(
            "expected count_bits DIR/HIR to match the real emulator's return value, got {other:?}"
        ),
    }
}

/// Proves ground-truth checking has teeth: a HIR body that's been corrupted
/// to always return 0 (as if a structuring bug had collapsed the whole
/// accumulator loop away) still evaluates "successfully" (it's a valid,
/// evaluable tree -- `diff_dir_hir` alone can't tell it's wrong except by
/// disagreeing with DIR), but the real emulator's ground truth catches it
/// for any input where the real answer isn't 0.
#[test]
fn corrupted_hir_is_caught_against_real_machine_code() {
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
    let mut corrupted_hir = pair.hir.clone();
    corrupted_hir.body = vec![HirStmt::Return(Some(fission_midend_core::ir::HirExpr::Const(
        0,
        corrupted_hir.return_type.clone(),
    )))];

    let samples = default_samples(pair.hir.params.len());
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let outcome = check_ground_truth(&mut harness, func.address, &pair.dir, &corrupted_hir, &samples);

    match outcome {
        VerifyOutcome::Diverged(divs) => {
            assert!(!divs.is_empty(), "expected at least one real divergence");
            // Any nonzero-arg sample should diverge (real count_bits(x) != 0
            // for x with any set bit; corrupted HIR always returns 0).
            let found = divs.iter().any(|d| d.hir_result == Some(0) && d.emulator_result != Some(0));
            assert!(found, "expected a hir=0-vs-real-machine-nonzero divergence, got {divs:?}");
        }
        other => panic!("expected the corrupted HIR to diverge from real machine code, got {other:?}"),
    }
}
