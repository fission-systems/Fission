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
