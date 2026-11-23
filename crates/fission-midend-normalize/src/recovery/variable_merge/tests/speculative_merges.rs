use super::super::apply_variable_merge_pass;
use crate::prelude::*;

#[test]
fn test_speculative_disjoint_merge_type_unification() {
    let b1 = PreHirBinding {
        name: "temp_1".to_string(),
        ty: NirType::Unknown,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "temp_2".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_fn2".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            // Disjoint live ranges: temp_1 is used and then dead before temp_2 is used
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_1".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_2".to_string()),
                rhs: PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    assert!(changed);

    // They should be merged into temp_1 or temp_2 (since they are disjoint and types are compatible)
    assert_eq!(func.locals.len(), 1);
    // The unified type should be Int
    assert_eq!(
        func.locals[0].ty,
        NirType::Int {
            bits: 32,
            signed: false
        }
    );
}

#[test]
fn test_speculative_merging_symbolic_priority_guard() {
    let b1 = PreHirBinding {
        name: "result".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "retval".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_pri_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("result".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("retval".to_string()),
                rhs: PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    // Should NOT merge because both result and retval have priority >= 2.
    assert!(!changed);
    assert_eq!(func.locals.len(), 2);
}

#[test]
fn test_speculative_merging_disjoint_domain_guard() {
    let b1 = PreHirBinding {
        name: "local_10".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-16)),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "local_20".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::StackOffset(-32)),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_domain_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_10".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_20".to_string()),
                rhs: PreHirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
        ],
        ..Default::default()
    };

    let changed = apply_variable_merge_pass(&mut func);
    // Should NOT merge because neither variable is a hardware temporary (both are stack slots).
    assert!(!changed);
    assert_eq!(func.locals.len(), 2);
}

#[test]
fn test_speculative_merging_control_flow_boundary_guard() {
    let b1 = PreHirBinding {
        name: "temp_1".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let b2 = PreHirBinding {
        name: "temp_2".to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };

    let mut func = PreHirFunction {
        name: "test_ctrl_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![b1, b2],
        body: vec![
            // temp_1 defined before the loop
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_1".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::While {
                cond: PreHirExpr::Const(1, NirType::Bool),
                body: vec![
                    // temp_2 defined inside the loop
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("temp_2".to_string()),
                        rhs: PreHirExpr::Const(
                            2,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    },
                ]
                .into(),
            },
        ],
        ..Default::default()
    };

    let changed1 = apply_variable_merge_pass(&mut func);
    // Should NOT merge because temp_2 starts inside the loop, and no direct copy links them.
    assert!(!changed1);
    assert_eq!(func.locals.len(), 2);
}

/// saturating_add / multi-register arithmetic: co-occurring GPRs must not
/// collapse into one name (`eax + eax`).
#[test]
fn variable_merge_preserves_cooccurring_hw_gprs_in_add() {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let bind = |name: &str| PreHirBinding {
        name: name.to_string(),
        ty: i32_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    };
    let mut func = PreHirFunction {
        name: "sat_like".to_string(),
        params: vec![
            PreHirBinding {
                name: "param_1".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            PreHirBinding {
                name: "param_2".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![bind("ecx"), bind("edx"), bind("eax")],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("ecx".to_string()),
                rhs: PreHirExpr::Var("param_1".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("edx".to_string()),
                rhs: PreHirExpr::Var("param_2".to_string()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("eax".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("ecx".to_string())),
                    rhs: Box::new(PreHirExpr::Var("edx".to_string())),
                    ty: i32_ty.clone(),
                },
            },
            PreHirStmt::If {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLt,
                    lhs: Box::new(PreHirExpr::Var("eax".to_string())),
                    rhs: Box::new(PreHirExpr::Var("ecx".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    2147483647,
                    i32_ty.clone(),
                )))]
                .into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
        ],
        return_type: i32_ty,
        ..Default::default()
    };

    let _ = apply_variable_merge_pass(&mut func);
    let code = format!("{func:?}");
    assert!(
        !code.contains("eax + eax") && !code.contains("eax+eax"),
        "co-occurring GPRs collapsed:\n{code}"
    );
    // Prefer keeping distinct param sources or distinct regs.
    let has_distinct_add = code.contains("ecx") && code.contains("edx")
        || code.contains("param_1") && code.contains("param_2");
    assert!(
        has_distinct_add || code.contains("+"),
        "expected multi-register sum form:\n{code}"
    );
    assert!(
        !(code.contains("eax = param_1")
            && code.contains("eax = param_2")
            && code.matches("eax = param").count() >= 2),
        "both params forced onto eax:\n{code}"
    );
}

#[test]
fn speculative_merge_cannot_reuse_hw_register_for_disjoint_temp_range() {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = PreHirFunction {
        name: "definition_scoped_registers".to_string(),
        locals: vec![
            PreHirBinding {
                name: "edx".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::TempPreserved),
                initializer: None,
            },
            PreHirBinding {
                name: "uVar1".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        ],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("edx".to_string()),
                rhs: PreHirExpr::Const(1, i32_ty.clone()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("edx".to_string())),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar1".to_string()),
                rhs: PreHirExpr::Const(2, i32_ty.clone()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("uVar1".to_string()))),
        ],
        return_type: i32_ty,
        ..Default::default()
    };

    assert!(!apply_variable_merge_pass(&mut func));
    assert!(func.locals.iter().any(|binding| binding.name == "edx"));
    assert!(func.locals.iter().any(|binding| binding.name == "uVar1"));
}

#[test]
fn speculative_merge_respects_blocked_pairs_across_transitive_groups() {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let binding = |name: &str| PreHirBinding {
        name: name.to_string(),
        ty: i32_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    };
    let mut func = PreHirFunction {
        name: "transitive_live_ranges".to_string(),
        locals: vec![
            binding("temp_early"),
            binding("temp_left"),
            binding("temp_right"),
        ],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_early".to_string()),
                rhs: PreHirExpr::Const(0, i32_ty.clone()),
            },
            PreHirStmt::Expr(PreHirExpr::Var("temp_early".to_string())),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_left".to_string()),
                rhs: PreHirExpr::Const(7, i32_ty.clone()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("temp_right".to_string()),
                rhs: PreHirExpr::Const(3, i32_ty.clone()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(PreHirExpr::Var("temp_left".to_string())),
                rhs: Box::new(PreHirExpr::Var("temp_right".to_string())),
                ty: i32_ty.clone(),
            })),
        ],
        return_type: i32_ty,
        ..Default::default()
    };

    assert!(apply_variable_merge_pass(&mut func));
    let PreHirStmt::Return(Some(PreHirExpr::Binary { lhs, rhs, .. })) = &func.body[4] else {
        panic!("expected binary return");
    };
    assert_ne!(lhs, rhs, "co-occurring group members were merged");
}
