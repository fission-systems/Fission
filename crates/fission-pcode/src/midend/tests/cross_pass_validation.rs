//! Cross-pass structural invariant & independent semantic-control tests (#1224).

use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirFunction, HirStmt, IrInvariantSet, NirBinding, NirType,
    validate_hir_function,
};
use std::collections::BTreeMap;

fn i32_ty() -> NirType {
    NirType::Int {
        bits: 32,
        signed: true,
    }
}

/// Independent minimal concrete evaluator over straight-line arithmetic HIR.
/// Used ONLY as an independent semantic control against structural validation:
/// it must never be treated as part of the structural validator itself.
fn eval_simple_hir(func: &HirFunction, args: &[i64]) -> Result<i64, &'static str> {
    let mut env = BTreeMap::<String, i64>::new();
    for (param, &val) in func.params.iter().zip(args.iter()) {
        env.insert(param.name.clone(), val);
    }
    for stmt in &func.body {
        match stmt {
            HirStmt::Assign {
                lhs: fission_midend_core::ir::HirLValue::Var(name),
                rhs,
            } => {
                let v = eval_expr(rhs, &env)?;
                env.insert(name.clone(), v);
            }
            HirStmt::Return(Some(expr)) => return eval_expr(expr, &env),
            _ => return Err("unsupported stmt in test evaluator"),
        }
    }
    Err("missing return")
}

fn eval_expr(expr: &HirExpr, env: &BTreeMap<String, i64>) -> Result<i64, &'static str> {
    match expr {
        HirExpr::Var(name) => env.get(name).copied().ok_or("unbound var"),
        HirExpr::Const(v, _) => Ok(*v),
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let l = eval_expr(lhs, env)?;
            let r = eval_expr(rhs, env)?;
            match op {
                HirBinaryOp::Add => Ok(l.wrapping_add(r)),
                HirBinaryOp::Sub => Ok(l.wrapping_sub(r)),
                HirBinaryOp::Mul => Ok(l.wrapping_mul(r)),
                _ => Err("unsupported binop"),
            }
        }
        _ => Err("unsupported expr"),
    }
}

#[test]
fn structural_validation_passes_semantic_bug_and_disclaims_equivalence_proof() {
    let add_fn = HirFunction {
        name: "add_pair".to_string(),
        params: vec![
            NirBinding {
                name: "a".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "b".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: i32_ty(),
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("a".to_string())),
            rhs: Box::new(HirExpr::Var("b".to_string())),
            ty: i32_ty(),
        }))],
        ..Default::default()
    };

    // Structurally valid: Add -> Sub corrupts semantics while remaining 100% structurally well-formed.
    let mut miscompiled = add_fn.clone();
    miscompiled.body = vec![HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(HirExpr::Var("a".to_string())),
        rhs: Box::new(HirExpr::Var("b".to_string())),
        ty: i32_ty(),
    }))];

    let report_orig =
        validate_hir_function(&add_fn, IrInvariantSet::HIR_STAGE, "semantic_control", 0);
    let report_mut = validate_hir_function(
        &miscompiled,
        IrInvariantSet::HIR_STAGE,
        "semantic_control",
        1,
    );
    assert!(report_orig.is_ok());
    assert!(
        report_mut.is_ok(),
        "Add -> Sub must pass structural validation"
    );

    // An independent semantic oracle still catches the divergence.
    assert_eq!(eval_simple_hir(&add_fn, &[5, 3]).unwrap(), 8);
    assert_eq!(eval_simple_hir(&miscompiled, &[5, 3]).unwrap(), 2);

    // Even when a report is constructed for an invalid mutant, semantic equivalence is never claimed.
    let mut bad_mutant = add_fn;
    bad_mutant.body.push(HirStmt::Break);
    let err1 =
        validate_hir_function(&bad_mutant, IrInvariantSet::HIR_STAGE, "det_check", 4).unwrap_err();
    let err2 =
        validate_hir_function(&bad_mutant, IrInvariantSet::HIR_STAGE, "det_check", 4).unwrap_err();
    assert!(!err1.proves_semantic_equivalence());
    assert_eq!(err1.to_string(), err2.to_string());
}
