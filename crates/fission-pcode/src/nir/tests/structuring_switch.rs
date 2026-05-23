use super::*;
#[test]
fn multi_block_preview_lowers_canonical_switch_chain() {
    let cond0 = uniq(0x500, 1);
    let cond1 = uniq(0x501, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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

    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    assert!(code.contains("switch (param_1) {"));
    assert!(code.contains("case 1:"));
    assert!(code.contains("case 2:"));
    assert!(code.contains("default:"));
}

#[test]
fn multi_block_preview_does_not_lower_switch_when_default_exit_differs_from_case_exit() {
    let cond0 = uniq(0x530, 1);
    let cond1 = uniq(0x531, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5300,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5300,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 4), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5301,
                        output: None,
                        inputs: vec![cst(0x5330, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5310,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5310,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x08, 4), cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5311,
                        output: None,
                        inputs: vec![cst(0x5340, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5320,
                    output: None,
                    inputs: vec![cst(0x5380, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5330,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5330,
                    output: None,
                    inputs: vec![cst(0x5370, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5340,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5340,
                    output: None,
                    inputs: vec![cst(0x5370, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x5350,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x5350,
                    output: None,
                    inputs: vec![cst(0x5380, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x5370,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5370,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 7,
                start_address: 0x5380,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5380,
                    output: None,
                    inputs: vec![cst(0, 8), cst(9, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "switch_skip_guard", 0x5300, &preview_options())
        .expect("preview render");
    assert!(!code.contains("switch ("), "{code}");
    assert!(code.contains("param_1 == 1"), "{code}");
    assert!(code.contains("param_1 == 2"), "{code}");
}

#[test]
fn multi_block_preview_lowers_switch_chain_after_upper_bound_guard() {
    let cond0 = uniq(0x540, 1);
    let cond1 = uniq(0x541, 1);
    let cond2 = uniq(0x542, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5400,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLess,
                        address: 0x5400,
                        output: Some(cond0.clone()),
                        inputs: vec![cst(3, 4), reg(0x08, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5401,
                        output: None,
                        inputs: vec![cst(0x5430, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5410,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5410,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x08, 4), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5411,
                        output: None,
                        inputs: vec![cst(0x5440, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5420,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5420,
                        output: Some(cond2.clone()),
                        inputs: vec![reg(0x08, 4), cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5421,
                        output: None,
                        inputs: vec![cst(0x5450, 8), cond2],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5430,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5430,
                    output: None,
                    inputs: vec![cst(0, 8), cst(9, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5440,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5440,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x5450,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5450,
                    output: None,
                    inputs: vec![cst(0, 8), cst(2, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let mut options = preview_options();
    options.force_linear_structuring = true;
    let code =
        render_mlil_preview(&func, "guarded_switchy", 0x5400, &options).expect("preview render");
    assert!(code.contains("switch (param_1) {"), "{code}");
    assert!(code.contains("case 1:"), "{code}");
    assert!(code.contains("case 2:"), "{code}");
    assert!(code.contains("default:"), "{code}");
}

#[test]
fn multi_block_preview_rejects_switch_chain_with_mixed_selector_family() {
    let cond0 = uniq(0x550, 1);
    let cond1 = uniq(0x551, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5500,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5500,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 4), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5501,
                        output: None,
                        inputs: vec![cst(0x5530, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5510,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x5510,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x10, 4), cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5511,
                        output: None,
                        inputs: vec![cst(0x5540, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5520,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5520,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5530,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5530,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5540,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5540,
                    output: None,
                    inputs: vec![cst(0, 8), cst(2, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "mixed_switch", 0x5500, &preview_options())
        .expect("preview render");
    assert!(!code.contains("switch ("), "{code}");
    assert!(
        code.contains("param_1 == 1") || code.contains("param_1 != 1"),
        "{code}"
    );
    assert!(code.contains("param_2 == 2"), "{code}");
}

#[test]
fn multiblock_preview_skips_orphan_unreachable_unsupported_block() {
    let orphan_target = uniq(0x700, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6000,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6050,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x6050,
                        output: Some(orphan_target.clone()),
                        inputs: vec![cst(1, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::BranchInd,
                        address: 0x6051,
                        output: None,
                        inputs: vec![orphan_target],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "orphan_branchind", 0x6000, &preview_options())
        .expect("preview render");
    assert!(!code.contains("__fission_branchind("), "{code}");
    assert!(!code.contains("block_6050"), "{code}");
}
