//! Phase 2 demo: decompile a real corpus function, then drive the real
//! `fission-emulator::Emulator` to actually call the compiled function with
//! concrete arguments and compare its real return value against both PreHIR's
//! and HIR's concrete evaluation. This is the tier `scripts/quality/
//! prehir_hir_check.py` and Phase 1's `diff_prehir_hir` can't provide on their
//! own -- a real, non-decompiler-derived ground truth.

use fission_dir::report::VerifyOutcome;
use fission_dir::{
    EmulatorHarness, Grounding, RunPlan, check_ground_truth, check_ground_truth_with, decompile_one,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_midend_core::ir::HirStmt;
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
fn count_bits_matches_real_machine_code() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "count_bits".to_string(),
        address: entry_of(&binary, "count_bits"),
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    let samples = RunPlan::default();

    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let outcome = check_ground_truth_with(
        &mut harness,
        func.address,
        &pair.prehir,
        &pair.hir,
        &samples,
        &Grounding::for_function(&binary, func.address, &pair),
    );

    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(
                checked > 0,
                "expected at least one sample groundable against the emulator"
            );
        }
        other => panic!(
            "expected count_bits PreHIR/HIR to match the real emulator's return value, got {other:?}"
        ),
    }
}

#[test]
fn float_return_signature_reports_unsupported() {
    let path = fixture();
    let entry = entry_of(
        &LoadedBinary::from_file(&path).expect("load binary"),
        "count_bits",
    );
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let mut hir = fission_midend_core::ir::HirFunction::default();
    hir.return_type = fission_midend_core::ir::NirType::Float { bits: 32 };
    let prehir = fission_midend_prehir::PreHirFunction::default();
    let outcome = check_ground_truth(&mut harness, entry, &prehir, &hir, &RunPlan::default());
    match outcome {
        VerifyOutcome::Unsupported(fission_dir::report::UnsupportedReason::Construct(msg)) => {
            assert_eq!(msg, "floating-point ABI in ground-truth");
        }
        other => panic!("expected Unsupported construct, got {other:?}"),
    }
}

/// Proves ground-truth checking has teeth: a HIR body that's been corrupted
/// to always return 0 (as if a structuring bug had collapsed the whole
/// accumulator loop away) still evaluates "successfully" (it's a valid,
/// evaluable tree -- `diff_prehir_hir` alone can't tell it's wrong except by
/// disagreeing with PreHIR), but the real emulator's ground truth catches it
/// for any input where the real answer isn't 0.
#[test]
fn corrupted_hir_is_caught_against_real_machine_code() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "count_bits".to_string(),
        address: entry_of(&binary, "count_bits"),
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    let mut corrupted_hir = pair.hir.clone();
    corrupted_hir.body = vec![HirStmt::Return(Some(
        fission_midend_core::ir::HirExpr::Const(0, corrupted_hir.return_type.clone()),
    ))];

    let samples = RunPlan::default();
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let outcome = check_ground_truth_with(
        &mut harness,
        func.address,
        &pair.prehir,
        &corrupted_hir,
        &samples,
        &Grounding::for_function(&binary, func.address, &pair),
    );

    match outcome {
        VerifyOutcome::Diverged(divs) => {
            assert!(!divs.is_empty(), "expected at least one real divergence");
            // Any nonzero-arg sample should diverge (real count_bits(x) != 0
            // for x with any set bit; corrupted HIR always returns 0).
            let found = divs
                .iter()
                .any(|d| d.hir_result == Some(0) && d.emulator_result != Some(0));
            assert!(
                found,
                "expected a hir=0-vs-real-machine-nonzero divergence, got {divs:?}"
            );
        }
        other => {
            panic!("expected the corrupted HIR to diverge from real machine code, got {other:?}")
        }
    }
}

/// Regression test for the real bug this session's manual `clamp`
/// investigation found via this exact tier: a 3-argument, `-O0`-compiled
/// function whose prologue spills its register args to stack home slots
/// (needing `EmulatorHarness`'s Win64 shadow-space reservation), where one
/// arg is negative (needing `fission-emulator`'s JIT to sign-extend a
/// narrower-than-register operand before a signed comparison -- see
/// `jit::compiler::tests::signed_ops_on_narrow_negative_memory_operand_are_
/// correct` in `fission-emulator` for the unit-level regression). Both
/// fixes land in this one real-corpus function.
#[test]
fn clamp_matches_real_machine_code_including_negative_args() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "clamp".to_string(),
        address: entry_of(&binary, "clamp"),
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    // Edge runs include -1 and the 32-bit sign bit for every argument.
    let samples = RunPlan::default();

    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("build emulator");
    let outcome = check_ground_truth_with(
        &mut harness,
        func.address,
        &pair.prehir,
        &pair.hir,
        &samples,
        &Grounding::for_function(&binary, func.address, &pair),
    );

    match outcome {
        VerifyOutcome::Equivalent { checked } => {
            assert!(
                checked > 0,
                "expected at least one sample groundable against the emulator"
            );
        }
        other => panic!(
            "expected clamp (including negative args) to match the real emulator's return \
             value, got {other:?}"
        ),
    }
}
