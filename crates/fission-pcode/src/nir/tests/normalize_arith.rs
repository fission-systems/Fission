use super::*;

#[test]
fn preview_prints_direct_srem_as_mod() {
    let result = uniq(0x200, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x2000,
            successors: vec![],
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

    let code =
        render_mlil_preview(&func, "mod_ll", 0x2000, &preview_options()).expect("preview render");
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
fn signed_mod_idiom_with_invalid_shift_does_not_panic() {
    let base = HirExpr::Var("param_1".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
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
                            0,
                            NirType::Int {
                                bits: 64,
                                signed: true,
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
                    -1,
                    NirType::Int {
                        bits: 64,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
            }),
            rhs: Box::new(HirExpr::Const(
                -1,
                NirType::Int {
                    bits: 64,
                    signed: true,
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
    }));
    normalize_stmt(&mut stmt);
    assert!(print_stmt(&stmt).contains("<< -1"));
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
fn arithmetic_identity_removes_div_by_one() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: Box::new(HirExpr::Var("uVar1".to_string())),
        rhs: Box::new(HirExpr::Const(
            1,
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
    assert_eq!(print_stmt(&stmt), "return uVar1;");
}

#[test]
fn arithmetic_identity_collapses_mod_by_one_to_zero() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: Box::new(HirExpr::Var("uVar1".to_string())),
        rhs: Box::new(HirExpr::Const(
            1,
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
    assert_eq!(print_stmt(&stmt), "return 0;");
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
fn high_part_extract_canonicalizes_to_shift_and_cast() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Cast {
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        expr: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs: Box::new(HirExpr::Var("wide".to_string())),
            rhs: Box::new(HirExpr::Const(
                32,
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
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return (uint)(wide >> 32);");
}

#[test]
fn wide_recombine_from_shifted_hi_and_cast_lo_collapses_to_source() {
    let source = HirExpr::Var("wide".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(source.clone()),
                    rhs: Box::new(HirExpr::Const(
                        32,
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
            }),
            rhs: Box::new(HirExpr::Const(
                32,
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
        rhs: Box::new(HirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(source.clone()),
        }),
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return (ulonglong)wide;");
}

#[test]
fn wide_recombine_from_shifted_hi_and_masked_lo_collapses_to_source() {
    let source = HirExpr::Var("wide".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Shr,
                    lhs: Box::new(source.clone()),
                    rhs: Box::new(HirExpr::Const(
                        32,
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
            }),
            rhs: Box::new(HirExpr::Const(
                32,
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
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(source.clone()),
            rhs: Box::new(HirExpr::Const(
                0xffff_ffff,
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
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return (ulonglong)wide;");
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
fn normalize_bool_compare_to_zero() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
            lhs: Box::new(HirExpr::Var("flag_a".to_string())),
            rhs: Box::new(HirExpr::Var("flag_b".to_string())),
            ty: NirType::Bool,
        }),
        rhs: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )),
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
fn flag_intrinsics_zero_fold_to_false() {
    for target in ["__carry", "__scarry", "__sborrow"] {
        let mut stmt = HirStmt::Return(Some(HirExpr::Call {
            target: target.to_string(),
            args: vec![
                HirExpr::Var("x".to_string()),
                HirExpr::Const(
                    0,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            ],
            ty: NirType::Bool,
        }));
        normalize_stmt(&mut stmt);
        assert_eq!(print_stmt(&stmt), "return 0;");
    }
}

#[test]
fn carry_intrinsic_constant_canonicalizes_to_unsigned_compare() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Call {
        target: "__carry".to_string(),
        args: vec![
            HirExpr::Var("uVar1".to_string()),
            HirExpr::Const(
                4,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        ],
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return 4294967292 <= uVar1;");
}

#[test]
fn sborrow_compare_canonicalizes_to_signed_less_than() {
    let a = HirExpr::Var("a".to_string());
    let b = HirExpr::Var("b".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(HirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::SLt,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            rhs: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return a < b;");
}

#[test]
fn sborrow_compare_canonicalizes_to_signed_less_equal() {
    let a = HirExpr::Var("a".to_string());
    let b = HirExpr::Var("b".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(HirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::SLt,
            lhs: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return a <= b;");
}

#[test]
fn carry_intrinsic_with_non_constant_rhs_is_preserved() {
    let mut stmt = HirStmt::Return(Some(HirExpr::Call {
        target: "__carry".to_string(),
        args: vec![HirExpr::Var("x".to_string()), HirExpr::Var("y".to_string())],
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return __carry(x, y);");
}

#[test]
fn sborrow_compare_non_matching_shape_is_preserved() {
    let a = HirExpr::Var("a".to_string());
    let b = HirExpr::Var("b".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(HirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::SLt,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            rhs: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return __sborrow(a, b) != (a + b < 0);");
}

#[test]
fn normalize_hir_function_removes_dead_flag_intrinsic_temp() {
    let mut func = HirFunction {
        name: "flag_temp_cleanup".to_string(),
        params: vec![],
        locals: vec![NirBinding {
            name: "xVar1".to_string(),
            ty: NirType::Bool,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("xVar1".to_string()),
                rhs: HirExpr::Call {
                    target: "__scarry".to_string(),
                    args: vec![
                        HirExpr::Var("eax".to_string()),
                        HirExpr::Const(
                            4,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    ],
                    ty: NirType::Bool,
                },
            },
            HirStmt::Return(None),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    assert!(func.locals.is_empty());
    assert_eq!(func.body, vec![HirStmt::Return(None)]);
}

#[test]
fn self_equality_on_integer_like_value_collapses_to_true() {
    let reg = HirExpr::Var("reg".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(reg.clone()),
        rhs: Box::new(reg),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return 1;");
}

#[test]
fn self_inequality_on_integer_like_value_collapses_to_false() {
    let reg = HirExpr::Var("reg".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(reg.clone()),
        rhs: Box::new(reg),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return 0;");
}

#[test]
fn logical_and_with_self_equality_tautology_collapses() {
    let reg = HirExpr::Var("reg".to_string());
    let mut stmt = HirStmt::If {
        cond: HirExpr::Binary {
            op: HirBinaryOp::LogicalAnd,
            lhs: Box::new(HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(reg.clone()),
                ty: NirType::Bool,
            }),
            rhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(reg.clone()),
                rhs: Box::new(reg),
                ty: NirType::Bool,
            }),
            ty: NirType::Bool,
        },
        then_body: vec![HirStmt::Return(None)],
        else_body: vec![],
    };
    normalize_stmt(&mut stmt);
    match stmt {
        HirStmt::If { cond, .. } => assert_eq!(
            cond,
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("reg".to_string())),
                ty: NirType::Bool,
            }
        ),
        _ => panic!("expected if stmt"),
    }
}

#[test]
fn float_self_equality_is_not_folded() {
    let x = HirExpr::Var("fVar1".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(HirExpr::Cast {
            ty: NirType::Float { bits: 32 },
            expr: Box::new(x.clone()),
        }),
        rhs: Box::new(HirExpr::Cast {
            ty: NirType::Float { bits: 32 },
            expr: Box::new(x),
        }),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return (float)fVar1 == (float)fVar1;");
}

#[test]
fn repeated_integer_bitwise_identity_simplifies() {
    let x = HirExpr::Var("eax".to_string());
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(x.clone()),
            rhs: Box::new(x),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        rhs: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return eax == 0;");
}

#[test]
fn normalize_hir_function_inlines_multi_use_temp_within_single_if_condition() {
    let mut func = HirFunction {
        name: "inline_condition_temp".to_string(),
        params: vec![],
        locals: vec![NirBinding {
            name: "uVar1".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("uVar1".to_string()),
                rhs: HirExpr::Var("eax".to_string()),
            },
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::LogicalAnd,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Ne,
                        lhs: Box::new(HirExpr::Var("uVar1".to_string())),
                        rhs: Box::new(HirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        )),
                        ty: NirType::Bool,
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(HirExpr::Var("uVar1".to_string())),
                        rhs: Box::new(HirExpr::Var("uVar1".to_string())),
                        ty: NirType::Bool,
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Return(None)],
                else_body: vec![],
            },
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    assert!(func.locals.is_empty());
    let rendered = print_hir_function(&func);
    assert!(rendered.contains("if (eax)"), "rendered:\n{}", rendered);
}

#[test]
fn compound_arm_flag_canonicalizes_to_signed_greater_than() {
    let a = HirExpr::Var("a".to_string());
    let b = HirExpr::Var("b".to_string());
    let sub = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(a.clone()),
        rhs: Box::new(b.clone()),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    let ne = HirExpr::Binary {
        op: HirBinaryOp::Ne,
        lhs: Box::new(sub.clone()),
        rhs: Box::new(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )),
        ty: NirType::Bool,
    };
    let eq = HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::SLt,
            lhs: Box::new(sub.clone()),
            rhs: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        }),
        rhs: Box::new(HirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    };
    let mut stmt = HirStmt::Return(Some(HirExpr::Binary {
        op: HirBinaryOp::LogicalAnd,
        lhs: Box::new(ne),
        rhs: Box::new(eq),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_stmt(&stmt), "return b < a;");
}
