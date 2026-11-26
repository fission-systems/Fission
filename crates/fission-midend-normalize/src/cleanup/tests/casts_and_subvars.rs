use super::*;

#[test]
fn cast_elision_removes_self_widening_assignment() {
    let mut func = PreHirFunction {
        name: "test_self_widening_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "uVar84".to_string(),
            ty: int(32),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("uVar84".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: int(64),
                    expr: Box::new(PreHirExpr::Var("uVar84".to_string())),
                },
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("uVar84".to_string()))),
        ],
        ..Default::default()
    };

    assert!(cast_elision_pass(&mut func));
    assert_eq!(func.body.len(), 1);
    assert_eq!(
        func.body[0],
        PreHirStmt::Return(Some(PreHirExpr::Var("uVar84".to_string()))),
    );
}

#[test]
fn cast_elision_keeps_self_narrowing_assignment_to_wide_binding() {
    let mut func = PreHirFunction {
        name: "test_self_narrowing_cast".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![PreHirBinding {
            name: "xVar29".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![PreHirStmt::Assign {
            lhs: PreHirLValue::Var("xVar29".to_string()),
            rhs: PreHirExpr::Cast {
                ty: int(32),
                expr: Box::new(PreHirExpr::Var("xVar29".to_string())),
            },
        }],
        ..Default::default()
    };

    assert!(!cast_elision_pass(&mut func));
}

#[test]
fn subvar_trim_eliminates_redundant_casts() {
    use crate::cleanup::apply_subvar_trim_pass;

    // We want to test two cases:
    // Case 1: (u8)(u32)b  where b: u8
    // Case 2: (u8)(y & 0xff)
    let u8_ty = int(8);
    let u32_ty = int(32);

    let mut func = PreHirFunction {
        name: "test_subvar_trim".to_string(),
        int_param_offsets: Vec::new(),
        locals: vec![
            PreHirBinding {
                name: "b".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            PreHirBinding {
                name: "y".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "z".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
            PreHirBinding {
                name: "res1".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "res2".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // x = (u32)b
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("b".to_string())),
                },
            },
            // res1 = (u8)x
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("res1".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("x".to_string())),
                },
            },
            // z = y & 0xff
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("z".to_string()),
                rhs: PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("y".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0xff, u32_ty.clone())),
                    ty: u32_ty.clone(),
                },
            },
            // res2 = (u8)z
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("res2".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u8_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("z".to_string())),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_subvar_trim_pass(&mut func));

    // Expected:
    // res1 = b (cast elided and replaced with original byte variable b)
    // res2 = (u8)y (bitwise AND elided, cast moved directly to y)
    let PreHirStmt::Assign { rhs: rhs1, .. } = &func.body[1] else {
        panic!();
    };
    assert_eq!(rhs1, &PreHirExpr::Var("b".to_string()));

    let PreHirStmt::Assign { rhs: rhs2, .. } = &func.body[3] else {
        panic!();
    };
    if let PreHirExpr::Cast { ty, expr } = rhs2 {
        assert_eq!(ty, &u8_ty);
        assert_eq!(expr.as_ref(), &PreHirExpr::Var("y".to_string()));
    } else {
        panic!("expected cast of y, got {:?}", rhs2);
    }
}

#[test]
fn subvar_trim_does_not_drop_narrowing_intermediate_cast() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };

    let mut func = PreHirFunction {
        params: vec![PreHirBinding {
            name: "x".to_string(),
            ty: u32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            PreHirBinding {
                name: "trunc".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "res".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // trunc = (u8)x
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("trunc".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u8_ty,
                    expr: Box::new(PreHirExpr::Var("x".to_string())),
                },
            },
            // res = (u32)trunc
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("res".to_string()),
                rhs: PreHirExpr::Cast {
                    ty: u32_ty,
                    expr: Box::new(PreHirExpr::Var("trunc".to_string())),
                },
            },
        ],
        ..Default::default()
    };

    assert!(!apply_subvar_trim_pass(&mut func));
    let PreHirStmt::Assign { rhs, .. } = &func.body[1] else {
        panic!();
    };
    assert!(matches!(rhs, PreHirExpr::Cast { .. }));
}

/// `t = (u64)y; y = 7; r = (u32)t;` -- `r` is the *old* `y`. Folding the
/// cast pair to `r = y` moves the read of `y` past its redefinition.
#[test]
fn subvar_trim_does_not_move_a_read_past_a_redefinition_of_its_source() {
    let u32_ty = int(32);
    let u64_ty = int(64);
    let binding = |name: &str, ty: NirType| PreHirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: None,
        initializer: None,
    };
    let assign = |lhs: &str, rhs: PreHirExpr| PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs.to_string()),
        rhs,
    };
    let mut func = PreHirFunction {
        params: vec![binding("y", u32_ty.clone())],
        locals: vec![binding("t", u64_ty.clone()), binding("r", u32_ty.clone())],
        body: vec![
            assign(
                "t",
                PreHirExpr::Cast {
                    ty: u64_ty,
                    expr: Box::new(PreHirExpr::Var("y".to_string())),
                },
            ),
            assign("y", PreHirExpr::Const(7, u32_ty.clone())),
            assign(
                "r",
                PreHirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("t".to_string())),
                },
            ),
            PreHirStmt::Return(Some(PreHirExpr::Var("r".to_string()))),
        ],
        return_type: u32_ty,
        ..Default::default()
    };

    crate::cleanup::apply_subvar_trim_pass(&mut func);
    let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
        panic!("{:#?}", func.body);
    };
    assert_ne!(
        rhs,
        &PreHirExpr::Var("y".to_string()),
        "`r` must not read `y` after `y = 7`"
    );
}

/// A stack slot can change with no assignment to its name: here a call
/// receives the frame pointer, so it may write the slot. `r` must stay the
/// value `t` captured before the call, not a re-read of the slot after it.
#[test]
fn subvar_trim_does_not_move_a_stack_slot_read_past_a_call() {
    let u32_ty = int(32);
    let u64_ty = int(64);
    let binding = |name: &str, ty: NirType, origin| PreHirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin,
        initializer: None,
    };
    let assign = |lhs: &str, rhs: PreHirExpr| PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs.to_string()),
        rhs,
    };
    let mut func = PreHirFunction {
        locals: vec![
            binding(
                "slot",
                u32_ty.clone(),
                Some(NirBindingOrigin::StackOffset(-0x1c)),
            ),
            binding("t", u64_ty.clone(), Some(NirBindingOrigin::Temp)),
            binding("r", u32_ty.clone(), Some(NirBindingOrigin::Temp)),
        ],
        body: vec![
            assign(
                "t",
                PreHirExpr::Cast {
                    ty: u64_ty,
                    expr: Box::new(PreHirExpr::Var("slot".to_string())),
                },
            ),
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "mktime".to_string(),
                args: vec![PreHirExpr::Var("sp".to_string())],
                ty: int(64),
            }),
            assign(
                "r",
                PreHirExpr::Cast {
                    ty: u32_ty.clone(),
                    expr: Box::new(PreHirExpr::Var("t".to_string())),
                },
            ),
            PreHirStmt::Return(Some(PreHirExpr::Var("r".to_string()))),
        ],
        return_type: u32_ty,
        ..Default::default()
    };

    crate::cleanup::apply_subvar_trim_pass(&mut func);
    let PreHirStmt::Assign { rhs, .. } = &func.body[2] else {
        panic!("{:#?}", func.body);
    };
    assert_ne!(
        rhs,
        &PreHirExpr::Var("slot".to_string()),
        "`r` must not re-read the slot after a call that may write it"
    );
}
