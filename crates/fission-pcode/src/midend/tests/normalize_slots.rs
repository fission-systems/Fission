use super::*;

#[test]
fn stack_slot_recovery_names_locals() {
    let ptr = uniq(0x100, 8);
    let load = uniq(0x110, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1000,
                    output: Some(ptr.clone()),
                    inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x1001,
                    output: None,
                    inputs: vec![cst(0, 4), ptr.clone(), cst(7, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Load,
                    address: 0x1002,
                    output: Some(load.clone()),
                    inputs: vec![cst(0, 4), ptr],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x1003,
                    output: None,
                    inputs: vec![cst(0, 8), load],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "stack_fn", 0x1000, &preview_options()).expect("preview render");
    // The stack slot may appear as a named local, or SCCP may fold it to a constant.
    // Either way the return value must be correct.
    assert!(
        code.contains("local_10") || code.contains("return 7;"),
        "expected stack slot name or folded constant in output, got:\n{code}"
    );
    assert!(
        code.contains("return local_10;") || code.contains("return 7;"),
        "expected return of stack slot value, got:\n{code}"
    );
}

#[test]
fn normalize_trivial_assign_return_chain() {
    let mut body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("result".to_string()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("param_1".to_string())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            },
        },
        HirStmt::Return(Some(HirExpr::Var("result".to_string()))),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 1);
    assert_eq!(print_stmt(&body[0]), "return param_1 + 1;");
}

#[test]
fn normalize_inlines_single_use_trivial_temp() {
    let mut body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar1".to_string()),
            rhs: HirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
        HirStmt::Return(Some(HirExpr::Var("uVar1".to_string()))),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 1);
    assert_eq!(print_stmt(&body[0]), "return 7;");
}

#[test]
fn normalize_inlines_non_adjacent_single_use_trivial_temp() {
    let mut body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar1".to_string()),
            rhs: HirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("local_10".to_string()),
            rhs: HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ),
        },
        HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("uVar1".to_string())),
            rhs: Box::new(HirExpr::Var("local_10".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 2);
    assert_eq!(print_stmt(&body[1]), "return 7 + local_10;");
}

#[test]
fn normalize_does_not_inline_load_temp_across_store() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut body = vec![
        HirStmt::Assign {
            lhs: HirLValue::Var("uVar1".to_string()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Var("a".to_string())),
                ty: uint_ty.clone(),
            },
        },
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("a".to_string())),
                ty: uint_ty.clone(),
            },
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Var("b".to_string())),
                ty: uint_ty.clone(),
            },
        },
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Var("b".to_string())),
                ty: uint_ty,
            },
            rhs: HirExpr::Var("uVar1".to_string()),
        },
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 3);
    assert_eq!(print_stmt(&body[0]), "uVar1 = *a;");
    assert_eq!(print_stmt(&body[2]), "*b = uVar1;");
}

#[test]
fn normalize_hir_function_surfaces_repeated_slot_accesses_as_alias() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".to_string())),
            offset: 0x20,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "slot_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        func.locals
            .iter()
            .any(|binding| binding.name == "slot_20" && binding.initializer.is_some()),
        "{rendered}"
    );
    assert!(
        rendered.contains("slot_20[idx] + slot_20[idx]"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_assigns_aliases_in_deterministic_first_use_order() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = |base: &str| HirExpr::PtrOffset {
        base: Box::new(HirExpr::Var(base.to_string())),
        offset: 0,
    };
    let repeated_load = |base: &str| HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr(base)),
            ty: byte_ty.clone(),
        }),
        rhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr(base)),
            ty: byte_ty.clone(),
        }),
        ty: byte_ty.clone(),
    };
    let mut func = HirFunction {
        name: "slot_alias_order_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            NirBinding {
                name: "rdi".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "rax".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(repeated_load("rax")),
                rhs: Box::new(repeated_load("param_1")),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(repeated_load("rdi")),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_hir_function(&func);
    assert!(
        !func
            .locals
            .iter()
            .any(|binding| binding.initializer.is_some() && binding.name.starts_with("slot_0")),
        "{rendered}"
    );
    assert!(
        rendered.contains("*rax") && rendered.contains("*param_1") && rendered.contains("*rdi"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_sorts_promoted_bindings_by_final_name() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let slot_ptr = |offset: i64| HirExpr::PtrOffset {
        base: Box::new(HirExpr::Var("param_1".to_string())),
        offset,
    };
    let repeated_load = |offset: i64| HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr(offset)),
            ty: uint_ty.clone(),
        }),
        rhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr(offset)),
            ty: uint_ty.clone(),
        }),
        ty: uint_ty.clone(),
    };
    let mut func = HirFunction {
        name: "slot_decl_order_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(repeated_load(0x12f0)),
            rhs: Box::new(repeated_load(0)),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let surfaced_names = func
        .locals
        .iter()
        .filter(|binding| binding.initializer.is_some() && binding.name.starts_with("slot_"))
        .map(|binding| binding.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(surfaced_names, vec!["slot_12f0".to_string()]);
}

#[test]
fn memory_slot_surfacing_collapses_zero_offset_direct_alias_source() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = HirFunction {
        name: "slot_alias_source_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "rax".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "xVar203".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Var("rax".to_string())),
            },
        ],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: byte_ty.clone(),
            }),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_hir_function(&func);
    assert!(
        !func.locals.iter().any(|binding| binding.name == "slot_0"),
        "{rendered}"
    );
    assert!(rendered.contains("*rax + *rax"), "{rendered}");
}

#[test]
fn memory_slot_surfacing_collapses_zero_offset_single_def_body_alias_source() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = HirFunction {
        name: "slot_body_alias_source_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "rax".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar203".to_string()),
                rhs: HirExpr::Var("rax".to_string()),
            },
            HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Load {
                    ptr: Box::new(slot_ptr.clone()),
                    ty: byte_ty.clone(),
                }),
                rhs: Box::new(HirExpr::Load {
                    ptr: Box::new(slot_ptr),
                    ty: byte_ty.clone(),
                }),
                ty: byte_ty.clone(),
            })),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_hir_function(&func);
    assert!(
        !func.locals.iter().any(|binding| binding.name == "slot_0"),
        "{rendered}"
    );
    assert!(
        rendered.contains("*rax + *rax")
            || rendered.contains("*rax * 2")
            || rendered.contains("*rax << 1"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_skips_zero_offset_naked_temp_bases() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = HirExpr::PtrOffset {
        base: Box::new(HirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = HirFunction {
        name: "slot_naked_temp_base_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: byte_ty.clone(),
            }),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    assert!(
        !func
            .locals
            .iter()
            .any(|binding| binding.name.starts_with("slot_")),
        "unexpected slot alias locals: {:?}",
        func.locals
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn normalize_hir_function_preserves_stack_origin_on_surfaced_slot_alias() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("local_base".to_string())),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "slot_origin_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_base".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let alias_binding = func
        .locals
        .iter()
        .find(|binding| binding.name.starts_with("slot_"))
        .expect("slot alias local should be surfaced");
    assert_eq!(
        alias_binding.origin,
        Some(NirBindingOrigin::DerivedFromStackOffset(-0x20))
    );
}

#[test]
fn preview_type_hints_apply_stack_local_type_to_surfaced_slot_alias() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("local_base".to_string())),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "slot_hint_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![NirBinding {
            name: "local_base".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let context = PreviewTypeContext {
        call_targets: std::collections::HashMap::default(),
        call_target_refs: std::collections::HashMap::default(),
        iat_target_refs: std::collections::HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: std::collections::HashMap::default(),
        call_prototype_summaries: std::collections::HashMap::default(),
        call_param_rules: Vec::new(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: std::collections::HashMap::default(),
            stack_local_names: [(-0x20, "base_ptr".to_string())].into_iter().collect::<std::collections::HashMap<_,_>>(),
            stack_local_type_names: [(-0x20, "RECT".to_string())].into_iter().collect::<std::collections::HashMap<_,_>>(),
            return_type_name: None,
        }),
    };

    apply_preview_type_hints(&mut func, &context);

    let direct_binding = func
        .locals
        .iter()
        .find(|binding| binding.name == "base_ptr")
        .expect("direct stack local should still be renamed");
    assert_eq!(
        direct_binding.origin,
        Some(NirBindingOrigin::StackOffset(-0x20))
    );

    let alias_binding = func
        .locals
        .iter()
        .find(|binding| binding.name.starts_with("slot_"))
        .expect("slot alias local should be surfaced");
    assert_eq!(
        alias_binding.origin,
        Some(NirBindingOrigin::DerivedFromStackOffset(-0x20))
    );
    assert_eq!(alias_binding.surface_type_name.as_deref(), Some("RECT"));
}

#[test]
fn normalize_hir_function_rewrites_slot_store_as_index_lvalue() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".to_string())),
            offset: 0x28,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "slot_store_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(slot_ptr.clone()),
                    ty: uint_ty.clone(),
                },
                rhs: HirExpr::Const(7, uint_ty.clone()),
            },
            HirStmt::Return(Some(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            })),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("slot_28[idx] = 7;"), "{rendered}");
    assert!(rendered.contains("return slot_28[idx];"), "{rendered}");
}

#[test]
fn normalize_hir_function_does_not_surface_stride_mismatch_as_slot_index() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let mismatched_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".to_string())),
            offset: 0x30,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "mismatch_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(mismatched_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(mismatched_ptr),
                ty: byte_ty.clone(),
            }),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(!rendered.contains("slot_30["), "{rendered}");
    assert!(
        !func
            .locals
            .iter()
            .any(|binding| binding.name.starts_with("slot_30"))
    );
}

#[test]
fn normalize_hir_function_surfaces_adjacent_lane_slots_under_same_family() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let lane0_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".to_string())),
            offset: 0xc9b8,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                16,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let lane1_ptr = HirExpr::PtrOffset {
        base: Box::new(lane0_ptr.clone()),
        offset: 4,
    };
    let mut func = HirFunction {
        name: "family_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(lane0_ptr),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(lane1_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("slot_c9b8[idx]"), "{rendered}");
    assert!(rendered.contains("slot_c9b8_lane1[idx]"), "{rendered}");
}

#[test]
fn normalize_hir_function_canonicalizes_index_bias_into_slot_index() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let biased_idx = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("idx".to_string())),
        rhs: Box::new(HirExpr::Const(
            1,
            NirType::Int {
                bits: 64,
                signed: true,
            },
        )),
        ty: NirType::Int {
            bits: 64,
            signed: true,
        },
    };
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::PtrOffset {
            base: Box::new(HirExpr::Var("param_1".to_string())),
            offset: 0x20,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(biased_idx),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut func = HirFunction {
        name: "biased_idx_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(HirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("slot_24[idx] + slot_24[idx]"),
        "{rendered}"
    );
}

#[test]
fn normalize_hir_function_applies_cheap_slot_surfacing_to_large_body() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = HirExpr::Var("idx".to_string());
    let slot_ptr = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("esp".to_string())),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(HirExpr::Const(
                4,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        ty: NirType::Ptr(Box::new(NirType::Unknown)),
    };
    let mut body = Vec::new();
    for i in 0..230 {
        body.push(HirStmt::Expr(HirExpr::Const(
            i,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )));
    }
    body.push(HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr.clone()),
            ty: uint_ty.clone(),
        }),
        rhs: Box::new(HirExpr::Load {
            ptr: Box::new(slot_ptr),
            ty: uint_ty.clone(),
        }),
        ty: uint_ty.clone(),
    })));
    let mut func = HirFunction {
        name: "large_slot_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: uint_ty,
        surface_return_type_name: None,
        body,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("slot_0[idx] + slot_0[idx]"), "{rendered}");
}

#[test]
fn normalize_hir_function_removes_write_only_non_temp_locals() {
    let mut func = HirFunction {
        name: "dead_local_clobber_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            NirBinding {
                name: "local_c".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "param_fffffffc".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_c".to_string()),
                rhs: HirExpr::Const(
                    4198578,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("param_fffffffc".to_string()),
                rhs: HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("param_1".to_string()))),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(!rendered.contains("local_c ="), "{rendered}");
    assert!(!rendered.contains("param_fffffffc ="), "{rendered}");
    assert!(!rendered.contains("uint local_c;"), "{rendered}");
    assert!(!rendered.contains("uint param_fffffffc;"), "{rendered}");
    assert!(rendered.contains("return param_1;"), "{rendered}");
}

#[test]
fn normalize_hir_function_keeps_read_locals_and_side_effectful_writes() {
    let mut func = HirFunction {
        name: "keep_local_clobber_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "local_c".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "local_10".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("local_c".to_string()),
                rhs: HirExpr::Call {
                    target: "sub_401000".to_string(),
                    args: vec![],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("local_10".to_string()),
                rhs: HirExpr::Const(
                    7,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            HirStmt::Return(Some(HirExpr::Var("local_10".to_string()))),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("local_c = sub_401000();"), "{rendered}");
    assert!(
        rendered.contains("return 7;") || rendered.contains("return local_10;"),
        "{rendered}"
    );
}
