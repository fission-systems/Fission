use super::*;
use crate::nir::structuring::cleanup_redundant_labels;
#[test]
fn multi_block_preview_lowers_wrapped_branch_condition_without_failing() {
    let cond_raw = uniq(0x390, 1);
    let cond_wrap = uniq(0x391, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3900,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3900,
                        output: Some(cond_raw.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x3901,
                        output: Some(cond_wrap.clone()),
                        inputs: vec![cond_raw],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3902,
                        output: None,
                        inputs: vec![cst(0x3920, 8), cond_wrap],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3910,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3910,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3920,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3920,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "wrapped_branch", 0x3900, &preview_options())
        .expect("preview render");
    assert!(code.contains("return 0;"));
    assert!(code.contains("return 1;"));
}

#[test]
fn malformed_store_is_skipped_without_preview_failure() {
    let ptr = uniq(0x3a0, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x3a00,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x3a00,
                    output: Some(ptr),
                    inputs: vec![reg(0x28, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x3a01,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x3a02,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "malformed_store", 0x3a00, &preview_options())
        .expect("preview render");
    assert!(code.contains("return 0;"));
}

#[test]

fn multiequal_with_identical_inputs_does_not_fail_preview() {
    let phi = uniq(0x500, 8);
    let copy = uniq(0x508, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
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
        }],
    };

    let code =
        render_mlil_preview(&func, "phi_fn", 0x5000, &preview_options()).expect("preview render");
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

    let code =
        render_mlil_preview(&func, "piece_fn", 0x6000, &preview_options()).expect("preview render");
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
fn redundant_adjacent_labels_are_folded_to_canonical_target() {
    let body = vec![
        HirStmt::Label("block_1000".to_string()),
        HirStmt::Label("block_1004".to_string()),
        HirStmt::Goto("block_1000".to_string()),
    ];

    let cleaned = cleanup_redundant_labels(body);
    assert_eq!(
        cleaned,
        vec![
            HirStmt::Label("block_1004".to_string()),
            HirStmt::Goto("block_1004".to_string()),
        ]
    );
}

#[test]
fn normalize_removes_constant_false_and_empty_if_residue() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("reg".to_string())),
                ty: NirType::Bool,
            },
            then_body: Vec::new(),
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Const(0, NirType::Bool),
            then_body: vec![HirStmt::Goto("block_aa0".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_keep".to_string()),
    ];

    normalize_function_body(&mut body);

    assert_eq!(body, vec![HirStmt::Label("block_keep".to_string())]);
}

#[test]
fn normalize_removes_if_goto_immediate_next_label() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_join".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_join".to_string()),
        HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::Label("block_join".to_string()),
            HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
        ]
    );
}

#[test]
fn normalize_rewrites_two_way_branch_with_fallthrough_target_to_one_way_branch() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_exit".to_string())],
            else_body: vec![HirStmt::Goto("block_fallthrough".to_string())],
        },
        HirStmt::Label("block_fallthrough".to_string()),
        HirStmt::Return(Some(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        ))),
        HirStmt::Label("block_exit".to_string()),
        HirStmt::Return(Some(HirExpr::Const(
            1,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        ))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Var("reg".to_string()),
                then_body: vec![HirStmt::Goto("block_exit".to_string())],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
            HirStmt::Label("block_exit".to_string()),
            HirStmt::Return(Some(HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
        ]
    );
}

#[test]
fn normalize_removes_unreferenced_leading_entry_label() {
    let mut body = vec![
        HirStmt::Label("block_entry".to_string()),
        HirStmt::Expr(HirExpr::Var("reg".to_string())),
        HirStmt::Return(Some(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        ))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::Expr(HirExpr::Var("reg".to_string())),
            HirStmt::Return(Some(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
        ]
    );
}

#[test]
fn normalize_fuses_single_predecessor_boundary_segment_under_negated_if() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_join".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Assign {
            lhs: HirLValue::Var("tmp_1".to_string()),
            rhs: HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        },
        HirStmt::Expr(HirExpr::Var("tmp_1".to_string())),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("tmp_1".to_string()),
                        rhs: HirExpr::Const(
                            1,
                            NirType::Int {
                                bits: 32,
                                signed: false,
                            },
                        ),
                    },
                    HirStmt::Expr(HirExpr::Var("tmp_1".to_string())),
                ],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
        ]
    );
}

#[test]
fn normalize_fuses_boundary_segment_with_nested_if() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("reg".to_string()),
            then_body: vec![HirStmt::Goto("block_join".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Expr(HirExpr::Var("pre".to_string())),
        HirStmt::If {
            cond: HirExpr::Var("flag".to_string()),
            then_body: vec![HirStmt::Expr(HirExpr::Var("body".to_string()))],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_join".to_string()),
        HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Var("reg".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![
                    HirStmt::Expr(HirExpr::Var("pre".to_string())),
                    HirStmt::If {
                        cond: HirExpr::Var("flag".to_string()),
                        then_body: vec![HirStmt::Expr(HirExpr::Var("body".to_string()))],
                        else_body: Vec::new(),
                    },
                ],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Var("reg".to_string()))),
        ]
    );
}

#[test]
fn normalize_promotes_guarded_jump_target_tail() {
    let mut body = vec![
        HirStmt::If {
            cond: HirExpr::Var("cond_a".to_string()),
            then_body: vec![HirStmt::Goto("block_body".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::If {
            cond: HirExpr::Var("cond_b".to_string()),
            then_body: vec![HirStmt::Goto("block_join".to_string())],
            else_body: Vec::new(),
        },
        HirStmt::Label("block_body".to_string()),
        HirStmt::Expr(HirExpr::Var("body".to_string())),
        HirStmt::Label("block_join".to_string()),
        HirStmt::Return(Some(HirExpr::Const(
            0,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        ))),
    ];

    normalize_function_body(&mut body);

    assert_eq!(
        body,
        vec![
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::LogicalOr,
                    lhs: Box::new(HirExpr::Var("cond_a".to_string())),
                    rhs: Box::new(HirExpr::Unary {
                        op: HirUnaryOp::Not,
                        expr: Box::new(HirExpr::Var("cond_b".to_string())),
                        ty: NirType::Bool,
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Expr(HirExpr::Var("body".to_string()))],
                else_body: Vec::new(),
            },
            HirStmt::Return(Some(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ))),
        ]
    );
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
fn trap_like_unknown_producer_can_surface_as_opaque_callind_target() {
    let trap_target = uniq(0x6a0, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x6a00,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Unknown,
                    address: 0x6a00,
                    output: Some(trap_target.clone()),
                    inputs: vec![cst(16, 4), cst(3, 4)],
                    asm_mnemonic: Some("INT3".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x6a01,
                    output: None,
                    inputs: vec![trap_target],
                    asm_mnemonic: Some("INT3".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x6a02,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "trap_callind", 0x6a00, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("((code *)swi(3))();"), "{code}");
    assert!(code.contains("return 0;"), "{code}");
}

#[test]
fn non_trap_unknown_callind_target_still_fails_preview() {
    let trap_target = uniq(0x6b0, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x6b00,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Unknown,
                    address: 0x6b00,
                    output: Some(trap_target.clone()),
                    inputs: vec![cst(16, 4), cst(3, 4)],
                    asm_mnemonic: Some("NOP".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x6b01,
                    output: None,
                    inputs: vec![trap_target],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x6b02,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let err = render_mlil_preview(&func, "non_trap_callind", 0x6b00, &preview_options_x86())
        .expect_err("unknown non-trap producer should still fail");
    assert!(matches!(
        err,
        MlilPreviewError::UnsupportedPattern("opcode")
    ));
}
