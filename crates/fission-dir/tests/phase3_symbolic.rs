//! Phase 3 demo: decompile a real, loop-free corpus function and ask the
//! real `fission-solver::Solver` to *prove* PreHIR and HIR compute the same
//! return value for every input (not just a handful of boundary samples),
//! then prove the check has teeth by corrupting HIR and confirming the
//! solver finds a genuine counterexample.

use fission_dir::decompile_one;
use fission_dir::symbolic::{SymbolicOutcome, check_symbolic_equivalence};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
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
    match check_symbolic_equivalence(&pair.prehir, &pair.hir) {
        SymbolicOutcome::Equivalent => {}
        SymbolicOutcome::Diverged(cx) => {
            panic!("expected proof, got a counterexample: {:?}", cx.args)
        }
        SymbolicOutcome::Unsupported(reason) => {
            panic!("expected clamp to be provable, got: {reason}")
        }
    }
}

/// Proves the symbolic tier has teeth: corrupting HIR so it always returns
/// the low bound (as if a structuring bug had collapsed the clamp logic)
/// is *provably* different from PreHIR, and the solver hands back a genuine
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

    match check_symbolic_equivalence(&pair.prehir, &corrupted_hir) {
        SymbolicOutcome::Diverged(cx) => {
            assert!(!cx.args.is_empty(), "expected a nonempty counterexample");
        }
        SymbolicOutcome::Equivalent => {
            panic!("expected the corrupted HIR to be provably different")
        }
        SymbolicOutcome::Unsupported(reason) => {
            panic!("expected a decidable result, got: {reason}")
        }
    }
}

/// Swap the first pair of branches found: a `Select`'s then/else, or an
/// `If`'s bodies. Returns whether anything was swapped.
fn swap_first_branch_pair(stmts: &mut [fission_midend_core::ir::HirStmt]) -> bool {
    use fission_midend_core::ir::{HirExpr, HirStmt};

    fn in_expr(expr: &mut HirExpr) -> bool {
        match expr {
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                if in_expr(cond) {
                    return true;
                }
                std::mem::swap(then_expr, else_expr);
                true
            }
            HirExpr::Binary { lhs, rhs, .. } => in_expr(lhs) || in_expr(rhs),
            HirExpr::Cast { .. } | HirExpr::Unary { .. } => false,
            _ => false,
        }
    }

    for stmt in stmts.iter_mut() {
        let swapped = match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                std::mem::swap(then_body, else_body);
                true
            }
            HirStmt::Return(Some(expr)) | HirStmt::Expr(expr) => in_expr(expr),
            HirStmt::Assign { rhs, .. } => in_expr(rhs),
            HirStmt::Block(inner) => swap_first_branch_pair(inner),
            _ => false,
        };
        if swapped {
            return true;
        }
    }
    false
}

/// The teeth the test above lacks. Returning the first parameter removes
/// every branch from HIR, so it diverges even from a solver that thinks all
/// branches merge to zero -- and the solver did think that: it lowered
/// `Ite` to all-false bits, so both sides of a clamp returned 0 and the proof
/// was "0 == 0". Swapping the branches keeps the merge and changes what it
/// selects, which only a solver that encodes `Ite` can see.
#[test]
fn swapped_branches_are_not_proven_equivalent() {
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
    let mut swapped_hir = pair.hir.clone();
    assert!(
        swap_first_branch_pair(&mut swapped_hir.body),
        "clamp's HIR has no branch to swap; this test no longer checks anything: {:#?}",
        pair.hir.body
    );

    match check_symbolic_equivalence(&pair.prehir, &swapped_hir) {
        SymbolicOutcome::Diverged(cx) => {
            assert!(!cx.args.is_empty(), "expected a nonempty counterexample");
        }
        SymbolicOutcome::Equivalent => panic!(
            "a clamp with its branches swapped was proven equivalent to the original: \
             the solver is not encoding if-then-else"
        ),
        SymbolicOutcome::Unsupported(reason) => {
            panic!("expected a decidable result, got: {reason}")
        }
    }
}
