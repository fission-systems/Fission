//! Calibration suite verifying that comparison boundaries catch injected
//! faults, keep shared-oracle blind spots explicit, and never coerce missing
//! or errored observations into `Equivalent` (#1254).

use fission_dir::diff::{ConcreteDiffStatus, SampleErrorKind};
use fission_dir::eval::{World, run_hir};
use fission_dir::{UnsupportedReason, VerifyOutcome, diff_prehir_hir_detailed, interpret_hir};
use fission_midend_core::ir::{
    CallOrigin, HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, NirBinding,
    NirBindingOrigin, NirType,
};
use fission_midend_prehir::{
    PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
};
use std::collections::BTreeMap;

fn i32_ty() -> NirType {
    NirType::Int {
        bits: 32,
        signed: true,
    }
}

fn ptr_ty() -> NirType {
    NirType::Ptr(Box::new(i32_ty()))
}

fn pre_param(name: &str, idx: usize, ty: NirType) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(idx)),
        initializer: None,
    }
}

fn hir_param(name: &str, idx: usize, ty: NirType) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(idx)),
        initializer: None,
    }
}

fn pre_bin(op: PreHirBinaryOp, a: &str, b: &str) -> PreHirFunction {
    PreHirFunction {
        params: vec![pre_param(a, 0, i32_ty()), pre_param(b, 1, i32_ty())],
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op,
            lhs: Box::new(PreHirExpr::Var(a.to_string())),
            rhs: Box::new(PreHirExpr::Var(b.to_string())),
            ty: i32_ty(),
        }))],
        ..Default::default()
    }
}

fn hir_bin(op: HirBinaryOp, a: &str, b: &str) -> HirFunction {
    HirFunction {
        params: vec![hir_param(a, 0, i32_ty()), hir_param(b, 1, i32_ty())],
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op,
            lhs: Box::new(HirExpr::Var(a.to_string())),
            rhs: Box::new(HirExpr::Var(b.to_string())),
            ty: i32_ty(),
        }))],
        ..Default::default()
    }
}

#[test]
fn calibration_positive_control_and_shared_wrong_lift_blind_spot() {
    let samples = vec![vec![10, 3], vec![0, 5], vec![-4, 7]];

    // 1. Positive control: matching add produces sampled agreement, never a universal proof.
    let ok = diff_prehir_hir_detailed(
        &pre_bin(PreHirBinaryOp::Add, "a", "b"),
        &hir_bin(HirBinaryOp::Add, "a", "b"),
        &samples,
    );
    assert_eq!(ok.status, ConcreteDiffStatus::CompleteSampledAgreement);
    assert!(ok.counts.reconciles());
    assert!(!ok.is_universally_proven());
    assert_eq!(ok.outcome, VerifyOutcome::Equivalent { checked: 3 });

    // 2. Shared wrong lift (common-mode fault): BOTH PreHIR and HIR implement `a - b`
    //    when the true native specification is `a + b`. PreHIR-vs-HIR alone agrees
    //    (demonstrating its shared-lifter blind spot), whereas checking against an
    //    independent reference oracle `a + b` catches the divergence on every sample
    //    where `b != 0`.
    let wrong_pre = pre_bin(PreHirBinaryOp::Sub, "a", "b");
    let wrong_hir = hir_bin(HirBinaryOp::Sub, "a", "b");
    let self_report = diff_prehir_hir_detailed(&wrong_pre, &wrong_hir, &samples);
    assert_eq!(
        self_report.status,
        ConcreteDiffStatus::CompleteSampledAgreement,
        "shared PreHIR+HIR fault illustrates why same-pipeline diff cannot detect common-mode errors"
    );

    let mut caught_against_reference = 0usize;
    for args in &samples {
        let observed = interpret_hir(&wrong_hir.body, &wrong_hir.params, &wrong_hir.locals, args)
            .expect("interpret wrong_hir");
        let native_reference = Some(i64::from((args[0] as i32).wrapping_add(args[1] as i32)));
        if observed != native_reference {
            caught_against_reference += 1;
        }
    }
    assert_eq!(caught_against_reference, 3);
}

#[test]
fn calibration_hir_value_store_and_call_corruption_are_detected() {
    let samples = vec![vec![10, 3], vec![-1, 4]];

    // 1. HIR-only value corruption.
    let pre = pre_bin(PreHirBinaryOp::Add, "a", "b");
    let corrupted_val_hir = hir_bin(HirBinaryOp::Xor, "a", "b");
    let val_report = diff_prehir_hir_detailed(&pre, &corrupted_val_hir, &samples);
    assert_eq!(val_report.status, ConcreteDiffStatus::Diverged);
    assert!(val_report.counts.reconciles());
    assert!(!val_report.outcome.is_equivalent());
    match val_report.outcome {
        VerifyOutcome::Diverged(divs) => assert_eq!(divs.len(), 2),
        other => panic!("expected Diverged, got {other:?}"),
    }

    // 2. HIR-only store corruption (wrong byte or wrong target address).
    let params = vec![hir_param("buf", 0, ptr_ty()), hir_param("val", 1, i32_ty())];
    let clean_store_body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("buf".into())),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            },
            rhs: HirExpr::Var("val".into()),
        },
        HirStmt::Return(Some(HirExpr::Const(0, i32_ty()))),
    ];
    let corrupt_store_body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("buf".into())),
                    offset: 4, // out-of-bounds offset corruption
                }),
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
            },
            rhs: HirExpr::Var("val".into()),
        },
        HirStmt::Return(Some(HirExpr::Const(0, i32_ty()))),
    ];
    let image = [(0x2000u64, 0u8), (0x2001, 0), (0x2004, 0)];
    let world = World {
        image: &image,
        ..World::default()
    };
    let clean_run = run_hir(&clean_store_body, &params, &[], &[0x2000, 0x5a], &world).unwrap();
    let corrupt_run = run_hir(&corrupt_store_body, &params, &[], &[0x2000, 0x5a], &world).unwrap();
    assert_eq!(clean_run.writes, BTreeMap::from([(0x2000, 0x5au8)]));
    assert_ne!(
        corrupt_run.writes, clean_run.writes,
        "store corruption must be observable in writes map even when return value agrees"
    );

    // 3. HIR-only call argument corruption.
    let call_params = vec![hir_param("x", 0, i32_ty())];
    let clean_call_body = vec![
        HirStmt::Expr(HirExpr::Call {
            origin: Some(CallOrigin {
                instruction_address: 0x1000,
                pcode_sequence: 0,
            }),
            target: "sink".into(),
            args: vec![HirExpr::Var("x".into())],
            ty: i32_ty(),
        }),
        HirStmt::Return(Some(HirExpr::Const(1, i32_ty()))),
    ];
    let corrupt_call_body = vec![
        HirStmt::Expr(HirExpr::Call {
            origin: Some(CallOrigin {
                instruction_address: 0x1000,
                pcode_sequence: 0,
            }),
            target: "sink".into(),
            args: vec![HirExpr::Const(999, i32_ty())],
            ty: i32_ty(),
        }),
        HirStmt::Return(Some(HirExpr::Const(1, i32_ty()))),
    ];
    let call_stub = |_idx: usize| 0u64;
    let call_world = World {
        call_return: Some(&call_stub),
        ..World::default()
    };
    let clean_call_run = run_hir(&clean_call_body, &call_params, &[], &[7], &call_world).unwrap();
    let corrupt_call_run =
        run_hir(&corrupt_call_body, &call_params, &[], &[7], &call_world).unwrap();
    assert_ne!(
        clean_call_run.calls, corrupt_call_run.calls,
        "call argument corruption must be observable even when return value agrees"
    );
}

#[test]
fn calibration_missing_truncated_and_asymmetric_faults_never_pass() {
    // 1. Missing observations (empty sample set or all-error divisor).
    let pre = pre_bin(PreHirBinaryOp::Add, "a", "b");
    let hir = hir_bin(HirBinaryOp::Add, "a", "b");
    let empty_report = diff_prehir_hir_detailed(&pre, &hir, &[]);
    assert_eq!(
        empty_report.status,
        ConcreteDiffStatus::NoComparableEvidence
    );
    assert!(!empty_report.outcome.is_equivalent());
    assert_eq!(empty_report.outcome.category(), "unsupported");

    // 2. Truncated trace (asymmetric fuel exhaustion on HIR loop vs terminating PreHIR).
    let terminating_pre = PreHirFunction {
        params: vec![pre_param("x", 0, i32_ty())],
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".into())))],
        ..Default::default()
    };
    let infinite_loop_hir = HirFunction {
        params: vec![hir_param("x", 0, i32_ty())],
        body: vec![
            HirStmt::While {
                cond: HirExpr::Const(1, NirType::Bool),
                body: vec![],
            },
            HirStmt::Return(Some(HirExpr::Var("x".into()))),
        ],
        ..Default::default()
    };
    let trunc_report = diff_prehir_hir_detailed(&terminating_pre, &infinite_loop_hir, &[vec![5]]);
    assert_eq!(trunc_report.status, ConcreteDiffStatus::Diverged);
    assert!(trunc_report.counts.reconciles());
    assert_eq!(trunc_report.counts.asymmetric_errors, 1);
    assert_eq!(
        trunc_report
            .counts
            .asymmetric_error_reasons
            .get(&SampleErrorKind::FuelExhausted),
        Some(&1)
    );
    assert!(!trunc_report.outcome.is_equivalent());

    // Symmetric fuel exhaustion -> NoComparableEvidence / unsupported, NEVER Equivalent.
    let infinite_loop_pre = PreHirFunction {
        params: vec![pre_param("x", 0, i32_ty())],
        body: vec![
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![].into(),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("x".into()))),
        ],
        ..Default::default()
    };
    let sym_fuel = diff_prehir_hir_detailed(&infinite_loop_pre, &infinite_loop_hir, &[vec![5]]);
    assert_eq!(sym_fuel.status, ConcreteDiffStatus::NoComparableEvidence);
    assert!(!sym_fuel.outcome.is_equivalent());

    // 3. Asymmetric unsupported construct/call (no stub in default World).
    let unmodeled_call_hir = HirFunction {
        params: vec![hir_param("x", 0, i32_ty())],
        body: vec![HirStmt::Return(Some(HirExpr::Call {
            origin: None,
            target: "unmodeled_intrinsic".into(),
            args: vec![HirExpr::Var("x".into())],
            ty: i32_ty(),
        }))],
        ..Default::default()
    };
    let asym_unsup = diff_prehir_hir_detailed(&terminating_pre, &unmodeled_call_hir, &[vec![1]]);
    assert_eq!(asym_unsup.status, ConcreteDiffStatus::Diverged);
    assert_eq!(
        asym_unsup
            .counts
            .asymmetric_error_reasons
            .get(&SampleErrorKind::UnmodeledCallOrIntrinsic),
        Some(&1)
    );
    assert!(!asym_unsup.outcome.is_equivalent());

    // 4. Exhaustive check that non-passing VerifyOutcome variants never satisfy is_equivalent().
    let non_passing = [
        VerifyOutcome::Diverged(vec![]),
        VerifyOutcome::Suspect(vec![]),
        VerifyOutcome::Unsupported(UnsupportedReason::Construct("x")),
        VerifyOutcome::Unsupported(UnsupportedReason::UnsupportedType("f64")),
        VerifyOutcome::Unsupported(UnsupportedReason::ContainsLoop),
        VerifyOutcome::Unsupported(UnsupportedReason::EmulatorSetup("fail".into())),
        VerifyOutcome::Unsupported(UnsupportedReason::Interpreter("fail".into())),
        VerifyOutcome::Unsupported(UnsupportedReason::Harness("native fail".into())),
        VerifyOutcome::Inconclusive {
            reason: "stale witness".into(),
        },
    ];
    for outcome in non_passing {
        assert!(!outcome.is_equivalent(), "{outcome:?}");
        assert_ne!(outcome.category(), "equivalent", "{outcome:?}");
    }
}
