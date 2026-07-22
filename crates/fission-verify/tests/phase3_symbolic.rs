//! Phase 3 demo: decompile a real, loop-free corpus function and ask the
//! real `fission-solver::Solver` to *prove* DIR and HIR compute the same
//! return value for every input (not just a handful of boundary samples),
//! then prove the check has teeth by corrupting HIR and confirming the
//! solver finds a genuine counterexample.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use fission_verify::decompile_one;
use fission_verify::symbolic::{SymbolicOutcome, check_symbolic_equivalence};
use std::path::PathBuf;

fn corpus_binary(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/c")
        .join(name)
}

#[test]
fn clamp_dir_and_hir_are_provably_equivalent() {
    let path = corpus_binary("control_flow_gcc_O0.exe");
    if !path.exists() {
        eprintln!("skipping: corpus binary not found at {}", path.display());
        return;
    }
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    // `clamp` is loop-free (if/goto-diamond -> ternary-folded in HIR, per
    // this session's earlier `hir_presentation_recovers_clamp_goto_diamond`
    // finding) -- exactly the symbolic tier's v1 scope.
    let func = FunctionInfo {
        name: "clamp".to_string(),
        address: 0x14000155f,
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    match check_symbolic_equivalence(&pair.dir, &pair.hir) {
        SymbolicOutcome::Equivalent => {}
        SymbolicOutcome::Diverged(cx) => panic!("expected proof, got a counterexample: {:?}", cx.args),
        SymbolicOutcome::Unsupported(reason) => panic!("expected clamp to be provable, got: {reason}"),
    }
}

/// Proves the symbolic tier has teeth: corrupting HIR so it always returns
/// the low bound (as if a structuring bug had collapsed the clamp logic)
/// is *provably* different from DIR, and the solver hands back a genuine
/// counterexample -- not a guess from a fixed sample set.
#[test]
fn corrupted_hir_yields_a_real_counterexample() {
    let path = corpus_binary("control_flow_gcc_O0.exe");
    if !path.exists() {
        eprintln!("skipping: corpus binary not found at {}", path.display());
        return;
    }
    let binary = LoadedBinary::from_file(&path).expect("load binary");
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: "clamp".to_string(),
        address: 0x14000155f,
        ..Default::default()
    };

    let pair = decompile_one(&binary, &facts, &func).expect("decompile_one");
    let mut corrupted_hir = pair.hir.clone();
    let first_param = corrupted_hir.params[0].name.clone();
    corrupted_hir.body = vec![fission_midend_core::ir::HirStmt::Return(Some(
        fission_midend_core::ir::HirExpr::Var(first_param),
    ))];

    match check_symbolic_equivalence(&pair.dir, &corrupted_hir) {
        SymbolicOutcome::Diverged(cx) => {
            assert!(!cx.args.is_empty(), "expected a nonempty counterexample");
        }
        SymbolicOutcome::Equivalent => panic!("expected the corrupted HIR to be provably different"),
        SymbolicOutcome::Unsupported(reason) => panic!("expected a decidable result, got: {reason}"),
    }
}
