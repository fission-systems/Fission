use super::*;
use crate::pcode::{PcodeBasicBlock, PcodeOp};

fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn uniq(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn cst(value: i64, size: u32) -> Varnode {
        Varnode::constant(value, size)
    }

    fn preview_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            format: "PE".to_string(),
            image_base: 0x1400_0000,
            sections: vec![(0x1400_1000, 0x1400_2000)],
        }
    }

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

        let code = render_mlil_preview(&func, "stack_fn", 0x1000, &preview_options())
            .expect("preview render");
        assert!(code.contains("local_10"));
        assert!(code.contains("return local_10;"));
    }

    #[test]
    fn preview_prints_direct_srem_as_mod() {
        let result = uniq(0x200, 8);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x2000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSRem,
                        address: 0x2000,
                        output: Some(result.clone()),
                        inputs: vec![reg(0x08, 8), cst(2, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x2001,
                        output: None,
                        inputs: vec![cst(0, 8), result],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "mod_ll", 0x2000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return param_1 % 2;"));
    }

    #[test]
    fn signed_mod_idiom_recognition_collapses_to_percent() {
        let base = HirExpr::Var("param_1".to_string());
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(base.clone()),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Sar,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(base.clone()),
                        rhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::And,
                            lhs: Box::new(HirExpr::Binary {
                                op: HirBinaryOp::Shr,
                                lhs: Box::new(base.clone()),
                                rhs: Box::new(HirExpr::Const(
                                    63,
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
                            rhs: Box::new(HirExpr::Const(
                                1,
                                NirType::Int {
                                    bits: 64,
                                    signed: false,
                                },
                            )),
                            ty: NirType::Int {
                                bits: 64,
                                signed: true,
                            },
                        }),
                        ty: NirType::Int {
                            bits: 64,
                            signed: true,
                        },
                    }),
                    rhs: Box::new(HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                }),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
            }),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
        };
        let mut stmt = HirStmt::Return(Some(expr));
        normalize_stmt(&mut stmt);
        let rendered = print_stmt(&stmt);
        assert_eq!(rendered, "return param_1 % 2;");
    }

    #[test]
    fn unsigned_mod_mask_recognition_collapses_to_percent() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Var("uVar1".to_string())),
            rhs: Box::new(HirExpr::Const(
                3,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return uVar1 % 4;");
    }

    #[test]
    fn unsigned_div_shift_recognition_collapses_to_div() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(HirExpr::Var("uVar1".to_string())),
            rhs: Box::new(HirExpr::Const(
                2,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return uVar1 / 4;");
    }

    #[test]
    fn signed_div_idiom_recognition_collapses_to_slash() {
        let base = HirExpr::Var("param_1".to_string());
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(base.clone()),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Shr,
                        lhs: Box::new(base.clone()),
                        rhs: Box::new(HirExpr::Const(
                            63,
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
                    rhs: Box::new(HirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
            }),
            rhs: Box::new(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return param_1 / 2;");
    }

    #[test]
    fn cast_canonicalizer_removes_duplicate_same_type_cast() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Var("uVar1".to_string())),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (uint)uVar1;");
    }

    #[test]
    fn cast_canonicalizer_drops_redundant_widen_before_narrow() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Cast {
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    expr: Box::new(HirExpr::Var("var1".to_string())),
                }),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (longlong)(uint)var1;");
    }

    #[test]
    fn cast_canonicalizer_preserves_sign_extension_chain() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
            expr: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
                expr: Box::new(HirExpr::Var("iVar1".to_string())),
            }),
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (longlong)(int)iVar1;");
    }

    #[test]
    fn condition_canonicalizer_turns_nonzero_compare_into_truthy_value() {
        let mut stmt = HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::Ne,
                lhs: Box::new(HirExpr::Var("flag".to_string())),
                rhs: Box::new(HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(Some(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )))],
            else_body: vec![],
        };
        normalize_stmt(&mut stmt);
        match stmt {
            HirStmt::If { cond, .. } => assert_eq!(print_expr(&cond), "flag"),
            _ => panic!("expected if statement"),
        }
    }

    #[test]
    fn printer_uses_precedence_aware_parentheses() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::LogicalOr,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(HirExpr::Var("a".to_string())),
                rhs: Box::new(HirExpr::Var("b".to_string())),
                ty: NirType::Bool,
            }),
            rhs: Box::new(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("c".to_string())),
                ty: NirType::Bool,
            }),
            ty: NirType::Bool,
        };
        assert_eq!(print_expr(&expr), "a && b || !c");
    }

    #[test]
    fn printer_preserves_needed_parentheses_for_mul_over_add() {
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("x".to_string())),
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
            }),
            rhs: Box::new(HirExpr::Var("y".to_string())),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        };
        assert_eq!(print_expr(&expr), "(x + 1) * y");
    }

    #[test]
    fn preview_type_hints_surface_known_pointer_alias_on_param() {
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
                },
                NirBinding {
                    name: "param_2".to_string(),
                    ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
                    surface_type_name: None,
                },
            ],
            locals: vec![],
            return_type: NirType::Unknown,
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
        };

        let mut context = PreviewTypeContext::default();
        context.call_param_rules.push(PreviewCallParamRule {
            callee_name: "GetClientRect".to_string(),
            arg_index: 1,
            pointer_alias: "LPRECT".to_string(),
            pointee_alias: "RECT".to_string(),
            pointer_size: 8,
            pointee_sizes: vec![16],
        });

        apply_preview_type_hints(&mut func, &context);
        assert_eq!(
            func.params[1].surface_type_name.as_deref(),
            Some("LPRECT")
        );
        let rendered = print_hir_function(&func);
        assert!(rendered.contains("undefined FUN_0x140006260(longlong param_1, LPRECT param_2)"));
    }

    #[test]
    fn preview_type_hints_surface_known_pointer_alias_through_wrapper_cast() {
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
                },
                NirBinding {
                    name: "param_2".to_string(),
                    ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
                    surface_type_name: None,
                },
            ],
            locals: vec![],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Expr(HirExpr::Call {
                target: "GetClientRect".to_string(),
                args: vec![
                    HirExpr::Var("param_1".to_string()),
                    HirExpr::Cast {
                        ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
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
        };

        let mut context = PreviewTypeContext::default();
        context.call_param_rules.push(PreviewCallParamRule {
            callee_name: "GetClientRect".to_string(),
            arg_index: 1,
            pointer_alias: "LPRECT".to_string(),
            pointee_alias: "RECT".to_string(),
            pointer_size: 8,
            pointee_sizes: vec![16],
        });

        apply_preview_type_hints(&mut func, &context);
        assert_eq!(
            func.params[1].surface_type_name.as_deref(),
            Some("LPRECT")
        );
    }

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
                },
                NirBinding {
                    name: "param_2".to_string(),
                    ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
                    surface_type_name: None,
                },
            ],
            locals: vec![NirBinding {
                name: "local_3c".to_string(),
                ty: NirType::Aggregate { size: 16 },
                surface_type_name: None,
            }],
            return_type: NirType::Unknown,
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
                        ty: NirType::Aggregate { size: 16 },
                    },
                    rhs: HirExpr::Var("local_3c".to_string()),
                },
            ],
        };

        let mut context = PreviewTypeContext::default();
        context.call_param_rules.push(PreviewCallParamRule {
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
        assert_eq!(local_hints.get("local_3c").map(String::as_str), Some("RECT"));

        apply_preview_type_hints(&mut func, &context);
        assert_eq!(
            func.params[1].surface_type_name.as_deref(),
            Some("LPRECT")
        );
        assert_eq!(
            func.locals[0].surface_type_name.as_deref(),
            Some("RECT")
        );
        let rendered = print_hir_function(&func);
        assert!(rendered.contains("RECT local_3c;"));
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
                },
                NirBinding {
                    name: "param_2".to_string(),
                    ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
                    surface_type_name: Some("LPRECT".to_string()),
                },
            ],
            locals: vec![NirBinding {
                name: "local_3c".to_string(),
                ty: NirType::Aggregate { size: 16 },
                surface_type_name: None,
            }],
            return_type: NirType::Unknown,
            body: vec![HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Cast {
                        ty: NirType::Ptr(Box::new(NirType::Aggregate { size: 16 })),
                        expr: Box::new(HirExpr::PtrOffset {
                            base: Box::new(HirExpr::Var("param_2".to_string())),
                            offset: 0,
                        }),
                    }),
                    ty: NirType::Aggregate { size: 16 },
                },
                rhs: HirExpr::AggregateCopy {
                    src: Box::new(HirExpr::Var("local_3c".to_string())),
                    size: 16,
                },
            }],
        };

        let mut hints = std::collections::HashMap::new();
        hints.insert(
            "param_2".to_string(),
            PreviewCallParamRule {
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
        assert_eq!(local_hints.get("local_3c").map(String::as_str), Some("RECT"));
    }

    #[test]
    fn multi_block_preview_lowers_simple_if_without_failing() {
        let cond = uniq(0x300, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3000,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3001,
                            output: None,
                            inputs: vec![cst(0x3020, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3010,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3010,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3020,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3020,
                        output: None,
                        inputs: vec![cst(0, 8), cst(1, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "branchy", 0x3000, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
        assert!(code.contains("return 0;"));
        assert!(code.contains("return 1;"));
    }

    #[test]
    fn multi_block_preview_lowers_conditional_goto_style_if() {
        let cond = uniq(0x340, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3400,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3400,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3401,
                            output: None,
                            inputs: vec![cst(0x3420, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3410,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3410,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3420,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3420,
                        output: None,
                        inputs: vec![cst(0, 8), cst(1, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "cond_goto_if", 0x3400, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
        assert!(code.contains("return 0;"));
        assert!(code.contains("return 1;"));
    }

    #[test]
    fn multi_block_preview_lowers_canonical_if_else() {
        let cond = uniq(0x350, 1);
        let ptr = uniq(0x360, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3500,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3500,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3501,
                            output: None,
                            inputs: vec![cst(0x3520, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3510,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3510,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3511,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3512,
                            output: None,
                            inputs: vec![cst(0x3530, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3520,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3520,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3521,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(2, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3530,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3530,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "if_else_fn", 0x3500, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("} else {"));
        assert!(code.contains("local_10 = 2;"));
        assert!(!code.contains("goto block_3510;"));
        assert!(!code.contains("goto block_3520;"));
    }

    #[test]
    fn multi_block_preview_lowers_if_else_with_multi_block_then_region() {
        let cond = uniq(0x370, 1);
        let ptr = uniq(0x380, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3600,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3600,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3601,
                            output: None,
                            inputs: vec![cst(0x3630, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3610,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3610,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3611,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3620,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3620,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3621,
                            output: None,
                            inputs: vec![cst(0, 4), ptr.clone(), cst(2, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3622,
                            output: None,
                            inputs: vec![cst(0x3640, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3630,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3630,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3631,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(3, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3632,
                            output: None,
                            inputs: vec![cst(0x3640, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 4,
                    start_address: 0x3640,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3640,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "if_else_chain_fn", 0x3600, &preview_options())
            .expect("preview render");
        assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("local_10 = 2;"));
        assert!(code.contains("} else {"));
        assert!(code.contains("local_10 = 3;"));
        assert!(!code.contains("goto block_3620;"));
        assert!(!code.contains("goto block_3630;"));
    }

    #[test]
    fn multi_block_preview_folds_short_circuit_and() {
        let cond_a = uniq(0x390, 1);
        let cond_b = uniq(0x391, 1);
        let ptr = uniq(0x392, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3700,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3700,
                            output: Some(cond_a.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3701,
                            output: None,
                            inputs: vec![cst(0x3730, 8), cond_a],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3710,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3710,
                            output: Some(cond_b.clone()),
                            inputs: vec![reg(0x10, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3711,
                            output: None,
                            inputs: vec![cst(0x3730, 8), cond_b],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3720,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3720,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3721,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3722,
                            output: None,
                            inputs: vec![cst(0x3730, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3730,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3730,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "short_and_fn", 0x3700, &preview_options())
            .expect("preview render");
        assert!(code.contains("&&"));
        assert!(code.contains("local_10 = 7;"));
        assert!(!code.contains("goto block_3730;"));
    }

    #[test]
    fn multi_block_preview_folds_short_circuit_or() {
        let cond_a = uniq(0x3a0, 1);
        let cond_b = uniq(0x3a1, 1);
        let ptr = uniq(0x3a2, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x3800,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3800,
                            output: Some(cond_a.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3801,
                            output: None,
                            inputs: vec![cst(0x3830, 8), cond_a],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x3810,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x3810,
                            output: Some(cond_b.clone()),
                            inputs: vec![reg(0x10, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x3811,
                            output: None,
                            inputs: vec![cst(0x3830, 8), cond_b],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x3820,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3820,
                        output: None,
                        inputs: vec![cst(0x3840, 8)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x3830,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x3830,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x3831,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(9, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x3832,
                            output: None,
                            inputs: vec![cst(0x3840, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 4,
                    start_address: 0x3840,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x3840,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "short_or_fn", 0x3800, &preview_options())
            .expect("preview render");
        assert!(code.contains("||"), "{code}");
        assert!(code.contains("local_10 = 9;"));
        assert!(!code.contains("goto block_3830;"));
    }

    #[test]
    fn multiequal_with_identical_inputs_does_not_fail_preview() {
        let phi = uniq(0x500, 8);
        let copy = uniq(0x508, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x5000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::MultiEqual,
                            address: 0x5000,
                            output: Some(phi.clone()),
                            inputs: vec![reg(0x08, 8), reg(0x08, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Copy,
                            address: 0x5001,
                            output: Some(copy.clone()),
                            inputs: vec![phi],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Return,
                            address: 0x5002,
                            output: None,
                            inputs: vec![cst(0, 8), copy],
                            asm_mnemonic: None,
                        },
                    ],
                },
            ],
        };

        let code = render_mlil_preview(&func, "phi_fn", 0x5000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return param_1;"));
    }

    #[test]
    fn piece_and_subpiece_lower_without_preview_failure() {
        let piece = uniq(0x600, 8);
        let sub = uniq(0x608, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Piece,
                        address: 0x6000,
                        output: Some(piece.clone()),
                        inputs: vec![reg(0x08, 4), reg(0x10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6001,
                        output: Some(sub.clone()),
                        inputs: vec![piece, cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x6002,
                        output: None,
                        inputs: vec![cst(0, 8), sub],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "piece_fn", 0x6000, &preview_options())
            .expect("preview render");
        assert!(code.contains("return"));
        assert!(!code.contains("goto"));
    }

    #[test]
    fn piece_recombines_matching_subpieces_back_to_source_value() {
        let whole = reg(0x08, 8);
        let hi = uniq(0x610, 4);
        let lo = uniq(0x614, 4);
        let recombined = uniq(0x618, 8);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6100,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6100,
                        output: Some(hi.clone()),
                        inputs: vec![whole.clone(), cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6101,
                        output: Some(lo.clone()),
                        inputs: vec![whole.clone(), cst(0, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Piece,
                        address: 0x6102,
                        output: Some(recombined.clone()),
                        inputs: vec![hi, lo],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x6103,
                        output: None,
                        inputs: vec![cst(0, 8), recombined],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "piece_recombine_fn", 0x6100, &preview_options())
            .expect("preview render");
        assert!(code.contains("return param_1;"));
    }

    #[test]
    fn subpieces_inline_directly_into_call_arguments() {
        let whole = reg(0x08, 8);
        let hi = uniq(0x620, 4);
        let lo = uniq(0x624, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x6200,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6200,
                        output: Some(hi.clone()),
                        inputs: vec![whole.clone(), cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::SubPiece,
                        address: 0x6201,
                        output: Some(lo.clone()),
                        inputs: vec![whole, cst(0, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Call,
                        address: 0x6202,
                        output: None,
                        inputs: vec![cst(0x140001000, 8), lo, hi],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x6203,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };

        let code = render_mlil_preview(&func, "subpiece_call_fn", 0x6200, &preview_options())
            .expect("preview render");
        assert!(code.contains("sub_140001000"));
        assert!(code.contains("(uint)param_1"));
        assert!(code.contains("(uint)(param_1 >> 32)"));
        assert!(!code.contains("tmp_"));
    }

    #[test]
    fn do_while_preview_is_lowered_without_ghidra_fallback() {
        let ptr = uniq(0x400, 8);
        let cond = uniq(0x410, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4000,
                            output: Some(ptr.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4001,
                            output: None,
                            inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4002,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 3,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4003,
                            output: None,
                            inputs: vec![cst(0x4000, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4010,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4010,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "loop_fn", 0x4000, &preview_options())
            .expect("preview render");
        assert!(code.contains("do {"));
        assert!(code.contains("local_10 = 7;"));
        assert!(code.contains("} while (param_1);"));
    }

    #[test]
    fn while_preview_lowers_multi_block_body() {
        let cond = uniq(0x420, 1);
        let ptr1 = uniq(0x421, 8);
        let ptr2 = uniq(0x422, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4100,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4100,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4101,
                            output: None,
                            inputs: vec![cst(0x4140, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4110,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4110,
                            output: Some(ptr1.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4111,
                            output: None,
                            inputs: vec![cst(0, 4), ptr1, cst(1, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x4120,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4120,
                            output: Some(ptr2.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4121,
                            output: None,
                            inputs: vec![cst(0, 4), ptr2, cst(2, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Branch,
                            address: 0x4122,
                            output: None,
                            inputs: vec![cst(0x4100, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x4140,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4140,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "while_fn", 0x4100, &preview_options())
            .expect("preview render");
        assert!(code.contains("while (!param_1) {") || code.contains("while (param_1) {"));
        assert!(code.contains("local_10 = 1;"));
        assert!(code.contains("local_14 = 2;"));
        assert!(!code.contains("goto block_4100;"));
    }

    #[test]
    fn do_while_preview_lowers_multi_block_body() {
        let cond = uniq(0x430, 1);
        let ptr1 = uniq(0x431, 8);
        let ptr2 = uniq(0x432, 8);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x4200,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4200,
                            output: Some(ptr1.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4201,
                            output: None,
                            inputs: vec![cst(0, 4), ptr1, cst(5, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x4210,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntAdd,
                            address: 0x4210,
                            output: Some(ptr2.clone()),
                            inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::Store,
                            address: 0x4211,
                            output: None,
                            inputs: vec![cst(0, 4), ptr2, cst(6, 4)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x4220,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::Copy,
                            address: 0x4220,
                            output: Some(cond.clone()),
                            inputs: vec![reg(0x08, 1)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x4221,
                            output: None,
                            inputs: vec![cst(0x4200, 8), cond],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x4230,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x4230,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "do_while_chain_fn", 0x4200, &preview_options())
            .expect("preview render");
        assert!(code.contains("do {"));
        assert!(code.contains("local_10 = 5;"));
        assert!(code.contains("local_14 = 6;"));
        assert!(code.contains("} while (param_1);"));
    }

    #[test]
    fn normalize_bool_compare_to_zero() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(HirExpr::Var("flag_a".to_string())),
                rhs: Box::new(HirExpr::Var("flag_b".to_string())),
                ty: NirType::Bool,
            }),
            rhs: Box::new(HirExpr::Const(0, NirType::Int { bits: 32, signed: false })),
            ty: NirType::Bool,
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return flag_a && flag_b;");
    }

    #[test]
    fn normalize_trivial_integer_identities() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("param_1".to_string())),
            rhs: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return param_1;");
    }

    #[test]
    fn normalize_full_mask_and_wrapper() {
        let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 16,
                    signed: false,
                },
                expr: Box::new(HirExpr::Var("uVar1".to_string())),
            }),
            rhs: Box::new(HirExpr::Const(
                0xffff,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 16,
                signed: false,
            },
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return (ushort)uVar1;");
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
    fn multi_block_preview_lowers_canonical_switch_chain() {
        let cond0 = uniq(0x500, 1);
        let cond1 = uniq(0x501, 1);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x5000,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntEqual,
                            address: 0x5000,
                            output: Some(cond0.clone()),
                            inputs: vec![reg(0x08, 4), cst(1, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x5001,
                            output: None,
                            inputs: vec![cst(0x5030, 8), cond0],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x5010,
                    ops: vec![
                        PcodeOp {
                            seq_num: 0,
                            opcode: PcodeOpcode::IntEqual,
                            address: 0x5010,
                            output: Some(cond1.clone()),
                            inputs: vec![reg(0x08, 4), cst(2, 4)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 1,
                            opcode: PcodeOpcode::CBranch,
                            address: 0x5011,
                            output: None,
                            inputs: vec![cst(0x5040, 8), cond1],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x5020,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x5020,
                        output: None,
                        inputs: vec![cst(0, 8), cst(0, 4)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x5030,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x5030,
                        output: None,
                        inputs: vec![cst(0, 8), cst(1, 4)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 4,
                    start_address: 0x5040,
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Return,
                        address: 0x5040,
                        output: None,
                        inputs: vec![cst(0, 8), cst(2, 4)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        };

        let code = render_mlil_preview(&func, "switchy", 0x5000, &preview_options())
            .expect("preview render");
        assert!(code.contains("switch (param_1) {"));
        assert!(code.contains("case 1:"));
        assert!(code.contains("case 2:"));
        assert!(code.contains("default:"));
    }
