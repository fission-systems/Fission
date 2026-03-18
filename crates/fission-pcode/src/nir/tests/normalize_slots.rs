use super::*;

#[test]
fn stack_slot_recovery_names_locals() {
    let ptr = uniq(0x100, 8);
    let load = uniq(0x110, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
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
    assert!(code.contains("local_10"));
    assert!(code.contains("return local_10;"));
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
        params: vec![],
        locals: vec![],
        return_type: uint_ty,
        surface_return_type_name: None,
        body,
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("slot_0[idx] + slot_0[idx]"), "{rendered}");
}

#[test]
fn normalize_hir_function_removes_write_only_non_temp_locals() {
    let mut func = HirFunction {
        name: "dead_local_clobber_fn".to_string(),
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
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("local_c = sub_401000();"), "{rendered}");
    assert!(
        rendered.contains("return 7;") || rendered.contains("return local_10;"),
        "{rendered}"
    );
}
