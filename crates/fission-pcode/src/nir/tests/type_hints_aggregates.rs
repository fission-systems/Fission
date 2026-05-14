use super::*;
#[test]
fn preview_type_hints_surface_known_local_aggregate_alias() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
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
        locals: vec![NirBinding {
            name: "local_3c".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Expr(HirExpr::Call {
                target: "GetClientRect".to_string(),
                args: vec![
                    HirExpr::Var("param_1".to_string()),
                    HirExpr::Var("param_2".to_string()),
                ],
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Var("param_2".to_string())),
                    ty: NirType::Aggregate {
                        size: 16,
                        fields: vec![],
                    },
                },
                rhs: HirExpr::Var("local_3c".to_string()),
            },
        ],
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

    let mut hints = std::collections::HashMap::new();
    hints.insert("param_2".to_string(), context.call_param_rules[0].clone());
    let mut local_hints = std::collections::HashMap::new();
    collect_local_surface_hints(&func.body, &hints, &func, &mut local_hints);
    assert_eq!(
        local_hints.get("local_3c").map(String::as_str),
        Some("RECT")
    );

    apply_preview_type_hints(&mut func, &context);
    assert_eq!(func.params[1].surface_type_name.as_deref(), Some("LPRECT"));
    assert_eq!(func.locals[0].surface_type_name.as_deref(), Some("RECT"));
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("RECT local_3c;"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("*param_2 = local_3c;"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_surface_local_alias_through_aggregate_copy_wrapper() {
    let func = HirFunction {
        name: "FUN_0x140006260".to_string(),
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
                surface_type_name: Some("LPRECT".to_string()),
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![NirBinding {
            name: "local_3c".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::Cast {
                    ty: NirType::Ptr(Box::new(NirType::Aggregate {
                        size: 16,
                        fields: vec![],
                    })),
                    expr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("param_2".to_string())),
                        offset: 0,
                    }),
                }),
                ty: NirType::Aggregate {
                    size: 16,
                    fields: vec![],
                },
            },
            rhs: HirExpr::AggregateCopy {
                src: Box::new(HirExpr::Var("local_3c".to_string())),
                size: 16,
            },
        }],
        ..Default::default()
    };

    let mut hints = std::collections::HashMap::new();
    hints.insert(
        "param_2".to_string(),
        PreviewCallParamRule {
            callee_address: None,
            callee_name: "GetClientRect".to_string(),
            arg_index: 1,
            pointer_alias: "LPRECT".to_string(),
            pointee_alias: "RECT".to_string(),
            pointer_size: 8,
            pointee_sizes: vec![16],
        },
    );
    let mut local_hints = std::collections::HashMap::new();
    collect_local_surface_hints(&func.body, &hints, &func, &mut local_hints);
    assert_eq!(
        local_hints.get("local_3c").map(String::as_str),
        Some("RECT")
    );
}

#[test]
fn normalize_removes_dead_aggregate_temp_after_direct_store_recovery() {
    let mut func = HirFunction {
        name: "FUN_0x140006260".to_string(),
        params: vec![NirBinding {
            name: "param_2".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Aggregate {
                size: 16,
                fields: vec![],
            })),
            surface_type_name: Some("LPRECT".to_string()),
            origin: None,
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "local_3c".to_string(),
            ty: NirType::Aggregate {
                size: 16,
                fields: vec![],
            },
            surface_type_name: Some("RECT".to_string()),
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![HirStmt::If {
            cond: HirExpr::Const(1, NirType::Bool),
            then_body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar32".to_string()),
                    rhs: HirExpr::Var("local_3c".to_string()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("param_2".to_string())),
                        ty: NirType::Aggregate {
                            size: 16,
                            fields: vec![],
                        },
                    },
                    rhs: HirExpr::Var("local_3c".to_string()),
                },
            ],
            else_body: vec![],
        }],
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        !rendered.contains("xVar32 = local_3c;"),
        "rendered:\n{}",
        rendered
    );
    assert!(!rendered.contains("xVar32;"), "rendered:\n{}", rendered);
    assert!(
        rendered.contains("*param_2 = local_3c;"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn normalize_recovers_field_access_after_aggregate_pointer_inference() {
    let mut func = HirFunction {
        name: "process_config_like".to_string(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: Vec::new(),
        return_type: NirType::Int {
            bits: 16,
            signed: false,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Load {
            ptr: Box::new(HirExpr::Cast {
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 16,
                    signed: false,
                })),
                expr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("param_1".to_string())),
                    rhs: Box::new(HirExpr::Const(
                        8,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Ptr(Box::new(NirType::Unknown)),
                }),
            }),
            ty: NirType::Int {
                bits: 16,
                signed: false,
            },
        }))],
        is_64bit: true,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("param_1->field_8"),
        "rendered:\n{}",
        rendered
    );
    assert!(!rendered.contains("param_1 + 8"), "rendered:\n{}", rendered);
}

#[test]
fn normalize_rewrites_constant_index_alias_back_to_aggregate_field() {
    let mut func = HirFunction {
        name: "process_config_alias".to_string(),
        params: vec![NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }],
        locals: vec![NirBinding {
            name: "slot_4".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: None,
            initializer: Some(HirExpr::Cast {
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 32,
                    signed: false,
                })),
                expr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var("param_1".to_string())),
                    offset: 4,
                }),
            }),
        }],
        return_type: NirType::Int {
            bits: 32,
            signed: false,
        },
        surface_return_type_name: None,
        body: vec![HirStmt::Return(Some(HirExpr::Index {
            base: Box::new(HirExpr::Var("slot_4".to_string())),
            index: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            elem_ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }))],
        is_64bit: true,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("param_1->field_4"),
        "rendered:\n{}",
        rendered
    );
    assert!(!rendered.contains("slot_4[0]"), "rendered:\n{}", rendered);
    assert!(!rendered.contains("slot_4"), "rendered:\n{}", rendered);
}

#[test]
fn preview_type_hints_fold_subpiece_lane_aggregate_store_back_to_local() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(uniq(0x200, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("PUSH RSI".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x140006261,
                    output: Some(reg(0x20, 8)),
                    inputs: vec![reg(0x20, 8), cst(0x58, 8)],
                    asm_mnemonic: Some("SUB RSP,0x58".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006262,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006263,
                    output: None,
                    inputs: vec![cst(0x14012c378, 8), reg(0x08, 8), reg(0x10, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1400062af,
                    output: Some(uniq(0x4e80, 8)),
                    inputs: vec![cst(0x2c, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400062af,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![cst(0xb3f820180, 8), uniq(0x4e80, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062af,
                    output: Some(uniq(0x8fd00, 16)),
                    inputs: vec![uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x1200, 4)),
                    inputs: vec![uniq(0x8fd00, 16), cst(0, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x1204, 4)),
                    inputs: vec![uniq(0x8fd00, 16), cst(4, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 9,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x1208, 4)),
                    inputs: vec![uniq(0x8fd00, 16), cst(8, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 10,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x120c, 4)),
                    inputs: vec![uniq(0x8fd00, 16), cst(12, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 11,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062b4,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![reg(0x1200, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 12,
                    opcode: PcodeOpcode::Store,
                    address: 0x1400062b4,
                    output: None,
                    inputs: vec![cst(0xb3f820180, 8), reg(0x30, 8), uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 13,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006267,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let context = get_client_rect_preview_type_context();

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");

    assert!(
        rendered.contains("LPRECT param_2"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("RECT local_34;"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("*param_2 = local_34;"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_fold_full_register_aggregate_store_back_to_local() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(uniq(0x200, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("PUSH RSI".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x140006261,
                    output: Some(reg(0x20, 8)),
                    inputs: vec![reg(0x20, 8), cst(0x58, 8)],
                    asm_mnemonic: Some("SUB RSP,0x58".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006262,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006263,
                    output: None,
                    inputs: vec![cst(0x14012c378, 8), reg(0x08, 8), reg(0x10, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1400062af,
                    output: Some(uniq(0x4e80, 8)),
                    inputs: vec![cst(0x2c, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400062af,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![cst(0xb3f820180, 8), uniq(0x4e80, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062af,
                    output: Some(reg(0x1200, 16)),
                    inputs: vec![uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062b4,
                    output: Some(uniq(0x8fd00, 16)),
                    inputs: vec![reg(0x1200, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::Store,
                    address: 0x1400062b4,
                    output: None,
                    inputs: vec![cst(0xb3f820180, 8), reg(0x30, 8), uniq(0x8fd00, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 9,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006267,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let context = get_client_rect_preview_type_context();

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");

    assert!(
        rendered.contains("LPRECT param_2"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("RECT local_34;"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("*param_2 = local_34;"),
        "rendered:\n{}",
        rendered
    );
}

#[test]
fn preview_type_hints_fold_qword_lane_aggregate_store_back_to_local() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(uniq(0x200, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("PUSH RSI".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x140006261,
                    output: Some(reg(0x20, 8)),
                    inputs: vec![reg(0x20, 8), cst(0x58, 8)],
                    asm_mnemonic: Some("SUB RSP,0x58".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006262,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006263,
                    output: None,
                    inputs: vec![cst(0x14012c378, 8), reg(0x08, 8), reg(0x10, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1400062af,
                    output: Some(uniq(0x4e80, 8)),
                    inputs: vec![cst(0x2c, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400062af,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![cst(0xb3f820180, 8), uniq(0x4e80, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062af,
                    output: Some(uniq(0x8fd00, 16)),
                    inputs: vec![uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x1200, 8)),
                    inputs: vec![uniq(0x8fd00, 16), cst(0, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::SubPiece,
                    address: 0x1400062af,
                    output: Some(reg(0x1208, 8)),
                    inputs: vec![uniq(0x8fd00, 16), cst(8, 4)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 9,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400062b4,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![reg(0x1200, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 10,
                    opcode: PcodeOpcode::Store,
                    address: 0x1400062b4,
                    output: None,
                    inputs: vec![cst(0xb3f820180, 8), reg(0x30, 8), uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 11,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006267,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let context = get_client_rect_preview_type_context();

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");

    assert!(
        rendered.contains("LPRECT param_2"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("RECT local_34;"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("*param_2 = local_34;"),
        "rendered:\n{}",
        rendered
    );
}
