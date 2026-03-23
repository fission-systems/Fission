use super::*;
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

    let code =
        render_mlil_preview(&func, "branchy", 0x3000, &preview_options()).expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("return 0;"));
    assert!(code.contains("return 1;"));
}

#[test]
fn x86_try_lower_if_still_structures_canonical_if() {
    let cond = uniq(0x430, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4300,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4300,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4301,
                        output: None,
                        inputs: vec![cst(0x4320, 4), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4310,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4310,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4320,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_if", 0x4300, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}

#[test]

fn x86_pathological_try_lower_if_falls_back_without_hanging() {
    let cond0 = uniq(0x480, 1);
    let cond1 = uniq(0x481, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4800,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4800,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4801,
                        output: None,
                        inputs: vec![cst(0x4820, 4), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4810,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4810,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x09, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4811,
                        output: None,
                        inputs: vec![cst(0x4810, 4), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4820,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4820,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_path_if", 0x4800, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(
        code.contains("do {") || code.contains("block_4810") || code.contains("goto block_4810;"),
        "{code}"
    );
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
fn multi_block_preview_prefers_short_circuit_or_over_nested_plain_if() {
    let cond0 = uniq(0x354, 1);
    let cond1 = uniq(0x355, 1);
    let ptr = uniq(0x356, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3540,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3540,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3541,
                        output: None,
                        inputs: vec![cst(0x3570, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3550,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3550,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x09, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3551,
                        output: None,
                        inputs: vec![cst(0x3570, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3560,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x3560,
                    output: None,
                    inputs: vec![cst(0x3580, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3570,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3570,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3571,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3572,
                        output: None,
                        inputs: vec![cst(0x3580, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x3580,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3580,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "short_or_fn", 0x3540, &preview_options())
        .expect("preview render");
    assert!(code.contains("||"), "{code}");
    assert!(code.contains("local_10 = 1;"), "{code}");
    assert!(!code.contains("goto block_3550;"), "{code}");
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
fn region_recovery_succeeds_on_one_arm_forwarding_join() {
    let cond = uniq(0x4c0, 1);
    let side = uniq(0x4c1, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4c00,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4c00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4c01,
                        output: None,
                        inputs: vec![cst(0x4c20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4c10,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4c10,
                        output: Some(side.clone()),
                        inputs: vec![cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4c11,
                        output: None,
                        inputs: vec![cst(0x4c40, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4c20,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4c20,
                    output: None,
                    inputs: vec![cst(0x4c30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4c30,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4c30,
                    output: None,
                    inputs: vec![cst(0x4c40, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x4c40,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4c40,
                    output: None,
                    inputs: vec![cst(0, 8), side],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(4), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_succeeds_on_trampoline_tail_shared_join() {
    let cond = uniq(0x4d0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4d00,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4d00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4d01,
                        output: None,
                        inputs: vec![cst(0x4d20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4d10,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d10,
                    output: None,
                    inputs: vec![cst(0x4d30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4d20,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d20,
                    output: None,
                    inputs: vec![cst(0x4d30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4d30,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d30,
                    output: None,
                    inputs: vec![cst(0x4d40, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x4d40,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4d40,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(4), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_succeeds_on_two_arm_nearby_join() {
    let cond = uniq(0x4e0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4e00,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4e00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4e01,
                        output: None,
                        inputs: vec![cst(0x4e20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4e10,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4e10,
                    output: None,
                    inputs: vec![cst(0x4e30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4e20,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4e20,
                    output: None,
                    inputs: vec![cst(0x4e30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4e30,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4e30,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(3), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_succeeds_on_multi_hop_trampoline_join() {
    let cond = uniq(0x4f0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4f00,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4f00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4f01,
                        output: None,
                        inputs: vec![cst(0x4f20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4f10,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f10,
                    output: None,
                    inputs: vec![cst(0x4f30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4f20,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f20,
                    output: None,
                    inputs: vec![cst(0x4f40, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4f30,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f30,
                    output: None,
                    inputs: vec![cst(0x4f50, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x4f40,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f40,
                    output: None,
                    inputs: vec![cst(0x4f50, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x4f50,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f50,
                    output: None,
                    inputs: vec![cst(0x4f60, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x4f60,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4f60,
                    output: None,
                    inputs: vec![cst(0x4f70, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 7,
                start_address: 0x4f70,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4f70,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(7), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_canonicalization_respects_origin_guard() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5000,
                    output: None,
                    inputs: vec![cst(0x5010, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5010,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5010,
                    output: None,
                    inputs: vec![cst(0x5040, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5020,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5020,
                    output: None,
                    inputs: vec![cst(0x5040, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5030,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5030,
                    output: None,
                    inputs: vec![cst(0x5040, 8)],
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
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let builder = PreviewBuilder::new(&func, &options, None);

    assert_eq!(
        builder.canonicalize_region_target_for_exit_for_test(0, 1, LinearExit::Join(4)),
        Some(4)
    );
    assert_eq!(
        builder.canonicalize_region_target_for_exit_for_test(2, 1, LinearExit::Join(4)),
        None
    );
}

#[test]
fn region_recovery_lowers_one_arm_join_adjacent_forwarding_chain() {
    let cond = uniq(0x510, 1);
    let side = uniq(0x511, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5100,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5100,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5101,
                        output: None,
                        inputs: vec![cst(0x5120, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5110,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5110,
                        output: Some(side.clone()),
                        inputs: vec![cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5111,
                        output: None,
                        inputs: vec![cst(0x5150, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5120,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5120,
                    output: None,
                    inputs: vec![cst(0x5130, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5130,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5130,
                    output: None,
                    inputs: vec![cst(0x5140, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5140,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5140,
                    output: None,
                    inputs: vec![cst(0x5150, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x5150,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5150,
                    output: None,
                    inputs: vec![cst(0, 8), side],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(5), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_lowers_two_arm_shared_tail_entry() {
    let cond = uniq(0x520, 1);
    let left = uniq(0x521, 4);
    let right = uniq(0x522, 4);
    let merged = uniq(0x523, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5200,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5200,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5201,
                        output: None,
                        inputs: vec![cst(0x5220, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5210,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5210,
                        output: Some(left.clone()),
                        inputs: vec![cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5211,
                        output: None,
                        inputs: vec![cst(0x5230, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5220,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5220,
                        output: Some(right.clone()),
                        inputs: vec![cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5221,
                        output: None,
                        inputs: vec![cst(0x5240, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5230,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5230,
                    output: None,
                    inputs: vec![cst(0x5250, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5240,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5240,
                    output: None,
                    inputs: vec![cst(0x5250, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x5250,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5250,
                        output: Some(merged.clone()),
                        inputs: vec![cst(9, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5251,
                        output: None,
                        inputs: vec![cst(0x5260, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x5260,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5260,
                    output: None,
                    inputs: vec![cst(0, 8), merged],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(6), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_lowers_two_arm_nontrivial_shared_follow() {
    let cond = uniq(0x530, 1);
    let left = uniq(0x531, 4);
    let right = uniq(0x532, 4);
    let merged = uniq(0x533, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5300,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5300,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5301,
                        output: None,
                        inputs: vec![cst(0x5320, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5310,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5310,
                        output: Some(left.clone()),
                        inputs: vec![cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5311,
                        output: None,
                        inputs: vec![cst(0x5330, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5320,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5320,
                        output: Some(right.clone()),
                        inputs: vec![cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5321,
                        output: None,
                        inputs: vec![cst(0x5340, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5330,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5330,
                        output: Some(left.clone()),
                        inputs: vec![left.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5331,
                        output: None,
                        inputs: vec![cst(0x5350, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5340,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5340,
                        output: Some(right.clone()),
                        inputs: vec![right.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5341,
                        output: None,
                        inputs: vec![cst(0x5350, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x5350,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5350,
                        output: Some(merged.clone()),
                        inputs: vec![left],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5351,
                        output: None,
                        inputs: vec![cst(0x5360, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x5360,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5360,
                    output: None,
                    inputs: vec![cst(0, 8), merged],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let lowered = builder
        .lower_linear_body_for_region_recovery_detailed(0, LinearExit::Join(6), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_follow_discovery_selects_immediate_common_postdom() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6000,
                    output: None,
                    inputs: vec![cst(0x6020, 8), reg(0x08, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6010,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6010,
                    output: None,
                    inputs: vec![cst(0x6030, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6020,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6020,
                    output: None,
                    inputs: vec![cst(0x6040, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6030,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6030,
                    output: None,
                    inputs: vec![cst(0x6050, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6040,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6040,
                    output: None,
                    inputs: vec![cst(0x6050, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x6050,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6050,
                    output: None,
                    inputs: vec![cst(0x6060, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x6060,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6060,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let builder = PreviewBuilder::new(&func, &options, None);
    let (shared, subtype) = builder.find_shared_tail_entries_for_region_for_test(0, 2, 1, 6);
    assert_eq!(shared.first().copied(), Some(5));
    assert_eq!(subtype, None);
}

#[test]
fn region_follow_discovery_rejects_side_entry_common_follow() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6100,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6100,
                    output: None,
                    inputs: vec![cst(0x6120, 8), reg(0x08, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6110,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6110,
                    output: None,
                    inputs: vec![cst(0x6130, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6120,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6120,
                    output: None,
                    inputs: vec![cst(0x6140, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6130,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6130,
                    output: None,
                    inputs: vec![cst(0x6150, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6140,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6140,
                    output: None,
                    inputs: vec![cst(0x6150, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x6150,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6150,
                    output: None,
                    inputs: vec![cst(0x6160, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x6160,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6160,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 7,
                start_address: 0x6170,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6170,
                    output: None,
                    inputs: vec![cst(0x6150, 8)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let builder = PreviewBuilder::new(&func, &options, None);
    let (shared, subtype) = builder.find_shared_tail_entries_for_region_for_test(0, 2, 1, 6);
    assert!(shared.is_empty());
    assert_eq!(subtype, Some("SideEntryOrExit"));
}

#[test]
fn region_follow_discovery_orders_multiple_candidates_closest_to_join_first() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6200,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6200,
                    output: None,
                    inputs: vec![cst(0x6220, 8), reg(0x08, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6210,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6210,
                    output: None,
                    inputs: vec![cst(0x6230, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6220,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6220,
                    output: None,
                    inputs: vec![cst(0x6240, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6230,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6230,
                    output: None,
                    inputs: vec![cst(0x6250, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6240,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6240,
                    output: None,
                    inputs: vec![cst(0x6250, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x6250,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6250,
                    output: None,
                    inputs: vec![cst(0x6260, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x6260,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6260,
                    output: None,
                    inputs: vec![cst(0x6270, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 7,
                start_address: 0x6270,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6270,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options_x86();
    options.region_linearize_structuring = true;
    let builder = PreviewBuilder::new(&func, &options, None);
    let (shared, subtype) = builder.find_shared_tail_entries_for_region_for_test(0, 2, 1, 7);
    assert_eq!(subtype, None);
    assert_eq!(shared, vec![6, 5]);
}
