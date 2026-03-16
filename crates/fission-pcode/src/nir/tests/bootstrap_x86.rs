use super::*;

#[test]
fn preview_supports_pe_x86_single_block() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x401000,
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x401000,
                output: None,
                inputs: vec![cst(0, 4), cst(7, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_ret", 0x401000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return 7;"), "{code}");
}

#[test]
fn preview_supports_pe_x86_multiblock_direct_target_branch() {
    let cond = uniq(0x360, 1);
    let direct_target = Varnode {
        space_id: 1,
        offset: 0x4020,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4000,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4000,
                        output: Some(cond.clone()),
                        inputs: vec![cst(1, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4001,
                        output: None,
                        inputs: vec![direct_target, cond],
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
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4020,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_branchy", 0x4000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}

#[test]
fn preview_names_x86_general_purpose_registers() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x402000,
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x402000,
                output: None,
                inputs: vec![cst(0, 4), reg(0x00, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_reg", 0x402000, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return eax;"), "{code}");
}

fn lower_x86_cond_expr(func: &PcodeFunction) -> HirExpr {
    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond { cond, .. } => cond,
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_recovers_test_reg_reg_jz_as_eq_zero() {
    let tmp = uniq(0x300, 4);
    let zf = reg(0x206, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x403000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x403000,
                    output: Some(tmp.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x00, 4)],
                    asm_mnemonic: Some("TEST EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x403000,
                    output: Some(zf.clone()),
                    inputs: vec![tmp, cst(0, 4)],
                    asm_mnemonic: Some("TEST EAX,EAX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x403001,
                    output: None,
                    inputs: vec![cst(0x403100, 4), zf],
                    asm_mnemonic: Some("JZ 0x403100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax == 0");
}

#[test]
fn preview_recovers_test_reg_reg_jg_as_gt_zero() {
    let tmp = uniq(0x310, 4);
    let of = reg(0x20b, 1);
    let sf = reg(0x207, 1);
    let zf = reg(0x206, 1);
    let not_zf = uniq(0x311, 1);
    let of_eq_sf = uniq(0x312, 1);
    let cond_vn = uniq(0x313, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x404000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x404000,
                    output: Some(of.clone()),
                    inputs: vec![cst(0, 1)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x404000,
                    output: Some(tmp.clone()),
                    inputs: vec![reg(0x04, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x404000,
                    output: Some(sf.clone()),
                    inputs: vec![tmp.clone(), cst(0, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x404000,
                    output: Some(zf.clone()),
                    inputs: vec![tmp, cst(0, 4)],
                    asm_mnemonic: Some("TEST ECX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x404001,
                    output: Some(not_zf.clone()),
                    inputs: vec![zf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x404001,
                    output: Some(of_eq_sf.clone()),
                    inputs: vec![of, sf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::BoolAnd,
                    address: 0x404001,
                    output: Some(cond_vn.clone()),
                    inputs: vec![not_zf, of_eq_sf],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x404001,
                    output: None,
                    inputs: vec![cst(0x404100, 4), cond_vn],
                    asm_mnemonic: Some("JG 0x404100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "0 < ecx");
}

#[test]
fn preview_recovers_cmp_je_as_eq() {
    let diff = uniq(0x320, 4);
    let zf = reg(0x206, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x405000,
                    output: Some(diff.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x405000,
                    output: Some(zf.clone()),
                    inputs: vec![diff, cst(0, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x405001,
                    output: None,
                    inputs: vec![cst(0x405100, 4), zf],
                    asm_mnemonic: Some("JE 0x405100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax == ecx");
}

#[test]
fn preview_recovers_cmp_jb_as_unsigned_lt() {
    let cf = reg(0x200, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x406000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntLess,
                    address: 0x406000,
                    output: Some(cf.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x406001,
                    output: None,
                    inputs: vec![cst(0x406100, 4), cf],
                    asm_mnemonic: Some("JB 0x406100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax < ecx");
}

#[test]
fn preview_recovers_cmp_jl_as_signed_lt() {
    let diff = uniq(0x330, 4);
    let sf = reg(0x207, 1);
    let of = reg(0x20b, 1);
    let cond_vn = uniq(0x331, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x407000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x407000,
                    output: Some(diff.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x407000,
                    output: Some(sf.clone()),
                    inputs: vec![diff, cst(0, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSBorrow,
                    address: 0x407000,
                    output: Some(of.clone()),
                    inputs: vec![reg(0x00, 4), reg(0x04, 4)],
                    asm_mnemonic: Some("CMP EAX,ECX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::IntNotEqual,
                    address: 0x407001,
                    output: Some(cond_vn.clone()),
                    inputs: vec![sf, of],
                    asm_mnemonic: Some("JL 0x407100".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x407001,
                    output: None,
                    inputs: vec![cst(0x407100, 4), cond_vn],
                    asm_mnemonic: Some("JL 0x407100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "eax < ecx");
}

#[test]
fn preview_leaves_non_exact_branch_shape_as_generic_value() {
    let weird = uniq(0x340, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x408000,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BoolXor,
                    address: 0x408000,
                    output: Some(weird.clone()),
                    inputs: vec![reg(0x206, 1), reg(0x207, 1)],
                    asm_mnemonic: Some("JCC".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x408001,
                    output: None,
                    inputs: vec![cst(0x408100, 4), weird],
                    asm_mnemonic: Some("JCC 0x408100".to_string()),
                },
            ],
        }],
    };

    let cond = lower_x86_cond_expr(&func);
    assert_eq!(print_expr(&cond), "reg ^ reg");
}
