//! Calls on both sides of a ground-truth check, on the fixture checked into
//! this crate (`testdata/ground_truth_fixture.c`).
//!
//! A stubbed call does what any callee may: it rewrites the image's writable
//! data and the memory it was handed, and leaves anything in the registers
//! the convention lets it kill. The same stub runs on both sides, so a
//! decompiled body that is right about the call agrees, and one that keeps a
//! value across the call the machine reloads does not.

use fission_dir::report::VerifyOutcome;
use fission_dir::{
    CallStub, EmulatorHarness, Grounding, InitialState, RunPlan, StubbedEnd, call_sites,
    check_ground_truth_with, decompile_one,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_midend_core::ir::{HirExpr, HirFunction, HirLValue, HirStmt};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

const RUNS: usize = 64;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ground_truth_fixture.elf")
}

fn address_of(binary: &LoadedBinary, name: &str) -> u64 {
    let inner = binary.inner();
    let index = *inner
        .function_name_index
        .get(name)
        .unwrap_or_else(|| panic!("{name} in the fixture's symbols"));
    inner.functions[index].address
}

/// Check `name`, with its decompiled tree first passed through `edit`.
fn check_edited(name: &str, edit: impl FnOnce(&mut HirFunction)) -> VerifyOutcome {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let address = address_of(&binary, name);
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

/// `before = g_value; note(before); return before * 3 + g_value;` -- the
/// stub rewrites `g_value` at the call, and the decompiled body, which reads
/// it again after the call as the machine does, agrees on every run.
#[test]
fn a_global_read_on_both_sides_of_a_call_agrees() {
    match check_edited("reload_after_call", |_| {}) {
        VerifyOutcome::Equivalent { checked } => assert_eq!(checked, RUNS),
        other => panic!("reload_after_call: expected agreement, got {other:?}"),
    }
}

/// Replace every read of `from` in `expr` with a read of `to`.
fn rename_reads(expr: &mut HirExpr, from: &str, to: &str) {
    match expr {
        HirExpr::Var(name) if name == from => *name = to.to_string(),
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => rename_reads(expr, from, to),
        HirExpr::Binary { lhs, rhs, .. } => {
            rename_reads(lhs, from, to);
            rename_reads(rhs, from, to);
        }
        HirExpr::Call { args, .. } => args.iter_mut().for_each(|a| rename_reads(a, from, to)),
        _ => {}
    }
}

/// The same body with the copy taken before the call carried past it: the
/// read of `g_value` that saved it moves to after the call, which is what a
/// copy propagation that ignores the call would print. The stub's write to
/// `g_value` makes that differ from the machine, which used the old value.
/// (With a stub that writes nothing, it would agree: the defect would hide.)
#[test]
fn a_copy_carried_across_a_call_differs() {
    let outcome = check_edited("reload_after_call", |hir| {
        let copy = hir
            .body
            .iter()
            .position(|stmt| {
                matches!(stmt, HirStmt::Assign { lhs: HirLValue::Var(_), rhs: HirExpr::Var(g) } if g == "g_value")
            })
            .expect("the copy of g_value taken before the call");
        let HirStmt::Assign {
            lhs: HirLValue::Var(saved),
            ..
        } = hir.body.remove(copy)
        else {
            unreachable!()
        };
        for stmt in hir.body.iter_mut() {
            match stmt {
                HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                    rename_reads(expr, &saved, "g_value")
                }
                HirStmt::Assign { rhs, .. } => rename_reads(rhs, &saved, "g_value"),
                _ => {}
            }
        }
    });
    assert!(
        matches!(outcome, VerifyOutcome::Diverged(_)),
        "expected a difference, got {outcome:?}"
    );
}

/// A stubbed call leaves the stub's word in every register the convention
/// lets a callee kill, and its own result in the return register.
/// `call_then_return` touches none of them after the call, so they reach
/// the function's return as the stub left them.
#[test]
fn a_stubbed_call_overwrites_the_registers_a_call_kills() {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let address = address_of(&binary, "call_then_return");
    let flow = call_sites(&binary, address).expect("call sites");
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("harness");
    let state = InitialState::new(fission_dir::DEFAULT_SEED, 40);
    harness.begin_run(address, &[7], &state).expect("set up");
    let result = |_: usize| 0x5151;
    let clobber = |n: usize, index: usize| 0x1000 + 0x10 * n as u64 + index as u64;
    let writes = |_: usize, _: &[u64]| Vec::new();
    let run = harness
        .run_prepared(
            &flow,
            &CallStub {
                result: &result,
                clobber: Some(&clobber),
                writes: &writes,
            },
        )
        .expect("run");
    assert!(matches!(run.end, StubbedEnd::Returned(7)), "{:?}", run.end);
    // SysV x86-64: rax rcx rdx rsi rdi r8 r9 r10 r11; rax is the result.
    let killed = ["rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"];
    let names: Vec<String> = killed.iter().map(|r| r.to_string()).collect();
    for (offset, (name, value)) in harness.registers(&names).into_iter().enumerate() {
        assert_eq!(
            value as u64,
            0x1000 + offset as u64 + 1,
            "{name} after the call"
        );
    }
}
