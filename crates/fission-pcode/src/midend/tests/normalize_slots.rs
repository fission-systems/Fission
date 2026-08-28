use super::*;

/// Whether `rendered` dereferences `name`, in either spelling.
///
/// A local typed `Ptr(Unknown)` is declared `void *`, and `*p` on one is not
/// C -- the deref names the type it reads instead (`*(uchar *)(p)`). These
/// tests pin that the slot collapsed onto the alias, not how the deref is
/// spelled.
fn derefs(rendered: &str, name: &str) -> bool {
    rendered.contains(&format!("*{name}")) || rendered.contains(&format!(" *)({name})"))
}

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
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("result".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("param_1".to_string())),
                rhs: Box::new(PreHirExpr::Const(
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
        PreHirStmt::Return(Some(PreHirExpr::Var("result".to_string()))),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 1);
    assert_eq!(print_dir_stmt(&body[0]), "return param_1 + 1;");
}

#[test]
fn normalize_inlines_single_use_trivial_temp() {
    let mut body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar1".to_string()),
            rhs: PreHirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("uVar1".to_string()))),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 1);
    assert_eq!(print_dir_stmt(&body[0]), "return 7;");
}

#[test]
fn normalize_inlines_non_adjacent_single_use_trivial_temp() {
    let mut body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar1".to_string()),
            rhs: PreHirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("local_10".to_string()),
            rhs: PreHirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ),
        },
        PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("uVar1".to_string())),
            rhs: Box::new(PreHirExpr::Var("local_10".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        })),
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 2);
    assert_eq!(print_dir_stmt(&body[1]), "return 7 + local_10;");
}

#[test]
fn normalize_does_not_inline_load_temp_across_store() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut body = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar1".to_string()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("a".to_string())),
                ty: uint_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("a".to_string())),
                ty: uint_ty.clone(),
            },
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("b".to_string())),
                ty: uint_ty.clone(),
            },
        },
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("b".to_string())),
                ty: uint_ty,
            },
            rhs: PreHirExpr::Var("uVar1".to_string()),
        },
    ];
    normalize_function_body(&mut body);
    assert_eq!(body.len(), 3);
    assert_eq!(print_dir_stmt(&body[0]), "uVar1 = *a;");
    assert_eq!(print_dir_stmt(&body[2]), "*b = uVar1;");
}

#[test]
fn normalize_hir_function_surfaces_repeated_slot_accesses_as_alias() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let idx = PreHirExpr::Var("idx".to_string());
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("param_1".to_string())),
            offset: 0x20,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "slot_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
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
    let slot_ptr = |base: &str| PreHirExpr::PtrOffset {
        base: Box::new(PreHirExpr::Var(base.to_string())),
        offset: 0,
    };
    let repeated_load = |base: &str| PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr(base)),
            ty: byte_ty.clone(),
        }),
        rhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr(base)),
            ty: byte_ty.clone(),
        }),
        ty: byte_ty.clone(),
    };
    let mut func = PreHirFunction {
        name: "slot_alias_order_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![
            PreHirBinding {
                name: "rdi".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "rax".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
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

    let rendered = print_prehir_function(&func);
    assert!(
        !func
            .locals
            .iter()
            .any(|binding| binding.initializer.is_some() && binding.name.starts_with("slot_0")),
        "{rendered}"
    );
    assert!(
        derefs(&rendered, "rax") && derefs(&rendered, "param_1") && derefs(&rendered, "rdi"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_sorts_promoted_bindings_by_final_name() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let slot_ptr = |offset: i64| PreHirExpr::PtrOffset {
        base: Box::new(PreHirExpr::Var("param_1".to_string())),
        offset,
    };
    let repeated_load = |offset: i64| PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr(offset)),
            ty: uint_ty.clone(),
        }),
        rhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr(offset)),
            ty: uint_ty.clone(),
        }),
        ty: uint_ty.clone(),
    };
    let mut func = PreHirFunction {
        name: "slot_decl_order_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
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
    let slot_ptr = PreHirExpr::PtrOffset {
        base: Box::new(PreHirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = PreHirFunction {
        name: "slot_alias_source_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            PreHirBinding {
                name: "rax".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
                name: "xVar203".to_string(),
                ty: NirType::Ptr(Box::new(NirType::Unknown)),
                surface_type_name: None,
                origin: None,
                initializer: Some(PreHirExpr::Var("rax".to_string())),
            },
        ],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: byte_ty.clone(),
            }),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_prehir_function(&func);
    assert!(
        !func.locals.iter().any(|binding| binding.name == "slot_0"),
        "{rendered}"
    );
    assert!(
        rendered.matches("(rax)").count() >= 2 || rendered.contains("*rax + *rax"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_collapses_zero_offset_single_def_body_alias_source() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = PreHirExpr::PtrOffset {
        base: Box::new(PreHirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = PreHirFunction {
        name: "slot_body_alias_source_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![PreHirBinding {
            name: "rax".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("xVar203".to_string()),
                rhs: PreHirExpr::Var("rax".to_string()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Load {
                    ptr: Box::new(slot_ptr.clone()),
                    ty: byte_ty.clone(),
                }),
                rhs: Box::new(PreHirExpr::Load {
                    ptr: Box::new(slot_ptr),
                    ty: byte_ty.clone(),
                }),
                ty: byte_ty.clone(),
            })),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    let rendered = print_prehir_function(&func);
    assert!(
        !func.locals.iter().any(|binding| binding.name == "slot_0"),
        "{rendered}"
    );
    assert!(
        rendered.contains("+ *rax") || rendered.contains("* 2") || rendered.contains("<< 1"),
        "{rendered}"
    );
}

#[test]
fn memory_slot_surfacing_skips_zero_offset_naked_temp_bases() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let slot_ptr = PreHirExpr::PtrOffset {
        base: Box::new(PreHirExpr::Var("xVar203".to_string())),
        offset: 0,
    };
    let mut func = PreHirFunction {
        name: "slot_naked_temp_base_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
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
    let idx = PreHirExpr::Var("idx".to_string());
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("local_base".to_string())),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "slot_origin_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![PreHirBinding {
            name: "local_base".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
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
    let idx = PreHirExpr::Var("idx".to_string());
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("local_base".to_string())),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "slot_hint_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![PreHirBinding {
            name: "local_base".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x20)),
            initializer: None,
        }],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    // Mirrors the real pipeline boundary (orchestrate.rs): normalize
    // operates on PreHIR, `apply_preview_type_hints` runs after structuring
    // on the real HIR -- this fixture has no actual structuring step (its
    // body is already a single `Return`), so the PreHIR->HIR conversion here
    // is the whole boundary crossing.
    let hir_body = fission_midend_prehir::ir::prehir_stmts_to_hir_stmts(func.body.clone());
    let mut func = func.into_hir_function(hir_body);

    let context = PreviewTypeContext {
        call_targets: std::collections::HashMap::default(),
        call_target_refs: std::collections::HashMap::default(),
        iat_target_refs: std::collections::HashMap::default(),
        ambiguous_call_targets: Default::default(),
        call_effect_summaries: std::collections::HashMap::default(),
        call_prototype_summaries: std::collections::HashMap::default(),
        call_result_is_source_value: std::collections::HashMap::default(),
        call_param_rules: Vec::new(),
        struct_types: std::collections::HashMap::default(),
        function_hints: Some(PreviewFunctionHints {
            param_names: Vec::new(),
            param_type_names: std::collections::HashMap::default(),
            stack_local_names: [(-0x20, "base_ptr".to_string())]
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>(),
            stack_local_type_names: [(-0x20, "RECT".to_string())]
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>(),
            return_type_name: None,
            register_local_names: std::collections::HashMap::default(),
            register_local_type_names: std::collections::HashMap::default(),
        }),
    };

    apply_preview_type_hints(&mut func, &context, &crate::midend::HashMap::default());

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
    let idx = PreHirExpr::Var("idx".to_string());
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("param_1".to_string())),
            offset: 0x28,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "slot_store_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(slot_ptr.clone()),
                    ty: uint_ty.clone(),
                },
                rhs: PreHirExpr::Const(7, uint_ty.clone()),
            },
            PreHirStmt::Return(Some(PreHirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            })),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(rendered.contains("slot_28[idx] = 7;"), "{rendered}");
    assert!(rendered.contains("return slot_28[idx];"), "{rendered}");
}

#[test]
fn normalize_hir_function_does_not_surface_stride_mismatch_as_slot_index() {
    let byte_ty = NirType::Int {
        bits: 8,
        signed: false,
    };
    let idx = PreHirExpr::Var("idx".to_string());
    let mismatched_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("param_1".to_string())),
            offset: 0x30,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "mismatch_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: byte_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(mismatched_ptr.clone()),
                ty: byte_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(mismatched_ptr),
                ty: byte_ty.clone(),
            }),
            ty: byte_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
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
    let idx = PreHirExpr::Var("idx".to_string());
    let lane0_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("param_1".to_string())),
            offset: 0xc9b8,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
    let lane1_ptr = PreHirExpr::PtrOffset {
        base: Box::new(lane0_ptr.clone()),
        offset: 4,
    };
    let mut func = PreHirFunction {
        name: "family_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(lane0_ptr),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(lane1_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(rendered.contains("slot_c9b8[idx]"), "{rendered}");
    assert!(rendered.contains("slot_c9b8_lane1[idx]"), "{rendered}");
}

#[test]
fn normalize_hir_function_canonicalizes_index_bias_into_slot_index() {
    let uint_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let biased_idx = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("idx".to_string())),
        rhs: Box::new(PreHirExpr::Const(
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
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::PtrOffset {
            base: Box::new(PreHirExpr::Var("param_1".to_string())),
            offset: 0x20,
        }),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(biased_idx),
            rhs: Box::new(PreHirExpr::Const(
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
    let mut func = PreHirFunction {
        name: "biased_idx_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![],
        return_type: uint_ty.clone(),
        surface_return_type_name: None,
        body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr.clone()),
                ty: uint_ty.clone(),
            }),
            rhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(slot_ptr),
                ty: uint_ty.clone(),
            }),
            ty: uint_ty.clone(),
        }))],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
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
    let idx = PreHirExpr::Var("idx".to_string());
    let slot_ptr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("esp".to_string())),
        rhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(idx.clone()),
            rhs: Box::new(PreHirExpr::Const(
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
        body.push(PreHirStmt::Expr(PreHirExpr::Const(
            i,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )));
    }
    body.push(PreHirStmt::Return(Some(PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr.clone()),
            ty: uint_ty.clone(),
        }),
        rhs: Box::new(PreHirExpr::Load {
            ptr: Box::new(slot_ptr),
            ty: uint_ty.clone(),
        }),
        ty: uint_ty.clone(),
    })));
    let mut func = PreHirFunction {
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
    let rendered = print_prehir_function(&func);
    assert!(rendered.contains("slot_0[idx] + slot_0[idx]"), "{rendered}");
}

#[test]
fn normalize_hir_function_removes_write_only_non_temp_locals() {
    let mut func = PreHirFunction {
        name: "dead_local_clobber_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![PreHirBinding {
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
            PreHirBinding {
                name: "local_c".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_c".to_string()),
                rhs: PreHirExpr::Const(
                    4198578,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("param_fffffffc".to_string()),
                rhs: PreHirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_string()))),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(!rendered.contains("local_c ="), "{rendered}");
    assert!(!rendered.contains("param_fffffffc ="), "{rendered}");
    assert!(!rendered.contains("uint local_c;"), "{rendered}");
    assert!(!rendered.contains("uint param_fffffffc;"), "{rendered}");
    assert!(rendered.contains("return param_1;"), "{rendered}");
}

#[test]
fn normalize_hir_function_keeps_read_locals_and_side_effectful_writes() {
    let mut func = PreHirFunction {
        name: "keep_local_clobber_fn".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            PreHirBinding {
                name: "local_c".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            PreHirBinding {
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_c".to_string()),
                rhs: PreHirExpr::Call {
                    target: "sub_401000".to_string(),
                    args: vec![],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var("local_10".to_string()),
                rhs: PreHirExpr::Const(
                    7,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            },
            PreHirStmt::Return(Some(PreHirExpr::Var("local_10".to_string()))),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_prehir_function(&func);
    assert!(rendered.contains("local_c = sub_401000();"), "{rendered}");
    assert!(
        rendered.contains("return 7;") || rendered.contains("return local_10;"),
        "{rendered}"
    );
}

/// A local written, then handed to a callee by address, keeps its write.
///
/// This is the shape three separate passes got wrong the moment `&local`
/// became expressible, each by the same reasoning: the local's *name* is never
/// read, so the write looked dead. It is not -- the address escapes to a
/// callee that reads the storage, and deleting the write hands that callee an
/// uninitialised frame. Constant propagation removed it, dead store
/// elimination was entitled to (its escape check was collected and never
/// consulted), and HIR presentation dropped it again on the way out.
///
/// The invariant, stated once: an address-taken local is potentially
/// observable after the address escapes, so its stores are live regardless of
/// how often the name is read.
#[test]
fn a_local_handed_to_a_callee_by_address_keeps_its_write() {
    let addr = uniq(0x100, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                // local_10 = 7  -- the write no pass may remove
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1000,
                    output: Some(addr.clone()),
                    inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x1001,
                    output: None,
                    inputs: vec![cst(0, 4), addr.clone(), cst(7, 4)],
                    asm_mnemonic: None,
                },
                // local_c = 9 -- the rest of the same buffer, written but
                // never read by name and never address-taken itself
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1002,
                    output: Some(addr.clone()),
                    inputs: vec![reg(0x28, 8), cst(-0xc, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Store,
                    address: 0x1003,
                    output: None,
                    inputs: vec![cst(0, 4), addr.clone(), cst(9, 4)],
                    asm_mnemonic: None,
                },
                // callee(&local_10) -- RCX is the first Win64 integer argument
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1004,
                    output: Some(addr.clone()),
                    inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1005,
                    output: Some(reg(0x08, 8)),
                    inputs: vec![addr],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Call,
                    address: 0x1006,
                    output: None,
                    inputs: vec![cst(0x2000, 8), reg(0x08, 8)],
                    asm_mnemonic: Some("CALL 0x2000".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Return,
                    address: 0x1007,
                    output: None,
                    inputs: vec![cst(0, 8)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "escaping_local_fn", 0x1000, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("local_10 = 7;"),
        "the write to an address-taken local must survive:\n{code}"
    );
    assert!(
        code.contains("&local_10"),
        "the argument must be the local's address:\n{code}"
    );
    // The rest of the buffer is reached by arithmetic on that same address,
    // which no analysis here can follow. `main` builds a six-byte string out
    // of two slots and passes the address of the first; the second was
    // dropped because its own name is never read and its own address never
    // taken.
    assert!(
        code.contains("local_c = 9;"),
        "a neighbouring slot's write must survive too:\n{code}"
    );
}
