use super::*;
#[test]
fn preview_type_hints_surface_known_pointer_alias_on_param() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                })),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Expr(HirExpr::Call {
            target: "GetClientRect".to_string(),
            args: vec![
                HirExpr::Var("param_1".to_string()),
                HirExpr::Var("param_2".to_string()),
            ],
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context.call_param_rules.push(PreviewCallParamRule {
        callee_address: None,
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
    let rendered = print_hir_function(&func);
    // The subject is the parameter alias, not the return spelling: an
    // undetermined return is now rendered at word width rather than as
    // `undefined`, which is not a C type and states no width.
    assert!(
        rendered.contains("FUN_0x140006260(long long param_1, LPRECT param_2)"),
        "{rendered}"
    );
    assert!(!rendered.contains("undefined FUN_"), "{rendered}");
}

#[test]
fn preview_type_hints_surface_known_pointer_alias_through_wrapper_cast() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                })),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Expr(HirExpr::Call {
            target: "GetClientRect".to_string(),
            args: vec![
                HirExpr::Var("param_1".to_string()),
                HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(NirType::Aggregate {
                        size: 16,
                        fields: vec![],
                    })),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_2".to_string())),
                        offset: 0,
                    }),
                },
            ],
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context.call_param_rules.push(PreviewCallParamRule {
        callee_address: None,
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
}

#[test]
fn preview_type_hints_propagate_pointer_surface_through_safe_aliases_only() {
    let pointer = || {
        NirType::Ptr(Box::new(NirType::Int {
            bits: 64,
            signed: false,
        }))
    };
    let mut func = HirFunction {
        name: "pointer_aliases".to_string(),
        params: vec![
            NirBinding {
                name: "arr".to_string(),
                ty: pointer(),
                surface_type_name: Some("int *".to_string()),
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "other".to_string(),
                ty: pointer(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![
            NirBinding {
                name: "cursor".to_string(),
                ty: pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "end".to_string(),
                ty: pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "loaded".to_string(),
                ty: pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "ambiguous".to_string(),
                ty: pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("cursor".to_string()),
                rhs: HirExpr::Var("arr".to_string()),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("cursor".to_string()),
                rhs: HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("cursor".to_string())),
                    offset: 4,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("end".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("arr".to_string())),
                    rhs: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: pointer(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("loaded".to_string()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("arr".to_string())),
                    ty: pointer(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("ambiguous".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("arr".to_string())),
                    rhs: Box::new(HirExpr::Var("other".to_string())),
                    ty: pointer(),
                },
            },
        ],
        ..Default::default()
    };

    let stats = apply_preview_type_hints(
        &mut func,
        &PreviewTypeContext::default(),
        &crate::midend::HashMap::default(),
    );

    assert_eq!(stats.local_surface_hits, 2);
    assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("int *"));
    assert_eq!(func.locals[1].surface_type_name.as_deref(), Some("int *"));
    assert_eq!(func.locals[2].surface_type_name, None);
    assert_eq!(func.locals[3].surface_type_name, None);
    assert!(matches!(func.locals[0].ty, NirType::Ptr(_)));
    assert!(matches!(func.locals[1].ty, NirType::Ptr(_)));
}

#[test]
fn preview_type_hints_preserve_byte_scale_for_unresolved_surface_pointer_add() {
    let wide_pointer = || {
        NirType::Ptr(Box::new(NirType::Int {
            bits: 64,
            signed: false,
        }))
    };
    let index_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let scaled_index = HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs: Box::new(HirExpr::Var("index".to_string())),
        rhs: Box::new(HirExpr::Const(4, index_ty.clone())),
        ty: index_ty.clone(),
    };
    let mut func = HirFunction {
        name: "packed_pointer_add".to_string(),
        params: vec![NirBinding {
            name: "arr".to_string(),
            ty: wide_pointer(),
            surface_type_name: Some("int *".to_string()),
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![
            NirBinding {
                name: "index".to_string(),
                ty: index_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "end".to_string(),
                ty: wide_pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "unscaled".to_string(),
                ty: wide_pointer(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("end".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("arr".to_string())),
                    rhs: Box::new(scaled_index),
                    ty: wide_pointer(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("unscaled".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("arr".to_string())),
                    rhs: Box::new(HirExpr::Var("index".to_string())),
                    ty: wide_pointer(),
                },
            },
        ],
        ..Default::default()
    };

    apply_preview_type_hints(
        &mut func,
        &PreviewTypeContext::default(),
        &crate::midend::HashMap::default(),
    );

    assert_eq!(func.locals[1].surface_type_name.as_deref(), Some("int *"));
    let HirStmt::Assign {
        rhs: HirExpr::Binary {
            lhs: scaled_base, ..
        },
        ..
    } = &func.body[0]
    else {
        panic!("expected scaled pointer add");
    };
    assert!(matches!(
        scaled_base.as_ref(),
        HirExpr::Cast {
            ty: NirType::Ptr(inner),
            expr,
        } if matches!(inner.as_ref(), NirType::Int { bits: 8, signed: false })
            && matches!(expr.as_ref(), HirExpr::Var(name) if name == "arr")
    ));

    let HirStmt::Assign {
        rhs: HirExpr::Binary {
            lhs: unscaled_base, ..
        },
        ..
    } = &func.body[1]
    else {
        panic!("expected unscaled pointer add");
    };
    assert!(matches!(unscaled_base.as_ref(), HirExpr::Var(name) if name == "arr"));
}
