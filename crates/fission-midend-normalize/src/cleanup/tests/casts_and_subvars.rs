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
