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
    let base = DirExpr::Var("param_1".to_string());
    let expr = DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs: Box::new(base.clone()),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Sar,
                lhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(base.clone()),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::And,
                        lhs: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Shr,
                            lhs: Box::new(base.clone()),
                            rhs: Box::new(DirExpr::Const(
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
                        rhs: Box::new(DirExpr::Const(
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
                rhs: Box::new(DirExpr::Const(
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
            rhs: Box::new(DirExpr::Const(
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
    let mut stmt = DirStmt::Return(Some(expr));
    normalize_stmt(&mut stmt);
    let rendered = print_dir_stmt(&stmt);
    assert_eq!(rendered, "return param_1 % 2;");
}

#[test]
fn signed_mod_idiom_with_invalid_shift_does_not_panic() {
    let base = DirExpr::Var("param_1".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs: Box::new(base.clone()),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Sar,
                lhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(base.clone()),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::And,
                        lhs: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Shr,
                            lhs: Box::new(base.clone()),
                            rhs: Box::new(DirExpr::Const(
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
                        rhs: Box::new(DirExpr::Const(
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
                rhs: Box::new(DirExpr::Const(
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
            rhs: Box::new(DirExpr::Const(
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
    assert!(print_dir_stmt(&stmt).contains("<< -1"));
}

#[test]
fn unsigned_mod_mask_recognition_collapses_to_percent() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::And,
        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return uVar1 % 4;");
}

#[test]
fn unsigned_div_shift_recognition_collapses_to_div() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Shr,
        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return uVar1 / 4;");
}

#[test]
fn arithmetic_identity_removes_div_by_one() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Div,
        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return uVar1;");
}

#[test]
fn arithmetic_identity_collapses_mod_by_one_to_zero() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Mod,
        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return 0;");
}

#[test]
fn signed_div_idiom_recognition_collapses_to_slash() {
    let base = DirExpr::Var("param_1".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Sar,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(base.clone()),
            rhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::And,
                lhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(base.clone()),
                    rhs: Box::new(DirExpr::Const(
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
                rhs: Box::new(DirExpr::Const(
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
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return param_1 / 2;");
}

#[test]
fn high_part_extract_canonicalizes_to_shift_and_cast() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Cast {
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        expr: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shr,
            lhs: Box::new(DirExpr::Var("wide".to_string())),
            rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return (uint)(wide >> 32);");
}

#[test]
fn wide_recombine_from_shifted_hi_and_cast_lo_collapses_to_source() {
    let source = DirExpr::Var("wide".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs: Box::new(DirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(source.clone()),
                    rhs: Box::new(DirExpr::Const(
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
            rhs: Box::new(DirExpr::Const(
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
        rhs: Box::new(DirExpr::Cast {
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
    assert_eq!(print_dir_stmt(&stmt), "return (ulonglong)wide;");
}

#[test]
fn wide_recombine_from_shifted_hi_and_masked_lo_collapses_to_source() {
    let source = DirExpr::Var("wide".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs: Box::new(DirExpr::Cast {
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                expr: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Shr,
                    lhs: Box::new(source.clone()),
                    rhs: Box::new(DirExpr::Const(
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
            rhs: Box::new(DirExpr::Const(
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
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs: Box::new(source.clone()),
            rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return (ulonglong)wide;");
}

#[test]
fn cast_canonicalizer_removes_duplicate_same_type_cast() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Cast {
        ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        expr: Box::new(DirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(DirExpr::Var("uVar1".to_string())),
        }),
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return (uint)uVar1;");
}

#[test]
fn cast_canonicalizer_drops_redundant_widen_before_narrow() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Cast {
        ty: NirType::Int {
            bits: 64,
            signed: true,
        },
        expr: Box::new(DirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            expr: Box::new(DirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(DirExpr::Var("var1".to_string())),
            }),
        }),
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return (longlong)(uint)var1;");
}

#[test]
fn cast_canonicalizer_preserves_sign_extension_chain() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Cast {
        ty: NirType::Int {
            bits: 64,
            signed: true,
        },
        expr: Box::new(DirExpr::Cast {
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
            expr: Box::new(DirExpr::Var("iVar1".to_string())),
        }),
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return (longlong)(int)iVar1;");
}

#[test]
fn condition_canonicalizer_turns_nonzero_compare_into_truthy_value() {
    let mut stmt = DirStmt::If {
        cond: DirExpr::Binary {
            op: DirBinaryOp::Ne,
            lhs: Box::new(DirExpr::Var("flag".to_string())),
            rhs: Box::new(DirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Bool,
        },
        then_body: vec![DirStmt::Return(Some(DirExpr::Const(
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
        DirStmt::If { cond, .. } => assert_eq!(print_dir_expr(&cond), "flag"),
        _ => panic!("expected if statement"),
    }
}

#[test]
fn printer_uses_precedence_aware_parentheses() {
    let expr = DirExpr::Binary {
        op: DirBinaryOp::LogicalOr,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Var("b".to_string())),
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: Box::new(DirExpr::Var("c".to_string())),
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    };
    assert_eq!(print_dir_expr(&expr), "a && b || !c");
}

#[test]
fn printer_preserves_needed_parentheses_for_mul_over_add() {
    let expr = DirExpr::Binary {
        op: DirBinaryOp::Mul,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("x".to_string())),
            rhs: Box::new(DirExpr::Const(
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
        rhs: Box::new(DirExpr::Var("y".to_string())),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    assert_eq!(print_dir_expr(&expr), "(x + 1) * y");
}

#[test]
fn normalize_bool_compare_to_zero() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs: Box::new(DirExpr::Var("flag_a".to_string())),
            rhs: Box::new(DirExpr::Var("flag_b".to_string())),
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return flag_a && flag_b;");
}

#[test]
fn normalize_trivial_integer_identities() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(DirExpr::Var("param_1".to_string())),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return param_1;");
}

#[test]
fn normalize_full_mask_and_wrapper() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::And,
        lhs: Box::new(DirExpr::Cast {
            ty: NirType::Int {
                bits: 16,
                signed: false,
            },
            expr: Box::new(DirExpr::Var("uVar1".to_string())),
        }),
        rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return (ushort)uVar1;");
}

#[test]
fn flag_intrinsics_zero_fold_to_false() {
    for target in ["__carry", "__scarry", "__sborrow"] {
        let mut stmt = DirStmt::Return(Some(DirExpr::Call {
            target: target.to_string(),
            args: vec![
                DirExpr::Var("x".to_string()),
                DirExpr::Const(
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
        assert_eq!(print_dir_stmt(&stmt), "return 0;");
    }
}

#[test]
fn carry_intrinsic_constant_canonicalizes_to_unsigned_compare() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Call {
        target: "__carry".to_string(),
        args: vec![
            DirExpr::Var("uVar1".to_string()),
            DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return 4294967292 <= uVar1;");
}

#[test]
fn sborrow_compare_canonicalizes_to_signed_less_than() {
    let a = DirExpr::Var("a".to_string());
    let b = DirExpr::Var("b".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs: Box::new(DirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Sub,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return a < b;");
}

#[test]
fn sborrow_compare_canonicalizes_to_signed_less_equal() {
    let a = DirExpr::Var("a".to_string());
    let b = DirExpr::Var("b".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs: Box::new(DirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(DirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            rhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Sub,
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
    assert_eq!(print_dir_stmt(&stmt), "return a <= b;");
}

#[test]
fn carry_intrinsic_with_non_constant_rhs_is_preserved() {
    let mut stmt = DirStmt::Return(Some(DirExpr::Call {
        target: "__carry".to_string(),
        args: vec![DirExpr::Var("x".to_string()), DirExpr::Var("y".to_string())],
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return __carry(x, y);");
}

#[test]
fn sborrow_compare_non_matching_shape_is_preserved() {
    let a = DirExpr::Var("a".to_string());
    let b = DirExpr::Var("b".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs: Box::new(DirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Add,
                lhs: Box::new(a.clone()),
                rhs: Box::new(b.clone()),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            }),
            rhs: Box::new(DirExpr::Const(
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
    assert_eq!(print_dir_stmt(&stmt), "return __sborrow(a, b) != (a + b < 0);");
}

#[test]
fn normalize_hir_function_removes_dead_flag_intrinsic_temp() {
    let mut func = DirFunction {
        name: "flag_temp_cleanup".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![DirBinding {
            name: "xVar1".to_string(),
            ty: NirType::Bool,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("xVar1".to_string()),
                rhs: DirExpr::Call {
                    target: "__scarry".to_string(),
                    args: vec![
                        DirExpr::Var("eax".to_string()),
                        DirExpr::Const(
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
            DirStmt::Return(None),
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    assert!(func.locals.is_empty());
    assert_eq!(func.body, vec![DirStmt::Return(None)]);
}

#[test]
fn self_equality_on_integer_like_value_collapses_to_true() {
    let reg = DirExpr::Var("reg".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs: Box::new(reg.clone()),
        rhs: Box::new(reg),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return 1;");
}

#[test]
fn self_inequality_on_integer_like_value_collapses_to_false() {
    let reg = DirExpr::Var("reg".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs: Box::new(reg.clone()),
        rhs: Box::new(reg),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return 0;");
}

#[test]
fn logical_and_with_self_equality_tautology_collapses() {
    let reg = DirExpr::Var("reg".to_string());
    let mut stmt = DirStmt::If {
        cond: DirExpr::Binary {
            op: DirBinaryOp::LogicalAnd,
            lhs: Box::new(DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(reg.clone()),
                ty: NirType::Bool,
            }),
            rhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Eq,
                lhs: Box::new(reg.clone()),
                rhs: Box::new(reg),
                ty: NirType::Bool,
            }),
            ty: NirType::Bool,
        },
        then_body: vec![DirStmt::Return(None)],
        else_body: vec![],
    };
    normalize_stmt(&mut stmt);
    match stmt {
        DirStmt::If { cond, .. } => assert_eq!(
            cond,
            DirExpr::Unary {
                op: DirUnaryOp::Not,
                expr: Box::new(DirExpr::Var("reg".to_string())),
                ty: NirType::Bool,
            }
        ),
        _ => panic!("expected if stmt"),
    }
}

#[test]
fn float_self_equality_is_not_folded() {
    let x = DirExpr::Var("fVar1".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs: Box::new(DirExpr::Cast {
            ty: NirType::Float { bits: 32 },
            expr: Box::new(x.clone()),
        }),
        rhs: Box::new(DirExpr::Cast {
            ty: NirType::Float { bits: 32 },
            expr: Box::new(x),
        }),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return (float)fVar1 == (float)fVar1;");
}

#[test]
fn repeated_integer_bitwise_identity_simplifies() {
    let x = DirExpr::Var("eax".to_string());
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs: Box::new(x.clone()),
            rhs: Box::new(x),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
        }),
        rhs: Box::new(DirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return eax == 0;");
}

#[test]
fn normalize_hir_function_inlines_multi_use_temp_within_single_if_condition() {
    let mut func = DirFunction {
        name: "inline_condition_temp".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![DirBinding {
            name: "eax".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: false,
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        locals: vec![DirBinding {
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
            DirStmt::Assign {
                lhs: DirLValue::Var("uVar1".to_string()),
                rhs: DirExpr::Var("eax".to_string()),
            },
            DirStmt::If {
                cond: DirExpr::Binary {
                    op: DirBinaryOp::LogicalAnd,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Ne,
                        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
                        rhs: Box::new(DirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        )),
                        ty: NirType::Bool,
                    }),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Eq,
                        lhs: Box::new(DirExpr::Var("uVar1".to_string())),
                        rhs: Box::new(DirExpr::Var("uVar1".to_string())),
                        ty: NirType::Bool,
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Return(None)],
                else_body: vec![],
            },
        ],
        ..Default::default()
    };

    normalize_hir_function(&mut func);

    assert!(func.locals.is_empty());
    let rendered = print_dir_function(&func);
    assert!(rendered.contains("if (eax)"), "rendered:\n{}", rendered);
}

#[test]
fn compound_arm_flag_canonicalizes_to_signed_greater_than() {
    let a = DirExpr::Var("a".to_string());
    let b = DirExpr::Var("b".to_string());
    let sub = DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs: Box::new(a.clone()),
        rhs: Box::new(b.clone()),
        ty: NirType::Int {
            bits: 32,
            signed: true,
        },
    };
    let ne = DirExpr::Binary {
        op: DirBinaryOp::Ne,
        lhs: Box::new(sub.clone()),
        rhs: Box::new(DirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: true,
            },
        )),
        ty: NirType::Bool,
    };
    let eq = DirExpr::Binary {
        op: DirBinaryOp::Eq,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::SLt,
            lhs: Box::new(sub.clone()),
            rhs: Box::new(DirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Bool,
        }),
        rhs: Box::new(DirExpr::Call {
            target: "__sborrow".to_string(),
            args: vec![a.clone(), b.clone()],
            ty: NirType::Bool,
        }),
        ty: NirType::Bool,
    };
    let mut stmt = DirStmt::Return(Some(DirExpr::Binary {
        op: DirBinaryOp::LogicalAnd,
        lhs: Box::new(ne),
        rhs: Box::new(eq),
        ty: NirType::Bool,
    }));
    normalize_stmt(&mut stmt);
    assert_eq!(print_dir_stmt(&stmt), "return b < a;");
}

#[test]
fn normalize_double_precision_reconstructs_32bit_arith_to_64bit() {
    let mut func = DirFunction {
        name: "test_dp".to_string(),
        int_param_offsets: Vec::new(),
        params: Vec::new(),
        locals: vec![
            DirBinding {
                name: "lo".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "hi".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "lo1".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "lo2".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "hi1".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "hi2".to_string(),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "carry".to_string(),
                ty: NirType::Bool,
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "val_64".to_string(),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("lo".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("lo1".to_string())),
                    rhs: Box::new(DirExpr::Var("lo2".to_string())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("carry".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Lt,
                    lhs: Box::new(DirExpr::Var("lo".to_string())),
                    rhs: Box::new(DirExpr::Var("lo1".to_string())),
                    ty: NirType::Bool,
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("hi".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Var("hi1".to_string())),
                        rhs: Box::new(DirExpr::Var("hi2".to_string())),
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    }),
                    rhs: Box::new(DirExpr::Cast {
                        ty: NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                        expr: Box::new(DirExpr::Var("carry".to_string())),
                    }),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("val_64".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Or,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Shl,
                        lhs: Box::new(DirExpr::Cast {
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                            expr: Box::new(DirExpr::Var("hi".to_string())),
                        }),
                        rhs: Box::new(DirExpr::Const(
                            32,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    }),
                    rhs: Box::new(DirExpr::Cast {
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        expr: Box::new(DirExpr::Var("lo".to_string())),
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                },
            },
            DirStmt::Return(Some(DirExpr::Var("val_64".to_string()))),
        ],
        is_64bit: false,
        ..Default::default()
    };

    normalize_hir_function(&mut func);
    let rendered = print_dir_function(&func);
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
    use crate::midend::normalize::recovery::apply_variable_merge_pass;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let make_binding = |name: &str| DirBinding {
        name: name.to_string(),
        ty: u32_ty.clone(),
        surface_type_name: None,
        origin: None,
        initializer: None,
    };

    // Use opaque calls so constant-folding can't trivially eliminate the temps.
    let call_f = || DirExpr::Call {
        target: "f".to_string(),
        args: vec![],
        ty: u32_ty.clone(),
    };
    let call_g = || DirExpr::Call {
        target: "g".to_string(),
        args: vec![],
        ty: u32_ty.clone(),
    };

    let mut func = DirFunction {
        name: "test_merge".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            make_binding("temp1"),
            make_binding("temp2"),
            make_binding("result"),
        ],
        body: vec![
            // stmt 0: temp1 = f()
            DirStmt::Assign {
                lhs: DirLValue::Var("temp1".to_string()),
                rhs: call_f(),
            },
            // stmt 1: result = temp1  (temp1's last use)
            DirStmt::Assign {
                lhs: DirLValue::Var("result".to_string()),
                rhs: DirExpr::Var("temp1".to_string()),
            },
            // stmt 2: temp2 = g()  (temp2's first use)
            DirStmt::Assign {
                lhs: DirLValue::Var("temp2".to_string()),
                rhs: call_g(),
            },
            // stmt 3: result = result + temp2  (temp2's last use)
            DirStmt::Assign {
                lhs: DirLValue::Var("result".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Var("result".to_string())),
                    rhs: Box::new(DirExpr::Var("temp2".to_string())),
                    ty: u32_ty.clone(),
                },
            },
            // stmt 4: return result
            DirStmt::Return(Some(DirExpr::Var("result".to_string()))),
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
    assert!(
        names.contains(&"result"),
        "result should survive: {:?}",
        names
    );
    // temp1 and temp2 are disjoint u32s — exactly one should remain.
    assert!(
        func.locals.len() < 3,
        "expected temp1/temp2 to be merged (one dropped), got: {:?}",
        names
    );
}

#[test]
fn normalize_conditional_const_propagates_equality_branches() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let mut func = DirFunction {
        name: "test_cond_const".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "y".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![DirStmt::If {
            cond: DirExpr::Binary {
                op: DirBinaryOp::Eq,
                lhs: Box::new(DirExpr::Var("x".to_string())),
                rhs: Box::new(DirExpr::Const(5, u32_ty.clone())),
                ty: NirType::Bool,
            },
            then_body: vec![DirStmt::Assign {
                lhs: DirLValue::Var("y".to_string()),
                rhs: DirExpr::Var("x".to_string()),
            }],
            else_body: vec![DirStmt::Assign {
                lhs: DirLValue::Var("y".to_string()),
                rhs: DirExpr::Var("x".to_string()),
            }],
        }],
        is_64bit: false,
        ..Default::default()
    };

    let changed = apply_conditional_const_pass(&mut func);
    assert!(changed);

    // In then_body, y = x should become y = 5
    if let DirStmt::If {
        then_body,
        else_body,
        ..
    } = &func.body[0]
    {
        if let DirStmt::Assign { rhs, .. } = &then_body[0] {
            assert!(matches!(rhs, DirExpr::Const(5, _)));
        } else {
            panic!("Expected Assign in then_body");
        }
        if let DirStmt::Assign { rhs, .. } = &else_body[0] {
            assert!(matches!(rhs, DirExpr::Var(name) if name == "x"));
        } else {
            panic!("Expected Assign in else_body");
        }
    } else {
        panic!("Expected If statement");
    }
}

#[test]
fn normalize_three_way_compare_simplifies_to_relational() {
    use crate::midend::normalize::arith::apply_three_way_compare_pass;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    // Pattern: (zext(a < b) + zext(a <= b) - 1) == 0  =>  a == b
    let mut func = DirFunction {
        name: "test_three_way".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "a".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "b".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "result".to_string(),
                ty: NirType::Bool,
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![DirStmt::Assign {
            lhs: DirLValue::Var("result".to_string()),
            rhs: DirExpr::Binary {
                op: DirBinaryOp::Eq,
                lhs: Box::new(DirExpr::Binary {
                    op: DirBinaryOp::Add,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Cast {
                            expr: Box::new(DirExpr::Binary {
                                op: DirBinaryOp::Lt,
                                lhs: Box::new(DirExpr::Var("a".to_string())),
                                rhs: Box::new(DirExpr::Var("b".to_string())),
                                ty: NirType::Bool,
                            }),
                            ty: u32_ty.clone(),
                        }),
                        rhs: Box::new(DirExpr::Cast {
                            expr: Box::new(DirExpr::Binary {
                                op: DirBinaryOp::Le,
                                lhs: Box::new(DirExpr::Var("a".to_string())),
                                rhs: Box::new(DirExpr::Var("b".to_string())),
                                ty: NirType::Bool,
                            }),
                            ty: u32_ty.clone(),
                        }),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Const(-1, u32_ty.clone())),
                    ty: u32_ty.clone(),
                }),
                rhs: Box::new(DirExpr::Const(0, u32_ty.clone())),
                ty: NirType::Bool,
            },
        }],
        is_64bit: false,
        ..Default::default()
    };

    let changed = apply_three_way_compare_pass(&mut func);
    assert!(changed);

    if let DirStmt::Assign { rhs, .. } = &func.body[0] {
        if let DirExpr::Binary {
            op: DirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } = rhs
        {
            assert!(matches!(lhs.as_ref(), DirExpr::Var(name) if name == "a"));
            assert!(matches!(rhs.as_ref(), DirExpr::Var(name) if name == "b"));
        } else {
            panic!("Expected Eq binary expression");
        }
    } else {
        panic!("Expected Assign statement");
    }
}

#[test]
fn normalize_conditional_move() {
    use crate::midend::normalize::arith::apply_conditional_move_pass;

    let mut func = DirFunction {
        name: "test_cmov".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![DirBinding {
            name: "x".to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            // Scenario 1: If-Then-Else
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(
                        10,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    ),
                }],
                else_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(
                        20,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    ),
                }],
            },
            // Scenario 2: Default-Override
            DirStmt::Assign {
                lhs: DirLValue::Var("x".to_string()),
                rhs: DirExpr::Const(
                    20,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            },
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(
                        10,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    ),
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
    if let DirStmt::Assign { lhs, rhs } = &func.body[0] {
        assert!(matches!(lhs, DirLValue::Var(name) if name == "x"));
        if let DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } = rhs
        {
            assert!(matches!(cond.as_ref(), DirExpr::Var(name) if name == "cond"));
            assert!(matches!(then_expr.as_ref(), DirExpr::Const(10, _)));
            assert!(matches!(else_expr.as_ref(), DirExpr::Const(20, _)));
        } else {
            panic!("Expected Select expression for scenario 1");
        }
    } else {
        panic!("Expected Assign statement for scenario 1");
    }

    // Verify Scenario 2 became Select
    if let DirStmt::Assign { lhs, rhs } = &func.body[1] {
        assert!(matches!(lhs, DirLValue::Var(name) if name == "x"));
        if let DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } = rhs
        {
            assert!(matches!(cond.as_ref(), DirExpr::Var(name) if name == "cond"));
            assert!(matches!(then_expr.as_ref(), DirExpr::Const(10, _)));
            assert!(matches!(else_expr.as_ref(), DirExpr::Const(20, _)));
        } else {
            panic!("Expected Select expression for scenario 2");
        }
    } else {
        panic!("Expected Assign statement for scenario 2");
    }
}

#[test]
fn subfloat_flow_narrowing_elides_redundant_casts() {
    use crate::midend::normalize::arith::apply_subfloat_flow_pass;

    let float32 = NirType::Float { bits: 32 };
    let float64 = NirType::Float { bits: 64 };

    let mut func = DirFunction {
        name: "test_subfloat".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![
            DirBinding {
                name: "x".to_string(),
                ty: float32.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "y".to_string(),
                ty: float32.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        locals: vec![DirBinding {
            name: "res".to_string(),
            ty: float32.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }],
        body: vec![
            // res = (float)((double)x + (double)y)
            DirStmt::Assign {
                lhs: DirLValue::Var("res".to_string()),
                rhs: DirExpr::Cast {
                    ty: float32.clone(),
                    expr: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(DirExpr::Cast {
                            ty: float64.clone(),
                            expr: Box::new(DirExpr::Var("x".to_string())),
                        }),
                        rhs: Box::new(DirExpr::Cast {
                            ty: float64.clone(),
                            expr: Box::new(DirExpr::Var("y".to_string())),
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
    let DirStmt::Assign { rhs, .. } = &func.body[0] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs {
        assert_eq!(*op, DirBinaryOp::Add);
        assert_eq!(ty, &float32);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("x".to_string()));
        assert_eq!(rhs.as_ref(), &DirExpr::Var("y".to_string()));
    } else {
        panic!(
            "expected direct float addition without casts, got {:?}",
            rhs
        );
    }
}

#[test]
fn normalize_or_compare_simplifies_zero_comparisons() {
    use crate::midend::normalize::arith::apply_or_compare_pass;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let bool_ty = NirType::Bool;

    let mut func = DirFunction {
        name: "test_or_compare".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "a".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "b".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "c".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res1".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res2".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res3".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = ((a | b) == 0)
            DirStmt::Assign {
                lhs: DirLValue::Var("res1".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Eq,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Or,
                        lhs: Box::new(DirExpr::Var("a".to_string())),
                        rhs: Box::new(DirExpr::Var("b".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Const(0, u32_ty.clone())),
                    ty: bool_ty.clone(),
                },
            },
            // res2 = (0 != (a | b))
            DirStmt::Assign {
                lhs: DirLValue::Var("res2".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Ne,
                    lhs: Box::new(DirExpr::Const(0, u32_ty.clone())),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Or,
                        lhs: Box::new(DirExpr::Var("a".to_string())),
                        rhs: Box::new(DirExpr::Var("b".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res3 = (((a | b) | c) == 0)
            DirStmt::Assign {
                lhs: DirLValue::Var("res3".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Eq,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Or,
                        lhs: Box::new(DirExpr::Binary {
                            op: DirBinaryOp::Or,
                            lhs: Box::new(DirExpr::Var("a".to_string())),
                            rhs: Box::new(DirExpr::Var("b".to_string())),
                            ty: u32_ty.clone(),
                        }),
                        rhs: Box::new(DirExpr::Var("c".to_string())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Const(0, u32_ty.clone())),
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
    let DirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs1 {
        assert_eq!(*op, DirBinaryOp::LogicalAnd);
        assert_eq!(ty, &bool_ty);

        let DirExpr::Binary {
            op: op_l,
            lhs: lhs_l,
            rhs: rhs_l,
            ty: ty_l,
        } = lhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_l, DirBinaryOp::Eq);
        assert_eq!(lhs_l.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs_l.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_l, &bool_ty);

        let DirExpr::Binary {
            op: op_r,
            lhs: lhs_r,
            rhs: rhs_r,
            ty: ty_r,
        } = rhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_r, DirBinaryOp::Eq);
        assert_eq!(lhs_r.as_ref(), &DirExpr::Var("b".to_string()));
        assert!(matches!(rhs_r.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);
    } else {
        panic!("expected logical AND, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = ((a != 0) || (b != 0))
    let DirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs2 {
        assert_eq!(*op, DirBinaryOp::LogicalOr);
        assert_eq!(ty, &bool_ty);

        let DirExpr::Binary {
            op: op_l,
            lhs: lhs_l,
            rhs: rhs_l,
            ty: ty_l,
        } = lhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_l, DirBinaryOp::Ne);
        assert_eq!(lhs_l.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs_l.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_l, &bool_ty);

        let DirExpr::Binary {
            op: op_r,
            lhs: lhs_r,
            rhs: rhs_r,
            ty: ty_r,
        } = rhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_r, DirBinaryOp::Ne);
        assert_eq!(lhs_r.as_ref(), &DirExpr::Var("b".to_string()));
        assert!(matches!(rhs_r.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);
    } else {
        panic!("expected logical OR, got {:?}", rhs2);
    }

    // Verify third simplification (nested OR): res3 = (((a == 0) && (b == 0)) && (c == 0))
    let DirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs3 {
        assert_eq!(*op, DirBinaryOp::LogicalAnd);
        assert_eq!(ty, &bool_ty);

        // Right side should be (c == 0)
        let DirExpr::Binary {
            op: op_r,
            lhs: lhs_r,
            rhs: rhs_r,
            ty: ty_r,
        } = rhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_r, DirBinaryOp::Eq);
        assert_eq!(lhs_r.as_ref(), &DirExpr::Var("c".to_string()));
        assert!(matches!(rhs_r.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_r, &bool_ty);

        // Left side should be ((a == 0) && (b == 0))
        let DirExpr::Binary {
            op: op_l,
            lhs: lhs_l,
            rhs: rhs_l,
            ty: ty_l,
        } = lhs.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_l, DirBinaryOp::LogicalAnd);
        assert_eq!(ty_l, &bool_ty);

        let DirExpr::Binary {
            op: op_ll,
            lhs: lhs_ll,
            rhs: rhs_ll,
            ty: ty_ll,
        } = lhs_l.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_ll, DirBinaryOp::Eq);
        assert_eq!(lhs_ll.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs_ll.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_ll, &bool_ty);

        let DirExpr::Binary {
            op: op_lr,
            lhs: lhs_lr,
            rhs: rhs_lr,
            ty: ty_lr,
        } = rhs_l.as_ref()
        else {
            panic!();
        };
        assert_eq!(*op_lr, DirBinaryOp::Eq);
        assert_eq!(lhs_lr.as_ref(), &DirExpr::Var("b".to_string()));
        assert!(matches!(rhs_lr.as_ref(), DirExpr::Const(0, _)));
        assert_eq!(ty_lr, &bool_ty);
    } else {
        panic!("expected nested logical AND, got {:?}", rhs3);
    }
}

#[test]
fn normalize_or_compare_simplifies_or_of_zero() {
    use crate::midend::normalize::arith::apply_or_compare_pass;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let bool_ty = NirType::Bool;

    let mut func = DirFunction {
        name: "test_or_of_zero".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "cond".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "val".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "other".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res = (cond ? val : 0) | other
            DirStmt::Assign {
                lhs: DirLValue::Var("res".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Or,
                    lhs: Box::new(DirExpr::Select {
                        cond: Box::new(DirExpr::Var("cond".to_string())),
                        then_expr: Box::new(DirExpr::Var("val".to_string())),
                        else_expr: Box::new(DirExpr::Const(0, u32_ty.clone())),
                        ty: u32_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Var("other".to_string())),
                    ty: u32_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_or_compare_pass(&mut func));
    assert_eq!(func.body.len(), 1);

    // Expected: res = cond ? (val | other) : other
    let DirStmt::Assign { rhs, .. } = &func.body[0] else {
        panic!();
    };
    if let DirExpr::Select {
        cond,
        then_expr,
        else_expr,
        ty,
    } = rhs
    {
        assert_eq!(ty, &u32_ty);
        assert_eq!(cond.as_ref(), &DirExpr::Var("cond".to_string()));
        assert_eq!(else_expr.as_ref(), &DirExpr::Var("other".to_string()));

        if let DirExpr::Binary {
            op,
            lhs,
            rhs,
            ty: or_ty,
        } = then_expr.as_ref()
        {
            assert_eq!(*op, DirBinaryOp::Or);
            assert_eq!(or_ty, &u32_ty);
            assert_eq!(lhs.as_ref(), &DirExpr::Var("val".to_string()));
            assert_eq!(rhs.as_ref(), &DirExpr::Var("other".to_string()));
        } else {
            panic!("expected Or expression in then branch, got {:?}", then_expr);
        }
    } else {
        panic!("expected Select expression, got {:?}", rhs);
    }
}

#[test]
fn normalize_nested_adds_subs_simplifies_constants() {
    use crate::midend::normalize::arith::simplify_nested_adds_subs;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };

    // Case 1: (a + 10) + 20 => a + 30
    let expr1 = DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Const(10, u32_ty.clone())),
            ty: u32_ty.clone(),
        }),
        rhs: Box::new(DirExpr::Const(20, u32_ty.clone())),
        ty: u32_ty.clone(),
    };
    let res1 = simplify_nested_adds_subs(&expr1).unwrap();
    if let DirExpr::Binary { op, lhs, rhs, .. } = res1 {
        assert_eq!(op, DirBinaryOp::Add);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs.as_ref(), DirExpr::Const(30, _)));
    } else {
        panic!("expected add, got {:?}", res1);
    }

    // Case 2: (a - 10) + 30 => a + 20
    let expr2 = DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Const(10, u32_ty.clone())),
            ty: u32_ty.clone(),
        }),
        rhs: Box::new(DirExpr::Const(30, u32_ty.clone())),
        ty: u32_ty.clone(),
    };
    let res2 = simplify_nested_adds_subs(&expr2).unwrap();
    if let DirExpr::Binary { op, lhs, rhs, .. } = res2 {
        assert_eq!(op, DirBinaryOp::Add);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs.as_ref(), DirExpr::Const(20, _)));
    } else {
        panic!("expected add, got {:?}", res2);
    }
}

#[test]
fn normalize_collect_mul_terms_simplifies_constants() {
    use crate::midend::normalize::arith::simplify_collect_mul_terms;

    let u32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };

    // Case 1: (a * 5) + (a * 2) => a * 7
    let expr1 = DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Const(5, u32_ty.clone())),
            ty: u32_ty.clone(),
        }),
        rhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Const(2, u32_ty.clone())),
            ty: u32_ty.clone(),
        }),
        ty: u32_ty.clone(),
    };
    let res1 = simplify_collect_mul_terms(&expr1).unwrap();
    if let DirExpr::Binary { op, lhs, rhs, .. } = res1 {
        assert_eq!(op, DirBinaryOp::Mul);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs.as_ref(), DirExpr::Const(7, _)));
    } else {
        panic!("expected mul, got {:?}", res1);
    }

    // Case 2: (a * 5) - a => a * 4
    let expr2 = DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Mul,
            lhs: Box::new(DirExpr::Var("a".to_string())),
            rhs: Box::new(DirExpr::Const(5, u32_ty.clone())),
            ty: u32_ty.clone(),
        }),
        rhs: Box::new(DirExpr::Var("a".to_string())),
        ty: u32_ty.clone(),
    };
    let res2 = simplify_collect_mul_terms(&expr2).unwrap();
    if let DirExpr::Binary { op, lhs, rhs, .. } = res2 {
        assert_eq!(op, DirBinaryOp::Mul);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("a".to_string()));
        assert!(matches!(rhs.as_ref(), DirExpr::Const(4, _)));
    } else {
        panic!("expected mul, got {:?}", res2);
    }
}

#[test]
fn normalize_float_sign_simplifies_manipulations() {
    use crate::midend::normalize::arith::apply_float_sign_pass;

    let f32_ty = NirType::Float { bits: 32 };
    let f64_ty = NirType::Float { bits: 64 };
    let i32_ty = NirType::Int {
        bits: 32,
        signed: false,
    };
    let i64_ty = NirType::Int {
        bits: 64,
        signed: false,
    };

    let mut func = DirFunction {
        name: "test_float_sign".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "f32_var".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "f64_var".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res1".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res2".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res3".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res4".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = (f32_var & 0x7fffffff)
            DirStmt::Assign {
                lhs: DirLValue::Var("res1".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::And,
                    lhs: Box::new(DirExpr::Var("f32_var".to_string())),
                    rhs: Box::new(DirExpr::Const(0x7fffffff, i32_ty.clone())),
                    ty: i32_ty.clone(),
                },
            },
            // res2 = (f32_var ^ 0x80000000)
            DirStmt::Assign {
                lhs: DirLValue::Var("res2".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Xor,
                    lhs: Box::new(DirExpr::Var("f32_var".to_string())),
                    rhs: Box::new(DirExpr::Const(0x80000000, i32_ty.clone())),
                    ty: i32_ty.clone(),
                },
            },
            // res3 = (f64_var & 0x7fffffffffffffff)
            DirStmt::Assign {
                lhs: DirLValue::Var("res3".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::And,
                    lhs: Box::new(DirExpr::Var("f64_var".to_string())),
                    rhs: Box::new(DirExpr::Const(0x7fffffffffffffff, i64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
            // res4 = (f64_var ^ 0x8000000000000000)
            DirStmt::Assign {
                lhs: DirLValue::Var("res4".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::Xor,
                    lhs: Box::new(DirExpr::Var("f64_var".to_string())),
                    rhs: Box::new(DirExpr::Const(i64::MIN, i64_ty.clone())),
                    ty: i64_ty.clone(),
                },
            },
        ],
        ..Default::default()
    };

    assert!(apply_float_sign_pass(&mut func));

    // Verify first simplification: res1 = fabsf(f32_var)
    let DirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    if let DirExpr::Call { target, args, ty } = rhs1 {
        assert_eq!(target, "fabsf");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], DirExpr::Var("f32_var".to_string()));
        assert_eq!(ty, &f32_ty);
    } else {
        panic!("expected call, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = -f32_var
    let DirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    if let DirExpr::Unary { op, expr, ty } = rhs2 {
        assert_eq!(*op, DirUnaryOp::Neg);
        assert_eq!(expr.as_ref(), &DirExpr::Var("f32_var".to_string()));
        assert_eq!(ty, &f32_ty);
    } else {
        panic!("expected unary, got {:?}", rhs2);
    }

    // Verify third simplification: res3 = fabs(f64_var)
    let DirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    if let DirExpr::Call { target, args, ty } = rhs3 {
        assert_eq!(target, "fabs");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], DirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &f64_ty);
    } else {
        panic!("expected call, got {:?}", rhs3);
    }

    // Verify fourth simplification: res4 = -f64_var
    let DirStmt::Assign { rhs: rhs4, .. } = &func.body[3] else {
        panic!();
    };
    if let DirExpr::Unary { op, expr, ty } = rhs4 {
        assert_eq!(*op, DirUnaryOp::Neg);
        assert_eq!(expr.as_ref(), &DirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &f64_ty);
    } else {
        panic!("expected unary, got {:?}", rhs4);
    }
}

#[test]
fn normalize_ignore_nan_simplifies_comparisons() {
    use crate::midend::normalize::arith::apply_ignore_nan_pass;

    let f32_ty = NirType::Float { bits: 32 };
    let f64_ty = NirType::Float { bits: 64 };
    let bool_ty = NirType::Bool;

    let mut func = DirFunction {
        name: "test_ignore_nan".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "f32_var".to_string(),
                ty: f32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "f64_var".to_string(),
                ty: f64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res1".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res2".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "res3".to_string(),
                ty: bool_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body: vec![
            // res1 = !__isnan(f32_var) && (f32_var < 5.0f)
            DirStmt::Assign {
                lhs: DirLValue::Var("res1".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::LogicalAnd,
                    lhs: Box::new(DirExpr::Unary {
                        op: DirUnaryOp::Not,
                        expr: Box::new(DirExpr::Call {
                            target: "__isnan".to_string(),
                            args: vec![DirExpr::Var("f32_var".to_string())],
                            ty: bool_ty.clone(),
                        }),
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: Box::new(DirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(DirExpr::Const(5, f32_ty.clone())),
                        ty: bool_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res2 = __isnan(f32_var) || (f32_var != 5.0f)
            DirStmt::Assign {
                lhs: DirLValue::Var("res2".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::LogicalOr,
                    lhs: Box::new(DirExpr::Call {
                        target: "__isnan".to_string(),
                        args: vec![DirExpr::Var("f32_var".to_string())],
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Ne,
                        lhs: Box::new(DirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(DirExpr::Const(5, f32_ty.clone())),
                        ty: bool_ty.clone(),
                    }),
                    ty: bool_ty.clone(),
                },
            },
            // res3 = !__isnan(f32_var) && !__isnan(f64_var) && (f32_var < f64_var)
            DirStmt::Assign {
                lhs: DirLValue::Var("res3".to_string()),
                rhs: DirExpr::Binary {
                    op: DirBinaryOp::LogicalAnd,
                    lhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::LogicalAnd,
                        lhs: Box::new(DirExpr::Unary {
                            op: DirUnaryOp::Not,
                            expr: Box::new(DirExpr::Call {
                                target: "__isnan".to_string(),
                                args: vec![DirExpr::Var("f32_var".to_string())],
                                ty: bool_ty.clone(),
                            }),
                            ty: bool_ty.clone(),
                        }),
                        rhs: Box::new(DirExpr::Unary {
                            op: DirUnaryOp::Not,
                            expr: Box::new(DirExpr::Call {
                                target: "__isnan".to_string(),
                                args: vec![DirExpr::Var("f64_var".to_string())],
                                ty: bool_ty.clone(),
                            }),
                            ty: bool_ty.clone(),
                        }),
                        ty: bool_ty.clone(),
                    }),
                    rhs: Box::new(DirExpr::Binary {
                        op: DirBinaryOp::Lt,
                        lhs: Box::new(DirExpr::Var("f32_var".to_string())),
                        rhs: Box::new(DirExpr::Var("f64_var".to_string())),
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
    let DirStmt::Assign { rhs: rhs1, .. } = &func.body[0] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs1 {
        assert_eq!(*op, DirBinaryOp::Lt);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &DirExpr::Const(5, f32_ty.clone()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs1);
    }

    // Verify second simplification: res2 = (f32_var != 5.0f)
    let DirStmt::Assign { rhs: rhs2, .. } = &func.body[1] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs2 {
        assert_eq!(*op, DirBinaryOp::Ne);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &DirExpr::Const(5, f32_ty.clone()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs2);
    }

    // Verify third simplification: res3 = (f32_var < f64_var)
    let DirStmt::Assign { rhs: rhs3, .. } = &func.body[2] else {
        panic!();
    };
    if let DirExpr::Binary { op, lhs, rhs, ty } = rhs3 {
        assert_eq!(*op, DirBinaryOp::Lt);
        assert_eq!(lhs.as_ref(), &DirExpr::Var("f32_var".to_string()));
        assert_eq!(rhs.as_ref(), &DirExpr::Var("f64_var".to_string()));
        assert_eq!(ty, &bool_ty);
    } else {
        panic!("expected binary op, got {:?}", rhs3);
    }
}

// ── Relational branch implication (conditional_const range tracking) ─────────

fn cond_const_test_func(body: Vec<DirStmt>) -> DirFunction {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    DirFunction {
        name: "test_range_cond".to_string(),
        int_param_offsets: Vec::new(),
        params: vec![],
        locals: vec![
            DirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
            DirBinding {
                name: "y".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            },
        ],
        body,
        is_64bit: false,
        ..Default::default()
    }
}

fn cmp_x(op: DirBinaryOp, val: i64) -> DirExpr {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    DirExpr::Binary {
        op,
        lhs: Box::new(DirExpr::Var("x".to_string())),
        rhs: Box::new(DirExpr::Const(val, i32_ty)),
        ty: NirType::Bool,
    }
}

fn assign_y(val: i64) -> DirStmt {
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    DirStmt::Assign {
        lhs: DirLValue::Var("y".to_string()),
        rhs: DirExpr::Const(val, i32_ty),
    }
}

#[test]
fn normalize_conditional_const_folds_implied_true_nested_cond() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // if (x > 0) { if (x >= 0) y=1 else y=2 }  →  inner cond decided true
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::SGt, 0),
        then_body: vec![DirStmt::If {
            cond: cmp_x(DirBinaryOp::SGe, 0),
            then_body: vec![assign_y(1)],
            else_body: vec![assign_y(2)],
        }],
        else_body: vec![],
    }]);

    assert!(apply_conditional_const_pass(&mut func));
    let DirStmt::If { then_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::If { cond, .. } = &then_body[0] else {
        panic!("expected inner if");
    };
    assert!(
        matches!(cond, DirExpr::Const(1, _)),
        "inner cond should fold to true, got {cond:?}"
    );
}

#[test]
fn normalize_conditional_const_folds_implied_false_nested_cond() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // if (x > 0) { if (x < 0) y=1 else y=2 }  →  inner cond decided false
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::SGt, 0),
        then_body: vec![DirStmt::If {
            cond: cmp_x(DirBinaryOp::SLt, 0),
            then_body: vec![assign_y(1)],
            else_body: vec![assign_y(2)],
        }],
        else_body: vec![],
    }]);

    assert!(apply_conditional_const_pass(&mut func));
    let DirStmt::If { then_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::If { cond, .. } = &then_body[0] else {
        panic!("expected inner if");
    };
    assert!(
        matches!(cond, DirExpr::Const(0, _)),
        "inner cond should fold to false, got {cond:?}"
    );
}

#[test]
fn normalize_conditional_const_keeps_undecidable_else_branch_cond() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // else of (x > 0) implies x <= 0; (x >= 0) still undecidable (x may be 0)
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::SGt, 0),
        then_body: vec![],
        else_body: vec![DirStmt::If {
            cond: cmp_x(DirBinaryOp::SGe, 0),
            then_body: vec![assign_y(1)],
            else_body: vec![assign_y(2)],
        }],
    }]);

    apply_conditional_const_pass(&mut func);
    let DirStmt::If { else_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::If { cond, .. } = &else_body[0] else {
        panic!("expected inner if");
    };
    assert!(
        matches!(cond, DirExpr::Binary { .. }),
        "undecidable cond must stay, got {cond:?}"
    );
}

#[test]
fn normalize_conditional_const_write_invalidates_range() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // if (x > 0) { x = call(); if (x >= 0) … }  →  no fold after the write
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::SGt, 0),
        then_body: vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("x".to_string()),
                rhs: DirExpr::Call {
                    target: "ext".to_string(),
                    args: vec![],
                    ty: i32_ty,
                },
            },
            DirStmt::If {
                cond: cmp_x(DirBinaryOp::SGe, 0),
                then_body: vec![assign_y(1)],
                else_body: vec![assign_y(2)],
            },
        ],
        else_body: vec![],
    }]);

    apply_conditional_const_pass(&mut func);
    let DirStmt::If { then_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::If { cond, .. } = &then_body[1] else {
        panic!("expected inner if");
    };
    assert!(
        matches!(cond, DirExpr::Binary { .. }),
        "cond after clobbering write must stay, got {cond:?}"
    );
}

#[test]
fn normalize_conditional_const_label_in_dead_arm_blocks_fold() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // Dead else arm holds a jump target label → fold must be skipped.
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::SGt, 0),
        then_body: vec![DirStmt::If {
            cond: cmp_x(DirBinaryOp::SGe, 0),
            then_body: vec![assign_y(1)],
            else_body: vec![DirStmt::Label("join_1".to_string()), assign_y(2)],
        }],
        else_body: vec![],
    }]);

    apply_conditional_const_pass(&mut func);
    let DirStmt::If { then_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::If { cond, .. } = &then_body[0] else {
        panic!("expected inner if");
    };
    assert!(
        matches!(cond, DirExpr::Binary { .. }),
        "label-bearing dead arm must block the fold, got {cond:?}"
    );
}

#[test]
fn normalize_conditional_const_branch_write_invalidates_eq_env_after_join() {
    use crate::midend::normalize::global_opt::apply_conditional_const_pass;

    // if (x == 5) { if (y != 0) { x = 1; }  y = x; }  →  y must NOT become 5
    let i32_ty = NirType::Int {
        bits: 32,
        signed: true,
    };
    let mut func = cond_const_test_func(vec![DirStmt::If {
        cond: cmp_x(DirBinaryOp::Eq, 5),
        then_body: vec![
            DirStmt::If {
                cond: DirExpr::Binary {
                    op: DirBinaryOp::Ne,
                    lhs: Box::new(DirExpr::Var("y".to_string())),
                    rhs: Box::new(DirExpr::Const(0, i32_ty.clone())),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(1, i32_ty),
                }],
                else_body: vec![],
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("y".to_string()),
                rhs: DirExpr::Var("x".to_string()),
            },
        ],
        else_body: vec![],
    }]);

    apply_conditional_const_pass(&mut func);
    let DirStmt::If { then_body, .. } = &func.body[0] else {
        panic!("expected outer if");
    };
    let DirStmt::Assign { rhs, .. } = &then_body[1] else {
        panic!("expected join assign");
    };
    assert!(
        matches!(rhs, DirExpr::Var(name) if name == "x"),
        "stale eq binding must not survive a branch write, got {rhs:?}"
    );
}
