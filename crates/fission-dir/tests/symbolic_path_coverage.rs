use std::collections::HashMap;

use fission_dir::eval::{interpret_hir, interpret_prehir};
use fission_dir::lower_sym::{
    self, LoweredFunctionPaths, PATH_EXPLORE_BUDGET, SymbolicExitKind, SymbolicPathLeaf,
};
use fission_dir::product::{
    AssuranceBand, EvidenceStrength, FoundationIdentity, UnresolvedEffectKind, ValidationVerdict,
};
use fission_dir::report::UnsupportedReason;
use fission_dir::symbolic::{
    DomainNonVacuityStatus, ObligationStatus, OverlapPolicy, SymbolicDomainAssumption,
    SymbolicOutcome, SymbolicVerificationOptions, check_lowered_paths_equivalence,
    check_symbolic_equivalence_report, check_symbolic_equivalence_with_domain,
    check_symbolic_equivalence_with_options,
};
use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirFunction, HirStmt, NirBinding, NirBindingOrigin, NirType,
};
use fission_midend_prehir::{
    PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
};
use fission_solver::ast::{SymExpr, SymNodeId};

fn u4_ty() -> NirType {
    NirType::Int {
        bits: 4,
        signed: false,
    }
}

fn pre_param(name: &str, ty: NirType, slot: usize) -> PreHirBinding {
    PreHirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(slot)),
        initializer: None,
    }
}

fn hir_param(name: &str, ty: NirType, slot: usize) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(slot)),
        initializer: None,
    }
}

fn pre_ult(lhs: PreHirExpr, rhs: PreHirExpr) -> PreHirExpr {
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Lt,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn hir_ult(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::Binary {
        op: HirBinaryOp::Lt,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

fn bv_mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else if bits == 0 {
        0
    } else {
        (1u64 << bits) - 1
    }
}

/// Independent small-width bitvector evaluator for cross-checking lowered path
/// predicates exhaustively over 4-bit inputs.
fn eval_pred_exhaustive(expr: &SymExpr, env: &HashMap<SymNodeId, u64>) -> u64 {
    let w = expr.get_bit_width();
    let val = match expr {
        SymExpr::Const { val, size } => val & bv_mask(*size),
        SymExpr::Var { id, .. } => env[id] & bv_mask(w),
        SymExpr::Eq(a, b) => {
            u64::from(eval_pred_exhaustive(a, env) == eval_pred_exhaustive(b, env))
        }
        SymExpr::Neq(a, b) => {
            u64::from(eval_pred_exhaustive(a, env) != eval_pred_exhaustive(b, env))
        }
        SymExpr::Ult(a, b) => {
            u64::from(eval_pred_exhaustive(a, env) < eval_pred_exhaustive(b, env))
        }
        SymExpr::Ule(a, b) => {
            u64::from(eval_pred_exhaustive(a, env) <= eval_pred_exhaustive(b, env))
        }
        SymExpr::And(a, b) => eval_pred_exhaustive(a, env) & eval_pred_exhaustive(b, env),
        SymExpr::Or(a, b) => eval_pred_exhaustive(a, env) | eval_pred_exhaustive(b, env),
        SymExpr::Xor(a, b) => eval_pred_exhaustive(a, env) ^ eval_pred_exhaustive(b, env),
        SymExpr::Ite { cond, t, f } => {
            if eval_pred_exhaustive(cond, env) != 0 {
                eval_pred_exhaustive(t, env)
            } else {
                eval_pred_exhaustive(f, env)
            }
        }
        other => panic!("unexpected expr in small-width evaluator: {other:?}"),
    };
    val & bv_mask(w)
}

#[test]
fn valid_acyclic_diamonds_and_backward_goto_joins_discharge_obligations() {
    let u4 = u4_ty();

    // PreHIR uses an acyclic goto join where control jumps forward to `dispatch`,
    // then jumps backward once to `finish` without revisiting any label.
    let pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![
            PreHirStmt::Goto("dispatch".to_string()),
            PreHirStmt::Label("finish_low".to_string()),
            PreHirStmt::Return(Some(PreHirExpr::Const(1, u4.clone()))),
            PreHirStmt::Label("dispatch".to_string()),
            PreHirStmt::If {
                cond: pre_ult(
                    PreHirExpr::Var("x".to_string()),
                    PreHirExpr::Const(5, u4.clone()),
                ),
                then_body: vec![PreHirStmt::Goto("finish_low".to_string())].into(),
                else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(2, u4.clone())))].into(),
            },
        ],
        ..Default::default()
    };

    // HIR uses nested if/else diamond.
    let hir = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4.clone(), 0)],
        body: vec![HirStmt::If {
            cond: hir_ult(HirExpr::Var("x".to_string()), HirExpr::Const(5, u4.clone())),
            then_body: vec![HirStmt::Return(Some(HirExpr::Const(1, u4.clone())))].into(),
            else_body: vec![HirStmt::If {
                cond: hir_ult(
                    HirExpr::Var("x".to_string()),
                    HirExpr::Const(10, u4.clone()),
                ),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(2, u4.clone())))].into(),
                else_body: vec![HirStmt::Return(Some(HirExpr::Const(2, u4.clone())))].into(),
            }]
            .into(),
        }],
        ..Default::default()
    };

    let report = check_symbolic_equivalence_report(&pre, &hir);
    assert_eq!(report.outcome, SymbolicOutcome::Equivalent);
    assert_eq!(report.verdict, ValidationVerdict::Equivalent);
    assert_eq!(report.band(), AssuranceBand::Proven);
    assert_eq!(report.evidence.strength, EvidenceStrength::Universal);
    assert_eq!(report.evidence.explored_paths, 5);
    assert_eq!(
        report.evidence.budget.max_paths,
        Some(PATH_EXPLORE_BUDGET as u64)
    );

    let oblig = report.path_obligations.as_ref().expect("path obligations");
    assert!(oblig.all_required_discharged);
    assert_eq!(
        oblig.domain_non_vacuity,
        ObligationStatus::DischargedByConstruction
    );
    assert_eq!(
        oblig.domain_kind,
        DomainNonVacuityStatus::UnconstrainedNonEmpty
    );
    assert_eq!(oblig.prehir.return_leaf_count, 2);
    assert_eq!(oblig.hir.return_leaf_count, 3);
    assert_eq!(oblig.prehir.path_coverage, ObligationStatus::Discharged);
    assert_eq!(oblig.prehir.path_exclusivity, ObligationStatus::Discharged);
    assert_eq!(oblig.hir.path_coverage, ObligationStatus::Discharged);
    assert_eq!(oblig.hir.path_exclusivity, ObligationStatus::Discharged);

    let foundation = FoundationIdentity {
        model_version: 1,
        function_address: 0x1000,
        block_count: 3,
        op_count: 6,
        digest: 0x1234,
    };
    let assurance = report
        .to_assurance_report(foundation)
        .expect("AssuranceReport contract holds");
    assert!(assurance.is_unconditionally_proven());

    // Exhaustively cross-check concrete 4-bit interpreter on all 16 inputs.
    for x in 0..16_i64 {
        let p = interpret_prehir(&pre.body, &pre.params, &pre.locals, &[x])
            .expect("pre eval")
            .expect("pre return");
        let h = interpret_hir(&hir.body, &hir.params, &hir.locals, &[x])
            .expect("hir eval")
            .expect("hir return");
        assert_eq!(p, h, "concrete 4-bit divergence at x={x}");
    }
}

#[test]
fn missing_leaf_and_mutated_path_conditions_are_rejected() {
    let u4 = u4_ty();
    let func_pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::If {
            cond: pre_ult(
                PreHirExpr::Var("x".to_string()),
                PreHirExpr::Const(5, u4.clone()),
            ),
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(10, u4.clone())))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(10, u4.clone())))].into(),
        }],
        ..Default::default()
    };
    let func_hir = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4.clone(), 0)],
        body: vec![HirStmt::If {
            cond: hir_ult(HirExpr::Var("x".to_string()), HirExpr::Const(5, u4.clone())),
            then_body: vec![HirStmt::Return(Some(HirExpr::Const(10, u4.clone())))].into(),
            else_body: vec![HirStmt::Return(Some(HirExpr::Const(10, u4.clone())))].into(),
        }],
        ..Default::default()
    };

    let x_var = SymExpr::new_var("x", 4);
    let SymExpr::Var { id: x_id, .. } = x_var else {
        unreachable!();
    };
    let pre_paths = lower_sym::prehir::lower_paths(
        &func_pre.body,
        &func_pre.params,
        &func_pre.locals,
        std::slice::from_ref(&x_var),
    )
    .expect("lower pre paths");
    let hir_paths = lower_sym::hir::lower_paths(
        &func_hir.body,
        &func_hir.params,
        &func_hir.locals,
        std::slice::from_ref(&x_var),
    )
    .expect("lower hir paths");

    // 1. Drop the second leaf (`!(x < 5)`): missing partition on 5..16.
    let mut dropped = pre_paths.clone();
    dropped.leaves.pop();
    let uncovered_report = check_lowered_paths_equivalence(
        &func_pre,
        &func_hir,
        &dropped,
        &hir_paths,
        std::slice::from_ref(&x_var),
        SymbolicVerificationOptions::default(),
        None,
    );
    assert_eq!(uncovered_report.verdict, ValidationVerdict::Inconclusive);
    let oblig = uncovered_report.path_obligations.expect("obligations");
    assert!(!oblig.all_required_discharged);
    assert!(matches!(
        oblig.prehir.path_coverage,
        ObligationStatus::Failed { .. }
    ));

    // Confirm via independent exhaustive 4-bit evaluation that at least one input is uncovered.
    let uncovered_inputs: Vec<u64> = (0..16_u64)
        .filter(|&x| {
            let env = HashMap::from([(x_id, x)]);
            dropped
                .leaves
                .iter()
                .all(|leaf| eval_pred_exhaustive(&leaf.path_cond, &env) == 0)
        })
        .collect();
    assert_eq!(uncovered_inputs, (5..16_u64).collect::<Vec<_>>());

    // 2. Mutate leaf 0's path condition from `(x < 5)` to `(x < 4)` leaving `x == 4` uncovered.
    let mut gap_mutated = pre_paths.clone();
    gap_mutated.leaves[0].path_cond = SymExpr::new_ult(x_var.clone(), SymExpr::new_const(4, 4));
    let gap_report = check_lowered_paths_equivalence(
        &func_pre,
        &func_hir,
        &gap_mutated,
        &hir_paths,
        std::slice::from_ref(&x_var),
        SymbolicVerificationOptions::default(),
        None,
    );
    assert_eq!(gap_report.verdict, ValidationVerdict::Inconclusive);
    assert!(matches!(
        gap_report
            .path_obligations
            .expect("oblig")
            .prehir
            .path_coverage,
        ObligationStatus::Failed { .. }
    ));

    // 3. Mutate leaf 0's path condition to `(x < 6)` so `x == 5` matches both leaves (overlap).
    let mut overlap_mutated = pre_paths;
    overlap_mutated.leaves[0].path_cond = SymExpr::new_ult(x_var.clone(), SymExpr::new_const(6, 4));
    let overlap_report = check_lowered_paths_equivalence(
        &func_pre,
        &func_hir,
        &overlap_mutated,
        &hir_paths,
        std::slice::from_ref(&x_var),
        SymbolicVerificationOptions::default(),
        None,
    );
    assert_eq!(overlap_report.verdict, ValidationVerdict::Inconclusive);
    let overlap_oblig = overlap_report.path_obligations.expect("oblig");
    assert_eq!(
        overlap_oblig.prehir.path_coverage,
        ObligationStatus::Discharged
    );
    assert!(matches!(
        overlap_oblig.prehir.path_exclusivity,
        ObligationStatus::Failed { .. }
    ));
}

#[test]
fn overlapping_leaves_enforce_exclusivity_and_consistency_policies() {
    let u4 = u4_ty();
    let pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))],
        ..Default::default()
    };
    let hir = HirFunction {
        return_type: u4,
        params: vec![hir_param("x", u4_ty(), 0)],
        body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
        ..Default::default()
    };

    let x = SymExpr::new_var("x", 4);
    let lt_8 = SymExpr::new_ult(x.clone(), SymExpr::new_const(8, 4));
    let ge_4 = SymExpr::new_not(SymExpr::new_ult(x.clone(), SymExpr::new_const(4, 4)));

    // Overlapping predicates (`4..8`) returning consistent value `x`.
    let consistent_overlap = LoweredFunctionPaths {
        leaves: vec![
            SymbolicPathLeaf {
                path_cond: lt_8.clone(),
                exit: SymbolicExitKind::ReturnValue(x.clone()),
            },
            SymbolicPathLeaf {
                path_cond: ge_4.clone(),
                exit: SymbolicExitKind::ReturnValue(x.clone()),
            },
        ],
        explored_steps: 2,
        budget_limit: PATH_EXPLORE_BUDGET,
        return_bits: Some(4),
    };
    let single_leaf = LoweredFunctionPaths {
        leaves: vec![SymbolicPathLeaf {
            path_cond: SymExpr::Const { val: 1, size: 1 },
            exit: SymbolicExitKind::ReturnValue(x.clone()),
        }],
        explored_steps: 1,
        budget_limit: PATH_EXPLORE_BUDGET,
        return_bits: Some(4),
    };

    let strict_rep = check_lowered_paths_equivalence(
        &pre,
        &hir,
        &consistent_overlap,
        &single_leaf,
        std::slice::from_ref(&x),
        SymbolicVerificationOptions::default(),
        None,
    );
    assert_eq!(strict_rep.verdict, ValidationVerdict::Inconclusive);
    let strict_oblig = strict_rep.path_obligations.expect("oblig");
    assert!(matches!(
        strict_oblig.prehir.path_exclusivity,
        ObligationStatus::Failed { .. }
    ));
    assert_eq!(
        strict_oblig.prehir.overlap_value_consistency,
        Some(ObligationStatus::Discharged)
    );

    let tolerant_rep = check_lowered_paths_equivalence(
        &pre,
        &hir,
        &consistent_overlap,
        &single_leaf,
        std::slice::from_ref(&x),
        SymbolicVerificationOptions {
            overlap_policy: OverlapPolicy::AllowConsistentOverlap,
            ..SymbolicVerificationOptions::default()
        },
        None,
    );
    assert_eq!(tolerant_rep.outcome, SymbolicOutcome::Equivalent);
    assert_eq!(tolerant_rep.band(), AssuranceBand::ConditionallyProven);
    assert!(!tolerant_rep.is_unconditionally_proven());

    // Conflicting overlap: leaf 1 returns `x ^ 1` when `4 <= x < 8`.
    let conflicting_overlap = LoweredFunctionPaths {
        leaves: vec![
            SymbolicPathLeaf {
                path_cond: lt_8,
                exit: SymbolicExitKind::ReturnValue(x.clone()),
            },
            SymbolicPathLeaf {
                path_cond: ge_4,
                exit: SymbolicExitKind::ReturnValue(SymExpr::new_xor(
                    x.clone(),
                    SymExpr::new_const(1, 4),
                )),
            },
        ],
        explored_steps: 2,
        budget_limit: PATH_EXPLORE_BUDGET,
        return_bits: Some(4),
    };

    for policy in [
        OverlapPolicy::RequireExclusive,
        OverlapPolicy::AllowConsistentOverlap,
    ] {
        let rep = check_lowered_paths_equivalence(
            &pre,
            &hir,
            &conflicting_overlap,
            &single_leaf,
            std::slice::from_ref(&x),
            SymbolicVerificationOptions {
                overlap_policy: policy,
                ..SymbolicVerificationOptions::default()
            },
            None,
        );
        assert_eq!(rep.verdict, ValidationVerdict::Inconclusive);
        let oblig = rep.path_obligations.expect("oblig");
        assert!(matches!(
            oblig.prehir.overlap_value_consistency,
            Some(ObligationStatus::Failed { .. })
        ));
    }
}

#[test]
fn contradictory_precondition_is_rejected_while_satisfiable_domain_yields_conditional_proof() {
    let u4 = u4_ty();
    // Diverge on `x >= 8`, agree on `x < 8`.
    let pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::If {
            cond: pre_ult(
                PreHirExpr::Var("x".to_string()),
                PreHirExpr::Const(8, u4.clone()),
            ),
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(1, u4.clone())),
                ty: u4.clone(),
            }))]
            .into(),
        }],
        ..Default::default()
    };
    let hir = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4.clone(), 0)],
        body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
        ..Default::default()
    };

    // Unconstrained must find a divergence for x >= 8.
    let uncond = check_symbolic_equivalence_report(&pre, &hir);
    let SymbolicOutcome::Diverged(cx) = &uncond.outcome else {
        panic!(
            "expected unconstrained divergence, got {:?}",
            uncond.outcome
        );
    };
    assert!(cx.args[0] >= 8);

    // Contradictory precondition: (x < 3) AND (x > 5) -- vacuous!
    let vacuous = check_symbolic_equivalence_with_domain(
        &pre,
        &hir,
        SymbolicVerificationOptions::default(),
        |vars| {
            let x = &vars[0];
            let lt3 = SymExpr::new_ult(x.clone(), SymExpr::new_const(3, 4));
            let gt5 = SymExpr::new_ult(SymExpr::new_const(5, 4), x.clone());
            Some(SymbolicDomainAssumption {
                predicate: SymExpr::Ite {
                    cond: Box::new(lt3),
                    t: Box::new(gt5),
                    f: Box::new(SymExpr::Const { val: 0, size: 1 }),
                },
                description: "x < 3 && x > 5".to_string(),
            })
        },
    );
    assert_eq!(vacuous.verdict, ValidationVerdict::Inconclusive);
    let vac_oblig = vacuous.path_obligations.expect("obligations");
    assert!(!vac_oblig.all_required_discharged);
    assert!(matches!(
        vac_oblig.domain_non_vacuity,
        ObligationStatus::Failed { .. }
    ));
    assert!(matches!(
        vac_oblig.domain_kind,
        DomainNonVacuityStatus::VacuousContradictory { .. }
    ));

    // Precondition referencing an unbound variable must also be rejected.
    let foreign = check_symbolic_equivalence_with_domain(
        &pre,
        &hir,
        SymbolicVerificationOptions::default(),
        |_| {
            Some(SymbolicDomainAssumption {
                predicate: SymExpr::new_eq(
                    SymExpr::new_var("unbound", 4),
                    SymExpr::new_const(1, 4),
                ),
                description: "foreign variable".to_string(),
            })
        },
    );
    assert_eq!(foreign.verdict, ValidationVerdict::Inconclusive);

    // Satisfiable precondition: x < 8 => conditionally proven!
    let cond_eq = check_symbolic_equivalence_with_domain(
        &pre,
        &hir,
        SymbolicVerificationOptions::default(),
        |vars| {
            Some(SymbolicDomainAssumption {
                predicate: SymExpr::new_ult(vars[0].clone(), SymExpr::new_const(8, 4)),
                description: "x < 8".to_string(),
            })
        },
    );
    assert_eq!(cond_eq.outcome, SymbolicOutcome::Equivalent);
    assert_eq!(cond_eq.band(), AssuranceBand::ConditionallyProven);
    assert!(cond_eq.is_proven());
    assert!(!cond_eq.is_unconditionally_proven());
    let cond_oblig = cond_eq.path_obligations.expect("oblig");
    assert_eq!(cond_oblig.domain_non_vacuity, ObligationStatus::Discharged);
    assert!(matches!(
        cond_oblig.domain_kind,
        DomainNonVacuityStatus::ConstrainedSatisfiable { .. }
    ));

    // Satisfiable precondition (x < 12) where functions still diverge (on 8..12) returns in-domain witness.
    let cond_div = check_symbolic_equivalence_with_domain(
        &pre,
        &hir,
        SymbolicVerificationOptions::default(),
        |vars| {
            Some(SymbolicDomainAssumption {
                predicate: SymExpr::new_ult(vars[0].clone(), SymExpr::new_const(12, 4)),
                description: "x < 12".to_string(),
            })
        },
    );
    let SymbolicOutcome::Diverged(cond_cx) = &cond_div.outcome else {
        panic!(
            "expected constrained divergence, got {:?}",
            cond_div.outcome
        );
    };
    assert!((8..12).contains(&cond_cx.args[0]));
}

#[test]
fn unreachable_branch_is_explicitly_reported_without_breaking_coverage() {
    let u4 = u4_ty();
    let pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::Ne,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Var("x".to_string())),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(7, u4.clone())))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))].into(),
        }],
        ..Default::default()
    };
    let hir = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4, 0)],
        body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
        ..Default::default()
    };

    let default_rep = check_symbolic_equivalence_report(&pre, &hir);
    assert_eq!(default_rep.outcome, SymbolicOutcome::Equivalent);
    let default_oblig = default_rep.path_obligations.expect("oblig");
    assert_eq!(default_oblig.prehir.leaf_reachability, None);

    let opt_in_rep = check_symbolic_equivalence_with_options(
        &pre,
        &hir,
        SymbolicVerificationOptions {
            check_leaf_reachability: true,
            ..SymbolicVerificationOptions::default()
        },
    );
    assert_eq!(opt_in_rep.outcome, SymbolicOutcome::Equivalent);
    let opt_in_oblig = opt_in_rep.path_obligations.expect("oblig");
    assert_eq!(opt_in_oblig.prehir.reachable_leaf_count, Some(1));
    assert_eq!(opt_in_oblig.prehir.unreachable_leaf_count, Some(1));
    assert_eq!(
        opt_in_oblig.prehir.leaf_reachability,
        Some(ObligationStatus::Discharged)
    );
}

#[test]
fn cyclic_gotos_and_incomplete_or_nonreturn_exits_are_refused() {
    let u4 = u4_ty();
    let trivial_hir = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4.clone(), 0)],
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, u4.clone())))],
        ..Default::default()
    };

    // 1. Direct goto self-loop without any While/DoWhile/For node.
    let self_loop = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![
            PreHirStmt::Label("loop".to_string()),
            PreHirStmt::Goto("loop".to_string()),
        ],
        ..Default::default()
    };
    let self_rep = check_symbolic_equivalence_report(&self_loop, &trivial_hir);
    assert_eq!(
        self_rep.outcome,
        SymbolicOutcome::Unsupported(UnsupportedReason::ContainsLoop)
    );
    let self_oblig = self_rep.path_obligations.expect("obligations");
    assert_eq!(self_oblig.prehir.cyclic_goto_count, 1);
    assert!(self_oblig.prehir.explored_steps < PATH_EXPLORE_BUDGET);

    // 2. Mutual two-label goto cycle (`goto b; a: goto b; b: goto a;`) guarded conditionally.
    let mutual_cycle = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![
            PreHirStmt::If {
                cond: pre_ult(
                    PreHirExpr::Var("x".to_string()),
                    PreHirExpr::Const(2, u4.clone()),
                ),
                then_body: vec![PreHirStmt::Goto("a".to_string())].into(),
                else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, u4.clone())))].into(),
            },
            PreHirStmt::Label("a".to_string()),
            PreHirStmt::Goto("b".to_string()),
            PreHirStmt::Label("b".to_string()),
            PreHirStmt::Goto("a".to_string()),
        ],
        ..Default::default()
    };
    let mutual_rep = check_symbolic_equivalence_report(&mutual_cycle, &trivial_hir);
    assert_eq!(
        mutual_rep.outcome,
        SymbolicOutcome::Unsupported(UnsupportedReason::ContainsLoop)
    );
    let mutual_oblig = mutual_rep.path_obligations.expect("obligations");
    assert_eq!(mutual_oblig.prehir.cyclic_goto_count, 1);
    assert!(mutual_oblig.prehir.explored_steps < 16);

    // 3. Void return / fallthrough / unresolved goto refused cleanly.
    let void_branch = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::If {
            cond: pre_ult(
                PreHirExpr::Var("x".to_string()),
                PreHirExpr::Const(2, u4.clone()),
            ),
            then_body: vec![PreHirStmt::Return(None)].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, u4.clone())))].into(),
        }],
        ..Default::default()
    };
    let void_rep = check_symbolic_equivalence_report(&void_branch, &trivial_hir);
    assert_eq!(void_rep.verdict, ValidationVerdict::Unsupported);
    assert_eq!(
        void_rep
            .path_obligations
            .expect("oblig")
            .prehir
            .void_return_count,
        1
    );

    let unresolved = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::Goto("missing".to_string())],
        ..Default::default()
    };
    let unresolved_rep = check_symbolic_equivalence_report(&unresolved, &trivial_hir);
    assert_eq!(unresolved_rep.verdict, ValidationVerdict::Unsupported);
    assert_eq!(
        unresolved_rep
            .path_obligations
            .expect("oblig")
            .prehir
            .unresolved_goto_count,
        1
    );

    // 4. Explicit Fault / NonReturning / Unsupported leaves in lowered paths never promote.
    let x = SymExpr::new_var("x", 4);
    let is_zero = SymExpr::new_eq(x.clone(), SymExpr::new_const(0, 4));
    let is_one = SymExpr::new_eq(x.clone(), SymExpr::new_const(1, 4));
    let else_cond = SymExpr::new_not(SymExpr::Ite {
        cond: Box::new(is_zero.clone()),
        t: Box::new(SymExpr::Const { val: 1, size: 1 }),
        f: Box::new(is_one.clone()),
    });
    let with_fault = LoweredFunctionPaths {
        leaves: vec![
            SymbolicPathLeaf {
                path_cond: is_zero,
                exit: SymbolicExitKind::Fault("div_by_zero".to_string()),
            },
            SymbolicPathLeaf {
                path_cond: is_one,
                exit: SymbolicExitKind::NonReturning("abort".to_string()),
            },
            SymbolicPathLeaf {
                path_cond: else_cond,
                exit: SymbolicExitKind::ReturnValue(SymExpr::new_const(0, 4)),
            },
        ],
        explored_steps: 3,
        budget_limit: PATH_EXPLORE_BUDGET,
        return_bits: Some(4),
    };
    let ok_hir = LoweredFunctionPaths {
        leaves: vec![SymbolicPathLeaf {
            path_cond: SymExpr::Const { val: 1, size: 1 },
            exit: SymbolicExitKind::ReturnValue(SymExpr::new_const(0, 4)),
        }],
        explored_steps: 1,
        budget_limit: PATH_EXPLORE_BUDGET,
        return_bits: Some(4),
    };
    let pre_id = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4, 0)],
        ..Default::default()
    };
    let fault_rep = check_lowered_paths_equivalence(
        &pre_id,
        &trivial_hir,
        &with_fault,
        &ok_hir,
        std::slice::from_ref(&x),
        SymbolicVerificationOptions::default(),
        None,
    );
    assert_eq!(fault_rep.verdict, ValidationVerdict::Inconclusive);
    let oblig = fault_rep.path_obligations.expect("oblig");
    assert_eq!(oblig.prehir.fault_count, 1);
    assert_eq!(oblig.prehir.nonreturning_count, 1);
    assert!(matches!(
        oblig.prehir.complete_exit_modeling,
        ObligationStatus::Failed { .. }
    ));
}

#[test]
fn path_exploration_and_obligation_budget_caps_stay_unproven() {
    let u4 = u4_ty();

    // 1. Build a 13-deep diamond chain of non-returning-branch `If`s without returns until the end -> 2^13 paths > 4096.
    let mut pre_body = Vec::new();
    for _ in 0..13 {
        pre_body.push(PreHirStmt::If {
            cond: pre_ult(
                PreHirExpr::Var("x".to_string()),
                PreHirExpr::Const(1, u4.clone()),
            ),
            then_body: Vec::new().into(),
            else_body: Vec::new().into(),
        });
    }
    pre_body.push(PreHirStmt::Return(Some(PreHirExpr::Const(0, u4.clone()))));
    let pre_blowup = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: pre_body,
        ..Default::default()
    };
    let hir_const = HirFunction {
        return_type: u4.clone(),
        params: vec![hir_param("x", u4.clone(), 0)],
        body: vec![HirStmt::Return(Some(HirExpr::Const(0, u4.clone())))],
        ..Default::default()
    };
    let blowup_rep = check_symbolic_equivalence_report(&pre_blowup, &hir_const);
    assert_eq!(blowup_rep.verdict, ValidationVerdict::Unsupported);
    assert!(
        blowup_rep
            .evidence
            .unresolved_effects
            .iter()
            .any(|e| { e.kind == UnresolvedEffectKind::PathBudget })
    );

    // 2. Obligation leaf budget cap (`max_solver_checked_leaves: 1`) prevents unverified Equivalent.
    let two_branch_pre = PreHirFunction {
        return_type: u4.clone(),
        params: vec![pre_param("x", u4.clone(), 0)],
        body: vec![PreHirStmt::If {
            cond: pre_ult(
                PreHirExpr::Var("x".to_string()),
                PreHirExpr::Const(3, u4.clone()),
            ),
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, u4.clone())))].into(),
            else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, u4)))].into(),
        }],
        ..Default::default()
    };
    let budget_rep = check_symbolic_equivalence_with_options(
        &two_branch_pre,
        &hir_const,
        SymbolicVerificationOptions {
            max_solver_checked_leaves: 1,
            ..SymbolicVerificationOptions::default()
        },
    );
    assert_eq!(budget_rep.verdict, ValidationVerdict::Inconclusive);
    let budget_oblig = budget_rep.path_obligations.expect("oblig");
    assert!(matches!(
        budget_oblig.prehir.path_coverage,
        ObligationStatus::SkippedBudget { .. }
    ));
    assert!(
        budget_rep
            .evidence
            .unresolved_effects
            .iter()
            .any(|e| { e.kind == UnresolvedEffectKind::PathBudget })
    );
}
