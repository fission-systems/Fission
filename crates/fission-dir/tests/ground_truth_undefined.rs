//! Reads of what C leaves undefined, on the fixture checked into this crate
//! (`testdata/ground_truth_fixture.c`).
//!
//! A run where both sides read such a thing -- a variable before anything
//! assigns it, a frame byte nothing in the run wrote -- says nothing about the
//! decompiled code: `UNKNOWN`. A run where only the decompiled code does is a
//! difference: it reads something the machine never needed.

use fission_dir::report::VerifyOutcome;
use fission_dir::{EmulatorHarness, Grounding, RunPlan, check_ground_truth_with, decompile_one};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_midend_core::ir::{HirBinaryOp, HirExpr, HirFunction, HirStmt, NirBinding, NirType};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

const RUNS: usize = 32;

fn check_edited(name: &str, edit: impl FnOnce(&mut HirFunction)) -> VerifyOutcome {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ground_truth_fixture.elf");
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let address = {
        let inner = binary.inner();
        let index = *inner
            .function_name_index
            .get(name)
            .unwrap_or_else(|| panic!("{name} in the fixture's symbols"));
        inner.functions[index].address
    };
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: name.to_string(),
        address,
        ..Default::default()
    };
    let mut pair = decompile_one(&binary, &facts, &func).expect("decompile");
    edit(&mut pair.hir);
    let grounding = Grounding::for_function(&binary, address, &pair);
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("harness");
    check_ground_truth_with(
        &mut harness,
        address,
        &pair.prehir,
        &pair.hir,
        &RunPlan {
            runs: RUNS,
            ..RunPlan::default()
        },
        &grounding,
    )
}

/// `volatile int x; return a + x;` -- the machine reads a stack slot nothing
/// wrote, and the decompiled code reads the local nothing assigned. Neither
/// run says anything about the decompilation.
#[test]
fn both_sides_reading_garbage_is_unknown() {
    let outcome = check_edited("read_garbage", |_| {});
    assert!(
        matches!(&outcome, VerifyOutcome::Inconclusive { reason } if reason.contains("both sides")),
        "expected UNKNOWN, got {outcome:?}"
    );
}

/// `return p[0] + p[3];`, edited to add a local nothing assigns into the
/// result (times zero, so the value cannot matter). The machine reads only
/// what the run defined, so the read alone is a difference.
#[test]
fn only_the_decompiled_side_reading_garbage_differs() {
    let outcome = check_edited("first_plus_fourth", |hir| {
        let ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        hir.locals.push(NirBinding {
            name: "ghost".to_string(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        });
        let ret = hir
            .body
            .iter_mut()
            .rev()
            .find_map(|stmt| match stmt {
                HirStmt::Return(Some(expr)) => Some(expr),
                _ => None,
            })
            .expect("a return");
        let value = std::mem::replace(ret, HirExpr::Const(0, ty.clone()));
        *ret = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(value),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(HirExpr::Var("ghost".to_string())),
                rhs: Box::new(HirExpr::Const(0, ty.clone())),
                ty: ty.clone(),
            }),
            ty,
        };
    });
    match outcome {
        VerifyOutcome::Diverged(divergences) => assert!(
            divergences[0]
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("'ghost'")),
            "{divergences:?}"
        ),
        other => panic!("expected a difference, got {other:?}"),
    }
}
