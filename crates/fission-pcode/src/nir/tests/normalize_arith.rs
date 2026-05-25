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

#[test]
fn normalize_double_precision_reconstructs_32bit_arith_to_64bit() {
    let mut func = HirFunction {
        name: "test_dp".to_string(),
        params: Vec::new(),
        locals: vec![
            NirBinding {
                name: "lo".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "hi".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "lo1".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "lo2".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "hi1".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "hi2".to_string(),
                ty: NirType::Int { bits: 32, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "carry".to_string(),
                ty: NirType::Bool,
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "val_64".to_string(),
                ty: NirType::Int { bits: 64, signed: false },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("lo".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("lo1".to_string())),
                    rhs: Box::new(HirExpr::Var("lo2".to_string())),
                    ty: NirType::Int { bits: 32, signed: false },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("carry".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Lt,
                    lhs: Box::new(HirExpr::Var("lo".to_string())),
                    rhs: Box::new(HirExpr::Var("lo1".to_string())),
                    ty: NirType::Bool,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("hi".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("hi1".to_string())),
                        rhs: Box::new(HirExpr::Var("hi2".to_string())),
                        ty: NirType::Int { bits: 32, signed: false },
                    }),
                    rhs: Box::new(HirExpr::Cast {
                        ty: NirType::Int { bits: 32, signed: false },
                        expr: Box::new(HirExpr::Var("carry".to_string())),
                    }),
                    ty: NirType::Int { bits: 32, signed: false },
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("val_64".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Or,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(HirExpr::Cast {
                            ty: NirType::Int { bits: 64, signed: false },
                            expr: Box::new(HirExpr::Var("hi".to_string())),
                        }),
                        rhs: Box::new(HirExpr::Const(32, NirType::Int { bits: 32, signed: false })),
                        ty: NirType::Int { bits: 64, signed: false },
                    }),
                    rhs: Box::new(HirExpr::Cast {
                        ty: NirType::Int { bits: 64, signed: false },
                        expr: Box::new(HirExpr::Var("lo".to_string())),
                    }),
                    ty: NirType::Int { bits: 64, signed: false },
                },
            },
            HirStmt::Return(Some(HirExpr::Var("val_64".to_string()))),
        ],
        is_64bit: false,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    // Should collapse the carry addition lo1+lo2 and hi1+hi2+carry into a single 64-bit addition!
    assert!(rendered.contains("+"), "rendered:\n{}", rendered);
}

/// Unit test: call `apply_variable_merge_pass` directly (bypasses full pipeline)
/// to verify that two u32 temps with disjoint live ranges get coalesced while
/// a named `result` variable that is returned is preserved.
///
/// Body (pre-pass):
///   temp1 = f();            // temp1 live: stmts 0–1
///   result = temp1;         // temp1 last use
///   temp2 = g();            // temp2 live: stmts 2–3
///   result = result + temp2; // temp2 last use; result live 1–4
///   return result;
///
/// Expected post-pass: temp1/temp2 merged (one dropped), result kept.
#[test]
fn normalize_variable_merge_coalesces_disjoint_variables() {
    use crate::nir::normalize::recovery::apply_variable_merge_pass;

    let u32_ty = NirType::Int { bits: 32, signed: false };
    let make_binding = |name: &str| NirBinding {
        name: name.to_string(),
        ty: u32_ty.clone(),
        surface_type_name: None,
        origin: None,
        initializer: None,
    };

    // Use opaque calls so constant-folding can't trivially eliminate the temps.
    let call_f = || HirExpr::Call {
        target: "f".to_string(),
        args: vec![],
        ty: u32_ty.clone(),
    };
    let call_g = || HirExpr::Call {
        target: "g".to_string(),
        args: vec![],
        ty: u32_ty.clone(),
    };

    let mut func = HirFunction {
        name: "test_merge".to_string(),
        params: vec![],
        locals: vec![
            make_binding("temp1"),
            make_binding("temp2"),
            make_binding("result"),
        ],
        body: vec![
            // stmt 0: temp1 = f()
            HirStmt::Assign {
                lhs: HirLValue::Var("temp1".to_string()),
                rhs: call_f(),
            },
            // stmt 1: result = temp1  (temp1's last use)
            HirStmt::Assign {
                lhs: HirLValue::Var("result".to_string()),
                rhs: HirExpr::Var("temp1".to_string()),
            },
            // stmt 2: temp2 = g()  (temp2's first use)
            HirStmt::Assign {
                lhs: HirLValue::Var("temp2".to_string()),
                rhs: call_g(),
            },
            // stmt 3: result = result + temp2  (temp2's last use)
            HirStmt::Assign {
                lhs: HirLValue::Var("result".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("result".to_string())),
                    rhs: Box::new(HirExpr::Var("temp2".to_string())),
                    ty: u32_ty.clone(),
                },
            },
            // stmt 4: return result
            HirStmt::Return(Some(HirExpr::Var("result".to_string()))),
        ],
        is_64bit: false,
        ..Default::default()
    };

    // Call the pass directly — no pipeline interference.
    let changed = apply_variable_merge_pass(&mut func);
    let names: Vec<&str> = func.locals.iter().map(|l| l.name.as_str()).collect();
    println!("LOCALS after merge pass: {:?}", names);
    println!("changed={}", changed);

    // The pass must report that it did something.
    assert!(changed, "apply_variable_merge_pass should return true");
    // result must survive: it's returned and has highest name priority.
    assert!(names.contains(&"result"), "result should survive: {:?}", names);
    // temp1 and temp2 are disjoint u32s — exactly one should remain.
    assert!(
        func.locals.len() < 3,
        "expected temp1/temp2 to be merged (one dropped), got: {:?}", names
    );
}

#[test]
fn normalize_conditional_const_propagates_equality_branches() {
    use crate::nir::normalize::global_opt::apply_conditional_const_pass;

    let u32_ty = NirType::Int { bits: 32, signed: false };
    let mut func = HirFunction {
        name: "test_cond_const".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "y".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(5, u32_ty.clone())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("y".to_string()),
                        rhs: HirExpr::Var("x".to_string()),
                    },
                ],
                else_body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("y".to_string()),
                        rhs: HirExpr::Var("x".to_string()),
                    },
                ],
            },
        ],
        is_64bit: false,
        ..Default::default()
    };

    let changed = apply_conditional_const_pass(&mut func);
    assert!(changed);

    // In then_body, y = x should become y = 5
    if let HirStmt::If { then_body, else_body, .. } = &func.body[0] {
        if let HirStmt::Assign { rhs, .. } = &then_body[0] {
            assert!(matches!(rhs, HirExpr::Const(5, _)));
        } else {
            panic!("Expected Assign in then_body");
        }
        if let HirStmt::Assign { rhs, .. } = &else_body[0] {
            assert!(matches!(rhs, HirExpr::Var(name) if name == "x"));
        } else {
            panic!("Expected Assign in else_body");
        }
    } else {
        panic!("Expected If statement");
    }
}

#[test]
fn normalize_three_way_compare_simplifies_to_relational() {
    use crate::nir::normalize::arith::apply_three_way_compare_pass;

    let u32_ty = NirType::Int { bits: 32, signed: false };
    // Pattern: (zext(a < b) + zext(a <= b) - 1) == 0  =>  a == b
    let mut func = HirFunction {
        name: "test_three_way".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "a".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "b".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "result".to_string(),
                ty: NirType::Bool,
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("result".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Cast {
                                expr: Box::new(HirExpr::Binary {
                                    op: HirBinaryOp::Lt,
                                    lhs: Box::new(HirExpr::Var("a".to_string())),
                                    rhs: Box::new(HirExpr::Var("b".to_string())),
                                    ty: NirType::Bool,
                                }),
                                ty: u32_ty.clone(),
                            }),
                            rhs: Box::new(HirExpr::Cast {
                                expr: Box::new(HirExpr::Binary {
                                    op: HirBinaryOp::Le,
                                    lhs: Box::new(HirExpr::Var("a".to_string())),
                                    rhs: Box::new(HirExpr::Var("b".to_string())),
                                    ty: NirType::Bool,
                                }),
                                ty: u32_ty.clone(),
                            }),
                            ty: u32_ty.clone(),
                        }),
                        rhs: Box::new(HirExpr::Const(-1, u32_ty.clone())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Const(0, u32_ty.clone())),
                    ty: NirType::Bool,
                },
            },
        ],
        is_64bit: false,
        ..Default::default()
    };

    let changed = apply_three_way_compare_pass(&mut func);
    assert!(changed);

    if let HirStmt::Assign { rhs, .. } = &func.body[0] {
        if let HirExpr::Binary { op: HirBinaryOp::Eq, lhs, rhs, .. } = rhs {
            assert!(matches!(lhs.as_ref(), HirExpr::Var(name) if name == "a"));
            assert!(matches!(rhs.as_ref(), HirExpr::Var(name) if name == "b"));
        } else {
            panic!("Expected Eq binary expression");
        }
    } else {
        panic!("Expected Assign statement");
    }
}

#[test]
fn normalize_conditional_move() {
    use crate::nir::normalize::arith::apply_conditional_move_pass;

    let mut func = HirFunction {
        name: "test_cmov".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "x".to_string(),
                ty: NirType::Int { bits: 32, signed: true },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // Scenario 1: If-Then-Else
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("x".to_string()),
                    rhs: HirExpr::Const(10, NirType::Int { bits: 32, signed: true }),
                }],
                else_body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("x".to_string()),
                    rhs: HirExpr::Const(20, NirType::Int { bits: 32, signed: true }),
                }],
            },
            // Scenario 2: Default-Override
            HirStmt::Assign {
                lhs: HirLValue::Var("x".to_string()),
                rhs: HirExpr::Const(20, NirType::Int { bits: 32, signed: true }),
            },
            HirStmt::If {
                cond: HirExpr::Var("cond".to_string()),
                then_body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("x".to_string()),
                    rhs: HirExpr::Const(10, NirType::Int { bits: 32, signed: true }),
                }],
                else_body: vec![],
            },
        ],
        is_64bit: false,
        ..Default::default()
    };

    let changed = apply_conditional_move_pass(&mut func);
    assert!(changed);
    assert_eq!(func.body.len(), 2);

    // Verify Scenario 1 became Select
    if let HirStmt::Assign { lhs, rhs } = &func.body[0] {
        assert!(matches!(lhs, HirLValue::Var(name) if name == "x"));
        if let HirExpr::Select { cond, then_expr, else_expr, .. } = rhs {
            assert!(matches!(cond.as_ref(), HirExpr::Var(name) if name == "cond"));
            assert!(matches!(then_expr.as_ref(), HirExpr::Const(10, _)));
            assert!(matches!(else_expr.as_ref(), HirExpr::Const(20, _)));
        } else {
            panic!("Expected Select expression for scenario 1");
        }
    } else {
        panic!("Expected Assign statement for scenario 1");
    }

    // Verify Scenario 2 became Select
    if let HirStmt::Assign { lhs, rhs } = &func.body[1] {
        assert!(matches!(lhs, HirLValue::Var(name) if name == "x"));
        if let HirExpr::Select { cond, then_expr, else_expr, .. } = rhs {
            assert!(matches!(cond.as_ref(), HirExpr::Var(name) if name == "cond"));
            assert!(matches!(then_expr.as_ref(), HirExpr::Const(10, _)));
            assert!(matches!(else_expr.as_ref(), HirExpr::Const(20, _)));
        } else {
            panic!("Expected Select expression for scenario 2");
        }
    } else {
        panic!("Expected Assign statement for scenario 2");
    }
}

#[test]
fn subfloat_flow_narrowing_elides_redundant_casts() {
    use crate::nir::normalize::arith::apply_subfloat_flow_pass;

    let float32 = NirType::Float { bits: 32 };
    let float64 = NirType::Float { bits: 64 };

    let mut func = HirFunction {
        name: "test_subfloat".to_string(),
        params: vec![
            NirBinding {
                name: "x".to_string(),
                ty: float32.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "y".to_string(),
                ty: float32.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![
            NirBinding {
                name: "res".to_string(),
                ty: float32.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res = (float)((double)x + (double)y)
            HirStmt::Assign {
                lhs: HirLValue::Var("res".to_string()),
                rhs: HirExpr::Cast {
                    ty: float32.clone(),
                    expr: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Cast {
                            ty: float64.clone(),
                            expr: Box::new(HirExpr::Var("x".to_string())),
                        }),
                        rhs: Box::new(HirExpr::Cast {
                            ty: float64.clone(),
                            expr: Box::new(HirExpr::Var("y".to_string())),
                        }),
                        ty: float64.clone(),
                    }),
                },
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_subfloat_flow_pass(&mut func));
    assert_eq!(func.body.len(), 1);

    // Expected: res = x + y (all casts elided, math is float32)
    let HirStmt::Assign { rhs, .. } = &func.body[0] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs {
        assert_eq!(*op, HirBinaryOp::Add);
        assert_eq!(ty, &float32);
        assert_eq!(lhs.as_ref(), &HirExpr::Var("x".to_string()));
        assert_eq!(rhs.as_ref(), &HirExpr::Var("y".to_string()));
    } else {
        panic!("expected direct float addition without casts, got {:?}", rhs);
    }
}

#[test]
fn normalize_or_compare_simplifies_zero_comparisons() {
    use crate::nir::normalize::arith::apply_or_compare_pass;

    let u32_ty = NirType::Int { bits: 32, signed: false };
    let bool_ty = NirType::Bool;

    let mut func = HirFunction {
        name: "test_or_compare".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "a".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "b".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "c".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res1".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res2".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res3".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = ((a | b) == 0)
            HirStmt::Assign {
                lhs: HirLValue::Var("res1".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Or,
                        lhs: Box::new(HirExpr::Var("a".to_string())),
                        rhs: Box::new(HirExpr::Var("b".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Const(0, u32_ty.clone())),
                    ty: bool_ty.clone(),
                },
            },
            // res2 = (0 != (a | b))
            HirStmt::Assign {
                lhs: HirLValue::Var("res2".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Const(0, u32_ty.clone())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Or,
                        lhs: Box::new(HirExpr::Var("a".to_string())),
                        rhs: Box::new(HirExpr::Var("b".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res3 = (((a | b) | c) == 0)
            HirStmt::Assign {
                lhs: HirLValue::Var("res3".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Or,
                        lhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Or,
                            lhs: Box::new(HirExpr::Var("a".to_string())),
                            rhs: Box::new(HirExpr::Var("b".to_string())),
                            ty: u32_ty.clone(),
                        }),
                        rhs: Box::new(HirExpr::Var("c".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Const(0, u32_ty.clone())),
                    ty: bool_ty.clone(),
                },
            },
        ],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        calling_convention: Default::default(),
        is_64bit: true,
        suppress_entry_register_params: false,
        callee_observed_max_arity: Default::default(),
        callee_summaries: Default::default(),
    };

    assert!(apply_or_compare_pass(&mut func));
    assert_eq!(func.body.len(), 3);

    // Verify first simplification: res1 = ((a == 0) && (b == 0))
    let HirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs1 {
        assert_eq!(*op, HirBinaryOp::LogicalAnd);
        assert_eq!(ty, &bool_ty);
        
        let HirExpr::Binary { op: op_l, lhs: lhs_l, rhs: rhs_l, ty: ty_l } = lhs.as_ref() else { panic!(); };
        assert_eq!(*op_l, HirBinaryOp::Eq);
        assert_eq!(lhs_l.as_ref(), &HirExpr::Var("a".to_string()));
        assert!(matches!(rhs_l.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_l, &bool_ty);

        let HirExpr::Binary { op: op_r, lhs: lhs_r, rhs: rhs_r, ty: ty_r } = rhs.as_ref() else { panic!(); };
        assert_eq!(*op_r, HirBinaryOp::Eq);
        assert_eq!(lhs_r.as_ref(), &HirExpr::Var("b".to_string()));
        assert!(matches!(rhs_r.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);
    } else {
        panic!("expected logical AND, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = ((a != 0) || (b != 0))
    let HirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs2 {
        assert_eq!(*op, HirBinaryOp::LogicalOr);
        assert_eq!(ty, &bool_ty);
        
        let HirExpr::Binary { op: op_l, lhs: lhs_l, rhs: rhs_l, ty: ty_l } = lhs.as_ref() else { panic!(); };
        assert_eq!(*op_l, HirBinaryOp::Ne);
        assert_eq!(lhs_l.as_ref(), &HirExpr::Var("a".to_string()));
        assert!(matches!(rhs_l.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_l, &bool_ty);

        let HirExpr::Binary { op: op_r, lhs: lhs_r, rhs: rhs_r, ty: ty_r } = rhs.as_ref() else { panic!(); };
        assert_eq!(*op_r, HirBinaryOp::Ne);
        assert_eq!(lhs_r.as_ref(), &HirExpr::Var("b".to_string()));
        assert!(matches!(rhs_r.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);
    } else {
        panic!("expected logical OR, got {:?}", rhs2);
    }

    // Verify third simplification (nested OR): res3 = (((a == 0) && (b == 0)) && (c == 0))
    let HirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs3 {
        assert_eq!(*op, HirBinaryOp::LogicalAnd);
        assert_eq!(ty, &bool_ty);

        // Right side should be (c == 0)
        let HirExpr::Binary { op: op_r, lhs: lhs_r, rhs: rhs_r, ty: ty_r } = rhs.as_ref() else { panic!(); };
        assert_eq!(*op_r, HirBinaryOp::Eq);
        assert_eq!(lhs_r.as_ref(), &HirExpr::Var("c".to_string()));
        assert!(matches!(rhs_r.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);

        // Left side should be ((a == 0) && (b == 0))
        let HirExpr::Binary { op: op_l, lhs: lhs_l, rhs: rhs_l, ty: ty_l } = lhs.as_ref() else { panic!(); };
        assert_eq!(*op_l, HirBinaryOp::LogicalAnd);
        assert_eq!(ty_l, &bool_ty);

        let HirExpr::Binary { op: op_ll, lhs: lhs_ll, rhs: rhs_ll, ty: ty_ll } = lhs_l.as_ref() else { panic!(); };
        assert_eq!(*op_ll, HirBinaryOp::Eq);
        assert_eq!(lhs_ll.as_ref(), &HirExpr::Var("a".to_string()));
        assert!(matches!(rhs_ll.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_ll, &bool_ty);

        let HirExpr::Binary { op: op_lr, lhs: lhs_lr, rhs: rhs_lr, ty: ty_lr } = rhs_l.as_ref() else { panic!(); };
        assert_eq!(*op_lr, HirBinaryOp::Eq);
        assert_eq!(lhs_lr.as_ref(), &HirExpr::Var("b".to_string()));
        assert!(matches!(rhs_lr.as_ref(), HirExpr::Const(0, _)));
        assert_eq!(ty_lr, &bool_ty);
    } else {
        panic!("expected nested logical AND, got {:?}", rhs3);
    }
}

#[test]
fn normalize_float_sign_simplifies_manipulations() {
    use crate::nir::normalize::arith::apply_float_sign_pass;

    let f32_ty = NirType::Float { bits: 32 };
    let f64_ty = NirType::Float { bits: 64 };
    let i32_ty = NirType::Int { bits: 32, signed: false };
    let i64_ty = NirType::Int { bits: 64, signed: false };

    let mut func = HirFunction {
        name: "test_float_sign".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "f32_var".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "f64_var".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res1".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res2".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res3".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res4".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = (f32_var & 0x7fffffff)
            HirStmt::Assign {
                lhs: HirLValue::Var("res1".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("f32_var".to_string())),
                    rhs: Box::new(HirExpr::Const(0x7fffffff, i32_ty.clone())),
                    ty: i32_ty.clone(),
                },
            },
            // res2 = (f32_var ^ 0x80000000)
            HirStmt::Assign {
                lhs: HirLValue::Var("res2".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Xor,
                    lhs: Box::new(HirExpr::Var("f32_var".to_string())),
                    rhs: Box::new(HirExpr::Const(0x80000000, i32_ty.clone())),
                    ty: i32_ty.clone(),
                },
            },
            // res3 = (f64_var & 0x7fffffffffffffff)
            HirStmt::Assign {
                lhs: HirLValue::Var("res3".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("f64_var".to_string())),
                    rhs: Box::new(HirExpr::Const(0x7fffffffffffffff, i64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
            // res4 = (f64_var ^ 0x8000000000000000)
            HirStmt::Assign {
                lhs: HirLValue::Var("res4".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Xor,
                    lhs: Box::new(HirExpr::Var("f64_var".to_string())),
                    rhs: Box::new(HirExpr::Const(i64::MIN, i64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_float_sign_pass(&mut func));

    // Verify first simplification: res1 = fabsf(f32_var)
    let HirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else { panic!(); };
    if let HirExpr::Call { target, args, ty } = rhs1 {
        assert_eq!(target, "fabsf");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], HirExpr::Var("f32_var".to_string()));
        assert_eq!(ty, &f32_ty);
    } else {
        panic!("expected call, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = -f32_var
    let HirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else { panic!(); };
    if let HirExpr::Unary { op, expr, ty } = rhs2 {
        assert_eq!(*op, HirUnaryOp::Neg);
        assert_eq!(expr.as_ref(), &HirExpr::Var("f32_var".to_string()));
        assert_eq!(ty, &f32_ty);
    } else {
        panic!("expected unary, got {:?}", rhs2);
    }

    // Verify third simplification: res3 = fabs(f64_var)
    let HirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else { panic!(); };
    if let HirExpr::Call { target, args, ty } = rhs3 {
        assert_eq!(target, "fabs");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], HirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &f64_ty);
    } else {
        panic!("expected call, got {:?}", rhs3);
    }

    // Verify fourth simplification: res4 = -f64_var
    let HirStmt::Assign { rhs: rhs4, .. } = &func.body[3] else { panic!(); };
    if let HirExpr::Unary { op, expr, ty } = rhs4 {
        assert_eq!(*op, HirUnaryOp::Neg);
        assert_eq!(expr.as_ref(), &HirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &f64_ty);
    } else {
        panic!("expected unary, got {:?}", rhs4);
    }
}

#[test]
fn normalize_ignore_nan_simplifies_comparisons() {
    use crate::nir::normalize::arith::apply_ignore_nan_pass;

    let f32_ty = NirType::Float { bits: 32 };
    let f64_ty = NirType::Float { bits: 64 };
    let bool_ty = NirType::Bool;

    let mut func = HirFunction {
        name: "test_ignore_nan".to_string(),
        params: vec![],
        locals: vec![
            NirBinding {
                name: "f32_var".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "f64_var".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res1".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res2".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            NirBinding {
                name: "res3".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = !__isnan(f32_var) && (f32_var < 5.0f)
            HirStmt::Assign {
                lhs: HirLValue::Var("res1".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::LogicalAnd,
                    lhs: Box::new(HirExpr::Unary {
                        op: HirUnaryOp::Not,
                        expr: Box::new(HirExpr::Call {
                            target: "__isnan".to_string(),
                            args: vec![HirExpr::Var("f32_var".to_string())],
                            ty: bool_ty.clone(),
                        }),
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: Box::new(HirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(HirExpr::Const(5, f32_ty.clone())),
                        ty: bool_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res2 = __isnan(f32_var) || (f32_var != 5.0f)
            HirStmt::Assign {
                lhs: HirLValue::Var("res2".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::LogicalOr,
                    lhs: Box::new(HirExpr::Call {
                        target: "__isnan".to_string(),
                        args: vec![HirExpr::Var("f32_var".to_string())],
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Ne,
                        lhs: Box::new(HirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(HirExpr::Const(5, f32_ty.clone())),
                        ty: bool_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res3 = !__isnan(f32_var) && !__isnan(f64_var) && (f32_var < f64_var)
            HirStmt::Assign {
                lhs: HirLValue::Var("res3".to_string()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::LogicalAnd,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::LogicalAnd,
                        lhs: Box::new(HirExpr::Unary {
                            op: HirUnaryOp::Not,
                            expr: Box::new(HirExpr::Call {
                                target: "__isnan".to_string(),
                                args: vec![HirExpr::Var("f32_var".to_string())],
                                ty: bool_ty.clone(),
                            }),
                            ty: bool_ty.clone(),
                        }),
                        rhs: Box::new(HirExpr::Unary {
                            op: HirUnaryOp::Not,
                            expr: Box::new(HirExpr::Call {
                                target: "__isnan".to_string(),
                                args: vec![HirExpr::Var("f64_var".to_string())],
                                ty: bool_ty.clone(),
                            }),
                            ty: bool_ty.clone(),
                        }),
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Lt,
                        lhs: Box::new(HirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(HirExpr::Var("f64_var".to_string())),
                        ty: bool_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_ignore_nan_pass(&mut func));

    // Verify first simplification: res1 = (f32_var < 5.0f)
    let HirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs1 {
        assert_eq!(*op, HirBinaryOp::Lt);
        assert_eq!(lhs.as_ref(), &HirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &HirExpr::Const(5, f32_ty.clone()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = (f32_var != 5.0f)
    let HirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs2 {
        assert_eq!(*op, HirBinaryOp::Ne);
        assert_eq!(lhs.as_ref(), &HirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &HirExpr::Const(5, f32_ty.clone()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs2);
    }

    // Verify third simplification: res3 = (f32_var < f64_var)
    let HirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else { panic!(); };
    if let HirExpr::Binary { op, lhs, rhs, ty } = rhs3 {
        assert_eq!(*op, HirBinaryOp::Lt);
        assert_eq!(lhs.as_ref(), &HirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &HirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs3);
    }
}




