use super::*;
use std::collections::HashMap;

#[test]
fn preview_type_hints_rename_params_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Var("param_2".to_string())))],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["hwnd".to_string(), "lpRect".to_string()],
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("BOOL".to_string()),
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.params[0].name, "hwnd");
    assert_eq!(func.params[1].name, "lpRect");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("hwnd"));
    assert!(rendered.contains("lpRect"));
    assert!(!rendered.contains("param_2"));
}

#[test]
fn preview_type_hints_rename_stack_locals_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_20".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_20".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("local_20".to_string()))),
        ],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
            stack_local_type_names: HashMap::default(),
            return_type_name: None,
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.locals[0].name, "rect");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("rect"));
    assert!(!rendered.contains("local_20"));
}

#[test]
fn preview_type_hints_surface_param_types_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Return(None)],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::from([(0, "HWND".to_string()), (1, "LPRECT".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: None,
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.params[0].surface_type_name.as_deref(), Some("HWND"));
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("undefined FUN_0x140001000(HWND param_1, LPRECT param_2)"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_surface_stack_local_types_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_20".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Return(None)],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
            stack_local_type_names: HashMap::from([(-0x20, "RECT".to_string())]),
            return_type_name: None,
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.locals[0].name, "rect");
    assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("RECT"));
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("RECT rect;"), "rendered:\n{}", rendered);
}

#[test]
fn preview_type_hints_surface_return_type_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Return(None)],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("BOOL".to_string()),
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(func.surface_return_type_name.as_deref(), Some("BOOL"));
    let rendered = print_hir_function(&func);
    assert!(
        rendered.starts_with("BOOL FUN_0x140001000("),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_elide_surface_implied_return_cast() {
    let int64 = NirType::Int {
        bits: 64,
        signed: true,
    };
    let int32 = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = HirFunction {
        name: "add".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: int64.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            },
            NirBinding {
                name: "param_2".to_string(),
                ty: int64.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(1)),
                initializer: None,
            },
        ],
        locals: vec![],
        return_type: int32.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Cast {
            ty: int32.clone(),
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("param_1".to_string())),
                rhs: Box::new(HirExpr::Var("param_2".to_string())),
                ty: int64,
            }),
        }))],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["a".to_string(), "b".to_string()],
            param_type_names: HashMap::from([(0, "int".to_string()), (1, "int".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("int".to_string()),
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    let rendered = print_hir_function(&func);
    assert!(rendered.contains("return a + b;"), "rendered:\n{rendered}");
    assert!(!rendered.contains("(uint)"), "rendered:\n{rendered}");
}

#[test]
fn preview_type_hints_create_missing_surface_params_from_function_hints() {
    let mut func = HirFunction {
        name: "FUN_0x140001420".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Return(None)],
        ..Default::default()
    };

    let context = PreviewTypeContext {
        call_targets: HashMap::default(),
        call_target_refs: HashMap::default(),
        iat_target_refs: HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: HashMap::default(),
        call_prototype_summaries: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["param_1".to_string()],
            param_type_names: HashMap::from([(0, "_func_5014 *".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("int".to_string()),
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    let rendered = print_hir_function(&func);
    assert!(
        rendered.starts_with("int FUN_0x140001420(_func_5014 * param_1)"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_explicit_function_types_override_derived_aliases() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
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
                origin: Some(NirBindingOrigin::ParamIndex(1)),
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
    context.function_hints = Some(PreviewFunctionHints {
        param_names: Vec::new(),
        param_type_names: HashMap::from([(1, "MY_RECT_PTR".to_string())]),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
    });

    apply_preview_type_hints(&mut func, &context);

    assert_eq!(
        func.params[1].surface_type_name.as_deref(),
        Some("MY_RECT_PTR")
    );
}

#[test]
fn preview_type_hints_collect_hint_stats() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            NirBinding {
                name: "param_1".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
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
        locals: vec![
            NirBinding {
                name: "local_20".to_string(),
                ty: NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(-0x20)),
                initializer: None,
            },
            NirBinding {
                name: "slot_20".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::DerivedFromStackOffset(-0x20)),
                initializer: None,
            },
        ],
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
    context.function_hints = Some(PreviewFunctionHints {
        param_names: vec!["hwnd".to_string()],
        param_type_names: HashMap::from([(0, "HWND".to_string())]),
        stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
        stack_local_type_names: HashMap::from([(-0x20, "RECT".to_string())]),
        return_type_name: Some("BOOL".to_string()),
    });

    let stats = apply_preview_type_hints(&mut func, &context);

    assert_eq!(stats.explicit_param_name_hits, 1);
    assert_eq!(stats.explicit_local_name_hits, 1);
    assert_eq!(stats.explicit_param_type_hits, 1);
    assert_eq!(stats.explicit_local_type_hits, 2);
    assert_eq!(stats.explicit_return_type_hit, 1);
    // derived-origin tracker remains separate from explicit facts
    assert_eq!(stats.derived_origin_type_hits, 1);
}

fn point_aggregate_binding() -> NirBinding {
    NirBinding {
        name: "param_1".to_string(),
        ty: NirType::Ptr(Box::new(NirType::Aggregate {
            size: 8,
            fields: vec![
                fission_midend_core::StructField {
                    offset: 0,
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    name: "field_0".to_string(),
                },
                fission_midend_core::StructField {
                    offset: 4,
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    name: "field_4".to_string(),
                },
            ],
        })),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(0)),
        initializer: None,
    }
}

fn point_struct_type_hint() -> fission_midend_core::NirStructTypeHint {
    fission_midend_core::NirStructTypeHint {
        name: "Point".to_string(),
        size: 8,
        fields: vec![
            fission_midend_core::NirStructFieldHint {
                name: "x".to_string(),
                type_name: "int".to_string(),
                offset: 0,
                size: 4,
            },
            fission_midend_core::NirStructFieldHint {
                name: "y".to_string(),
                type_name: "int".to_string(),
                offset: 4,
                size: 4,
            },
        ],
    }
}

#[test]
fn preview_type_hints_overlay_debug_struct_field_names_onto_recovered_aggregate() {
    let mut func = HirFunction {
        name: "sum_point".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![point_aggregate_binding()],
        locals: vec![],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_string())))],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context
        .struct_types
        .insert("Point".to_string(), point_struct_type_hint());
    context.function_hints = Some(PreviewFunctionHints {
        param_names: vec![],
        param_type_names: HashMap::from([(0, "Point*".to_string())]),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
    });

    let stats = apply_preview_type_hints(&mut func, &context);

    assert_eq!(stats.debug_struct_field_hits, 2);
    let NirType::Ptr(inner) = &func.params[0].ty else {
        panic!("expected Ptr(Aggregate)");
    };
    let NirType::Aggregate { fields, .. } = inner.as_ref() else {
        panic!("expected Aggregate");
    };
    assert_eq!(fields[0].name, "x");
    assert_eq!(fields[1].name, "y");
}

#[test]
fn preview_type_hints_debug_struct_field_names_reject_multi_level_pointer() {
    let mut func = HirFunction {
        name: "sum_point".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![point_aggregate_binding()],
        locals: vec![],
        return_type: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_string())))],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context
        .struct_types
        .insert("Point".to_string(), point_struct_type_hint());
    // Double pointer: the aggregate at *this* binding's offsets belongs to
    // `**param_1`, not `*param_1`, so the overlay must not apply.
    context.function_hints = Some(PreviewFunctionHints {
        param_names: vec![],
        param_type_names: HashMap::from([(0, "Point**".to_string())]),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
    });

    let stats = apply_preview_type_hints(&mut func, &context);

    assert_eq!(stats.debug_struct_field_hits, 0);
    let NirType::Ptr(inner) = &func.params[0].ty else {
        panic!("expected Ptr(Aggregate)");
    };
    let NirType::Aggregate { fields, .. } = inner.as_ref() else {
        panic!("expected Aggregate");
    };
    assert_eq!(fields[0].name, "field_0");
    assert_eq!(fields[1].name, "field_4");
}

#[test]
fn preview_type_hints_overlay_debug_struct_field_names_rewrites_body_field_access() {
    // `FieldAccess` AST nodes (as normalize's ptr_arith recovery would have
    // already built them, synthetic-named) referencing the same binding
    // this session's field-name overlay renames. The printer reads
    // `field_name` straight off these nodes, not off the binding's
    // `StructField` annotation -- so the overlay must rewrite them too, or
    // renaming the type-level annotation alone has zero visible effect.
    let field_access = |offset: u32, field_name: &str, ty: NirType| HirExpr::FieldAccess {
        base: Box::new(HirExpr::Var("param_1".to_string())),
        field_name: field_name.to_string(),
        offset,
        ty,
    };
    let int_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = HirFunction {
        name: "sum_point".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![point_aggregate_binding()],
        locals: vec![],
        return_type: int_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(field_access(0, "field_0", int_ty.clone())),
            rhs: Box::new(field_access(4, "field_4", int_ty.clone())),
            ty: int_ty,
        }))],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context
        .struct_types
        .insert("Point".to_string(), point_struct_type_hint());
    context.function_hints = Some(PreviewFunctionHints {
        param_names: vec!["p".to_string()],
        param_type_names: HashMap::from([(0, "Point*".to_string())]),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
    });

    let stats = apply_preview_type_hints(&mut func, &context);
    assert_eq!(stats.debug_struct_field_hits, 2);

    let HirStmt::Return(Some(HirExpr::Binary { lhs, rhs, .. })) = &func.body[0] else {
        panic!("expected Return(Binary)");
    };
    let HirExpr::FieldAccess { field_name, .. } = lhs.as_ref() else {
        panic!("expected FieldAccess lhs");
    };
    assert_eq!(field_name, "x");
    let HirExpr::FieldAccess { field_name, .. } = rhs.as_ref() else {
        panic!("expected FieldAccess rhs");
    };
    assert_eq!(field_name, "y");

    let rendered = print_hir_function(&func);
    assert!(rendered.contains("p->x"), "rendered: {rendered}");
    assert!(rendered.contains("p->y"), "rendered: {rendered}");
    assert!(!rendered.contains("field_0"), "rendered: {rendered}");
    assert!(!rendered.contains("field_4"), "rendered: {rendered}");
}
