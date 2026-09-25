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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["hwnd".to_string(), "lpRect".to_string()],
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("BOOL".to_string()),
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
            stack_local_type_names: HashMap::default(),
            return_type_name: None,
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

    assert_eq!(func.locals[0].name, "rect");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("rect"));
    assert!(!rendered.contains("local_20"));
}

#[test]
fn preview_type_hints_translate_cfa_relative_debug_stack_locals() {
    let mut func = HirFunction {
        name: "memory_layouts".to_string(),
        locals: vec![NirBinding {
            name: "local_30".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x30)),
            initializer: None,
        }],
        body: vec![HirStmt::Return(None)],
        ..Default::default()
    };
    let mut context = PreviewTypeContext::default();
    context.function_hints = Some(PreviewFunctionHints {
        stack_local_names: HashMap::from([(-0x30, "structural_name".to_string())]),
        debug_stack_local_names: HashMap::from([(-0x40, "source_name".to_string())]),
        debug_stack_local_type_names: HashMap::from([(-0x40, "float[4]".to_string())]),
        debug_stack_offset_base: NirStackOffsetBase::CallFrameCfa,
        ..Default::default()
    });

    let stats = apply_preview_type_hints_with_stack_bias(
        &mut func,
        &context,
        &crate::midend::HashMap::default(),
        Some(0x10),
    );

    assert_eq!(func.locals[0].name, "source_name");
    assert_eq!(
        func.locals[0].surface_type_name.as_deref(),
        Some("float[4]")
    );
    assert_eq!(stats.explicit_local_name_hits, 1);
    assert_eq!(stats.explicit_local_type_hits, 1);
}

#[test]
fn pre_hir_debug_array_hints_recover_overlapping_scalar_stack_view() {
    let u8_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = PreHirFunction {
        name: "array_stack_view".to_string(),
        locals: vec![
            PreHirBinding {
                name: "local_5".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(-5)),
                initializer: None,
            },
            PreHirBinding {
                name: "local_1".to_string(),
                ty: u8_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(-1)),
                initializer: None,
            },
        ],
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_5".to_string()),
                rhs: PreHirExpr::Const(0x0403_0201, u32_ty.clone()),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_1".to_string()),
                rhs: PreHirExpr::Const(5, u8_ty.clone()),
            },
        ],
        ..PreHirFunction::default()
    };
    let mut context = PreviewTypeContext::default();
    context.function_hints = Some(PreviewFunctionHints {
        debug_stack_local_names: HashMap::from([(-5, "data".to_string())]),
        debug_stack_local_type_names: HashMap::from([(-5, "unsigned char[5]".to_string())]),
        ..Default::default()
    });

    assert_eq!(
        apply_pre_hir_debug_array_hints(&mut func, &context, None),
        1
    );
    assert_eq!(func.locals.len(), 1);
    assert_eq!(func.locals[0].name, "data");
    assert_eq!(
        func.locals[0].surface_type_name.as_deref(),
        Some("unsigned char[5]")
    );
    assert!(matches!(
        func.locals[0].ty,
        NirType::Aggregate { size: 5, .. }
    ));
    assert!(matches!(
        &func.body[0],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { .. },
            ..
        }
    ));
    assert!(matches!(
        &func.body[1],
        PreHirStmt::Assign {
            lhs: PreHirLValue::Index { base, index, .. },
            ..
        } if matches!(base.as_ref(), PreHirExpr::Var(name) if name == "data")
            && matches!(index.as_ref(), PreHirExpr::Const(4, _))
    ));
}

#[test]
fn typed_pointer_call_decays_recovered_array_address() {
    let array_ty = NirType::Aggregate {
        size: 5,
        fields: (0..5)
            .map(|index| StructField {
                offset: index,
                ty: NirType::Int {
                    bits: 8,
                    signed: false,
                },
                name: format!("element_{index}"),
            })
            .collect(),
    };
    let mut func = HirFunction {
        name: "typed_array_call".to_string(),
        locals: vec![NirBinding {
            name: "data".to_string(),
            ty: array_ty,
            surface_type_name: Some("unsigned char[5]".to_string()),
            origin: Some(NirBindingOrigin::StackOffset(-5)),
            initializer: None,
        }],
        body: vec![HirStmt::Expr(HirExpr::Call {
            target: "checksum".to_string(),
            args: vec![HirExpr::AddressOfLocal("data".to_string())],
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })],
        ..HirFunction::default()
    };
    let mut context = PreviewTypeContext::default();
    context.call_prototype_summaries.insert(
        "checksum".to_string(),
        NirCallPrototypeSummary {
            min_arity: 1,
            max_arity: 1,
            param_pointer_pointees: vec![None],
            param_surface_type_names: vec![Some("const unsigned char *".to_string())],
            ..Default::default()
        },
    );

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

    let HirStmt::Expr(HirExpr::Call { args, .. }) = &func.body[0] else {
        panic!("expected checksum call")
    };
    assert!(matches!(&args[0], HirExpr::Var(name) if name == "data"));
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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::from([(0, "HWND".to_string()), (1, "LPRECT".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: None,
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

    assert_eq!(func.params[0].surface_type_name.as_deref(), Some("HWND"));
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
    let rendered = print_hir_function(&func);
    assert!(
        // Parameter spellings are the subject; the return is rendered at word
        // width now rather than as `undefined`, which names no C type.
        rendered.contains("FUN_0x140001000(HWND param_1, LPRECT param_2)"),
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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::from([(-0x20, "rect".to_string())]),
            stack_local_type_names: HashMap::from([(-0x20, "RECT".to_string())]),
            return_type_name: None,
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: HashMap::default(),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("BOOL".to_string()),
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

    assert_eq!(func.surface_return_type_name.as_deref(), Some("BOOL"));
    let rendered = print_hir_function(&func);
    assert!(
        rendered.starts_with("BOOL FUN_0x140001000("),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_restore_debug_pointer_typedef_shape() {
    let section_alias = "PIMAGE_SECTION_HEADER";
    let mut func = HirFunction {
        name: "section_for_address".to_string(),
        params: vec![],
        locals: vec![NirBinding {
            name: "section".to_string(),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
            surface_type_name: Some(section_alias.to_string()),
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }],
        return_type: NirType::Int {
            bits: 64,
            signed: false,
        },
        surface_return_type_name: Some(section_alias.to_string()),
        body: vec![HirStmt::Return(Some(HirExpr::Var("section".to_string())))],
        ..Default::default()
    };
    let mut context = PreviewTypeContext::default();
    context.pointer_type_aliases.insert(
        section_alias.to_string(),
        NirPointerTypeAlias {
            pointee_name: "_IMAGE_SECTION_HEADER".to_string(),
            pointer_depth: 1,
        },
    );
    context.struct_types.insert(
        "_IMAGE_SECTION_HEADER".to_string(),
        NirStructTypeHint {
            name: "_IMAGE_SECTION_HEADER".to_string(),
            size: 40,
            fields: vec![NirStructFieldHint {
                name: "VirtualAddress".to_string(),
                type_name: "DWORD".to_string(),
                offset: 12,
                size: 4,
            }],
        },
    );

    apply_preview_type_hints(&mut func, &context, &HashMap::default());

    assert!(matches!(
        &func.return_type,
        NirType::Ptr(inner)
            if matches!(inner.as_ref(), NirType::Aggregate { size: 40, .. })
    ));
    assert!(matches!(
        &func.locals[0].ty,
        NirType::Ptr(inner)
            if matches!(inner.as_ref(), NirType::Aggregate { size: 40, .. })
    ));
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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["a".to_string(), "b".to_string()],
            param_type_names: HashMap::from([(0, "int".to_string()), (1, "int".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("int".to_string()),
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

    let rendered = print_hir_function(&func);
    assert!(rendered.contains("return a + b;"), "rendered:\n{rendered}");
    assert!(!rendered.contains("(uint)"), "rendered:\n{rendered}");
}

#[test]
fn preview_type_hints_elide_incompatible_pointer_return_cast() {
    let int32 = NirType::Int {
        bits: 32,
        signed: true,
    };
    let uint8 = NirType::Int {
        bits: 8,
        signed: false,
    };
    let pointer = NirType::Ptr(Box::new(NirType::Unknown));
    let mut integer_func = HirFunction {
        name: "returns_integer".to_string(),
        locals: vec![NirBinding {
            name: "integer_temp".to_string(),
            ty: int32.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }],
        return_type: pointer.clone(),
        body: vec![HirStmt::Return(Some(HirExpr::Cast {
            ty: pointer.clone(),
            expr: Box::new(HirExpr::Cast {
                ty: uint8.clone(),
                expr: Box::new(HirExpr::Var("integer_temp".to_string())),
            }),
        }))],
        ..Default::default()
    };
    let integer_context = PreviewTypeContext {
        function_hints: Some(PreviewFunctionHints {
            return_type_name: Some("int".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    apply_preview_type_hints(
        &mut integer_func,
        &integer_context,
        &crate::midend::HashMap::default(),
    );

    assert_eq!(integer_func.return_type, int32);
    let HirStmt::Return(Some(HirExpr::Cast { ty, .. })) = &integer_func.body[0] else {
        panic!("the inner integer cast should remain on the return expression");
    };
    assert_eq!(ty, &uint8);
    let rendered = print_hir_function(&integer_func);
    assert!(
        rendered.starts_with("int returns_integer(void)"),
        "rendered:\n{rendered}"
    );
    assert!(!rendered.contains("(void *)"), "rendered:\n{rendered}");

    let mut pointer_func = HirFunction {
        name: "returns_pointer".to_string(),
        locals: vec![NirBinding {
            name: "pointer_temp".to_string(),
            ty: pointer.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }],
        return_type: pointer.clone(),
        body: vec![HirStmt::Return(Some(HirExpr::Cast {
            ty: pointer,
            expr: Box::new(HirExpr::Var("pointer_temp".to_string())),
        }))],
        ..Default::default()
    };
    let pointer_context = PreviewTypeContext {
        function_hints: Some(PreviewFunctionHints {
            return_type_name: Some("void *".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    apply_preview_type_hints(
        &mut pointer_func,
        &pointer_context,
        &crate::midend::HashMap::default(),
    );

    assert!(matches!(
        &pointer_func.body[0],
        HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Ptr(_),
            ..
        }))
    ));
    let rendered = print_hir_function(&pointer_func);
    assert!(
        rendered.starts_with("void * returns_pointer(void)"),
        "rendered:\n{rendered}"
    );
    assert!(
        rendered.contains("return (void *)pointer_temp;"),
        "rendered:\n{rendered}"
    );
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
        call_result_is_source_value: HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_type_aliases: std::collections::HashMap::default(),
        pointer_type_aliases: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: vec!["param_1".to_string()],
            param_type_names: HashMap::from([(0, "_func_5014 *".to_string())]),
            stack_local_names: HashMap::default(),
            stack_local_type_names: HashMap::default(),
            return_type_name: Some("int".to_string()),
            register_local_names: HashMap::default(),
            register_local_type_names: HashMap::default(),
            ..Default::default()
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
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

#[test]
fn preview_type_hints_rewrites_field_access_through_pointer_cast() {
    let int_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let pointer_ty = point_aggregate_binding().ty;
    let mut func = HirFunction {
        name: "read_point_y".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![point_aggregate_binding()],
        locals: vec![],
        return_type: int_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::FieldAccess {
            base: Box::new(HirExpr::Cast {
                expr: Box::new(HirExpr::Var("param_1".to_string())),
                ty: pointer_ty,
            }),
            field_name: "field_4".to_string(),
            offset: 4,
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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(stats.debug_struct_field_hits, 2);

    let HirStmt::Return(Some(HirExpr::FieldAccess { field_name, .. })) = &func.body[0] else {
        panic!("expected Return(FieldAccess)");
    };
    assert_eq!(field_name, "y");
}

#[test]
fn preview_type_hints_promotes_scalar_constant_index_to_debug_struct_field() {
    // Normalize may encode the second 32-bit field load as `Index(p, 1)`
    // while p is still inferred as `uint *`. Once the debug type proves that
    // p is a Point*, the index is still a four-byte offset, not a Point-sized
    // array step. It must therefore become p->y.
    let int_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = HirFunction {
        name: "read_second_field".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "p".to_string(),
            ty: NirType::Ptr(Box::new(int_ty.clone())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![],
        return_type: int_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Index {
            base: Box::new(HirExpr::Var("p".to_string())),
            index: Box::new(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            elem_ty: int_ty,
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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(stats.debug_struct_promotions, 1);

    let HirStmt::Return(Some(HirExpr::FieldAccess {
        field_name, offset, ..
    })) = &func.body[0]
    else {
        panic!(
            "expected constant scalar index to become FieldAccess: {:?}",
            func.body
        );
    };
    assert_eq!(field_name, "y");
    assert_eq!(*offset, 4);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("p->y"), "rendered: {rendered}");
    assert!(!rendered.contains("p[1]"), "rendered: {rendered}");
}

fn int_ty() -> NirType {
    NirType::Int {
        bits: 32,
        signed: true,
    }
}

/// Param not yet an aggregate (aggregate_fields.rs's own heuristic refuses
/// to promote from `Ptr(Int{32})`) -- the exact case a real -O0 build of
/// `struct Point { int x, y; }; int f(Point *p) { return p->x + p->y; }`
/// hits, confirmed empirically: `p`'s type lands on `Ptr(Int{32})` from the
/// first dereference and `aggregate_fields.rs` never advances it further.
#[test]
fn preview_type_hints_promotes_pointer_to_aggregate_from_debug_struct() {
    let mut func = HirFunction {
        name: "sum_point".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(int_ty())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![],
        return_type: int_ty(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("p".to_string())),
                ty: int_ty(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("p".to_string())),
                    offset: 4,
                }),
                ty: int_ty(),
            }),
            ty: int_ty(),
        }))],
        ..Default::default()
    };
    // Match `apply_function_name_hints` having already renamed param_1 -> p
    // (this test targets the later promotion stage, so pre-rename the body
    // to what it would look like at that point).
    func.params[0].name = "p".to_string();

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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(stats.debug_struct_promotions, 1);

    let NirType::Ptr(inner) = &func.params[0].ty else {
        panic!("expected promotion to Ptr(Aggregate)");
    };
    assert!(matches!(inner.as_ref(), NirType::Aggregate { .. }));

    let rendered = print_hir_function(&func);
    assert!(rendered.contains("p->x"), "rendered: {rendered}");
    assert!(rendered.contains("p->y"), "rendered: {rendered}");
}

/// The real-world dominant shape: the param gets copied into a local
/// "shadow" (`local_8 = p;`) before any use, common at -O0. The promotion
/// pass must follow this one level of single-assignment direct-copy alias,
/// or it would almost never fire on real compiler output.
#[test]
fn preview_type_hints_promotes_through_single_assignment_copy_alias() {
    let mut func = HirFunction {
        name: "sum_point".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "p".to_string(),
            ty: NirType::Ptr(Box::new(int_ty())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "local_8".to_string(),
            ty: NirType::Ptr(Box::new(int_ty())),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x8)),
            initializer: None,
        }],
        return_type: int_ty(),
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_8".to_string()),
                rhs: HirExpr::Var("p".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Load {
                ptr: Box::new(HirExpr::Var("local_8".to_string())),
                ty: int_ty(),
            })),
        ],
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
        register_local_names: HashMap::default(),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });

    let stats = apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());
    assert_eq!(stats.debug_struct_promotions, 1);

    let NirType::Ptr(inner) = &func.locals[0].ty else {
        panic!("expected local_8 promoted to Ptr(Aggregate)");
    };
    assert!(matches!(inner.as_ref(), NirType::Aggregate { .. }));

    let HirStmt::Return(Some(HirExpr::FieldAccess { field_name, .. })) = &func.body[1] else {
        panic!("expected Return(FieldAccess), body: {:?}", func.body);
    };
    assert_eq!(field_name, "x");
}

fn register_binding(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::TempPreserved),
        initializer: None,
    }
}

const EBX_ORIGIN: (u64, u32) = (0x1, 4);

#[test]
fn preview_type_hints_apply_register_resident_dwarf_name_and_type_when_unambiguous() {
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![register_binding("EBX")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("EBX".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("EBX".to_string()))),
        ],
        ..Default::default()
    };
    func.locals[0].ty = NirType::Int {
        bits: 64,
        signed: true,
    };

    let mut context = PreviewTypeContext::default();
    context.function_hints = Some(PreviewFunctionHints {
        param_names: Vec::new(),
        param_type_names: HashMap::default(),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
        register_local_names: HashMap::from([(EBX_ORIGIN.0, "counter".to_string())]),
        register_local_type_names: HashMap::from([(EBX_ORIGIN.0, "int".to_string())]),
        ..Default::default()
    });
    let register_origins: crate::midend::HashMap<String, (u64, u32)> =
        [("EBX".to_string(), EBX_ORIGIN)].into_iter().collect();

    let stats = apply_preview_type_hints(&mut func, &context, &register_origins);
    assert_eq!(stats.explicit_register_local_name_hits, 1);
    assert_eq!(stats.explicit_local_type_hits, 1);
    assert_eq!(func.locals[0].name, "counter");
    assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("int"));
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("int counter;"));
    assert!(!rendered.contains("EBX"));
}

#[test]
fn preview_type_hints_renames_synthetic_named_register_binding_by_identity() {
    // The common real-world case: materialization gave the binding a
    // generic synthetic name (`uVar0`) rather than its raw hardware
    // register name -- most register-resident values never get named after
    // their register at all. The rename must still fire because
    // `register_origins` carries the binding's *actual* originating
    // register, independent of what it happened to get named.
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![register_binding("uVar0")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar0".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("uVar0".to_string()))),
        ],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context.function_hints = Some(PreviewFunctionHints {
        param_names: Vec::new(),
        param_type_names: HashMap::default(),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
        register_local_names: HashMap::from([(EBX_ORIGIN.0, "total".to_string())]),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });
    let register_origins: crate::midend::HashMap<String, (u64, u32)> =
        [("uVar0".to_string(), EBX_ORIGIN)].into_iter().collect();

    let stats = apply_preview_type_hints(&mut func, &context, &register_origins);
    assert_eq!(stats.explicit_register_local_name_hits, 1);
    assert_eq!(func.locals[0].name, "total");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("total"));
    assert!(!rendered.contains("uVar0"));
}

#[test]
fn preview_type_hints_renames_register_local_written_more_than_once() {
    // A loop accumulator (`total = 0; ... total += x;`) is written more than
    // once *by construction* -- an assignment-count gate here would reject
    // the single most common real case this feature exists for. The DWARF
    // location list agreeing on one register across the variable's whole
    // declared scope (checked before `register_local_names` is ever
    // populated -- see `DwarfAnalyzer::parse_location_list`) is what makes
    // this safe, not how many times the binding gets written.
    let mut func = HirFunction {
        name: "FUN_0x140001000".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![register_binding("EBX")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("EBX".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("EBX".to_string()),
                rhs: HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("EBX".to_string()))),
        ],
        ..Default::default()
    };

    let mut context = PreviewTypeContext::default();
    context.function_hints = Some(PreviewFunctionHints {
        param_names: Vec::new(),
        param_type_names: HashMap::default(),
        stack_local_names: HashMap::default(),
        stack_local_type_names: HashMap::default(),
        return_type_name: None,
        register_local_names: HashMap::from([(EBX_ORIGIN.0, "counter".to_string())]),
        register_local_type_names: HashMap::default(),
        ..Default::default()
    });
    let register_origins: crate::midend::HashMap<String, (u64, u32)> =
        [("EBX".to_string(), EBX_ORIGIN)].into_iter().collect();

    let stats = apply_preview_type_hints(&mut func, &context, &register_origins);
    assert_eq!(stats.explicit_register_local_name_hits, 1);
    assert_eq!(func.locals[0].name, "counter");
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("counter"));
    assert!(!rendered.contains("EBX"));
}
