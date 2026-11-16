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
    assert!(
        code.contains("param_1 == 1") || code.contains("param_1 != 1"),
        "{code}"
    );
    assert!(
        code.contains("param_1 == 2") || code.contains("param_1 != 2"),
        "{code}"
    );
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

#[test]
fn test_switch_skips_to_exit() {
    let cond0 = uniq(0x500, 1);
    let cond1 = uniq(0x501, 1);
    let ptr1 = uniq(0x502, 8);
    let ptr2 = uniq(0x503, 8);
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
                        inputs: vec![cst(0x5020, 8), cond0],
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
                        inputs: vec![cst(0x5030, 8), cond1], // Case 2 goes directly to exit 0x5030 (skip!)
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5020,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x5021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(77, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5022,
                        output: None,
                        inputs: vec![cst(0x5030, 8)],
                        asm_mnemonic: None,
                    },
                ],
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
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "skips_switch", 0x5000, &preview_options())
        .expect("preview render");
    assert!(code.contains("switch (param_1) {"));
    assert!(code.contains("case 1:"));
    assert!(code.contains("case 2:"));
    assert!(code.contains("default:"));
    // The shared `return rax` exit is a terminal tail, so `duplicate_terminal_tails`
    // copies it into the `default` arm rather than jumping back into `case 2`'s
    // body. That removes both the goto and the label-inside-a-case it targeted.
    assert!(!code.contains("goto block_5030"), "{code}");
    assert!(!code.contains("block_5030:"), "{code}");
    assert_eq!(code.matches("return rax;").count(), 2, "{code}");
}

/// A jump-table dispatch over three cases that share a join, with `head` in
/// front of the dispatch in its block.
fn direct_dispatcher_with_join(head: Vec<PcodeOp>) -> PcodeFunction {
    let selector = uniq(0x600, 4);
    let ptr1 = uniq(0x601, 8);
    let ptr2 = uniq(0x602, 8);
    let ptr3 = uniq(0x603, 8);
    let mut func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                successors: vec![1, 2, 3],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x6000,
                        output: Some(selector.clone()),
                        inputs: vec![reg(0x08, 4)],
                        asm_mnemonic: None,
                    },
                    // Switch terminator
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::BranchInd,
                        address: 0x6001,
                        output: None,
                        inputs: vec![selector],
                        asm_mnemonic: Some("jmp [rax*8 + 0x6010]".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x6010,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x6011,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(11, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x6012,
                        output: None,
                        inputs: vec![cst(0x6040, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x6020,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x6021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2, cst(22, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x6022,
                        output: None,
                        inputs: vec![cst(0x6040, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6030,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x6030,
                        output: Some(ptr3.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x18, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x6031,
                        output: None,
                        inputs: vec![cst(0, 4), ptr3, cst(33, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x6032,
                        output: None,
                        inputs: vec![cst(0x6040, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6040,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6040,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };
    let dispatch = std::mem::take(&mut func.blocks[0].ops);
    func.blocks[0].ops = head.into_iter().chain(dispatch).collect();
    for (seq, op) in func.blocks[0].ops.iter_mut().enumerate() {
        op.seq_num = seq as u32;
    }
    func
}

#[test]
fn test_direct_switch_dispatcher_with_join() {
    let func = direct_dispatcher_with_join(Vec::new());
    // Inject manual switch facts into path config or construct it manually
    // Since direct switch dispatcher relies on lower_block_terminator returning LoweredTerminator::Switch,
    // let's simulate this via custom dispatcher metadata or by letting our parser/lifter recognize it.
    // Actually, Fission CLI recovers direct switches from BranchInd if we provide the switch targets via annotations or metadata.
    // Let's configure options/proof or just verify that the direct switch is structured.
    let opts = preview_options_x86();

    // Fission decompiler's FactStore or layout builder will build the direct switch metadata.
    // Let's see: we can mock/assert that it compiles and can be previewed.
    let code = render_mlil_preview(&func, "direct_switch", 0x6000, &opts).expect("preview render");
    assert!(code.contains("switch ("));
    assert!(code.contains("case 1:"));
    assert!(code.contains("case 2:"));
    assert!(code.contains("default:"));
    assert!(!code.contains("__fission_branchind("));
    assert!(!code.contains("goto block_6040"));
}

#[test]
fn test_switch_case_fallthrough() {
    let cond0 = uniq(0x700, 1);
    let cond1 = uniq(0x701, 1);
    let ptr1 = uniq(0x702, 8);
    let ptr2 = uniq(0x703, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x7000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x7000,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 4), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x7001,
                        output: None,
                        inputs: vec![cst(0x7010, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x7040,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntEqual,
                        address: 0x7040,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x08, 4), cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x7041,
                        output: None,
                        inputs: vec![cst(0x7020, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x7030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x7030,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x7010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x7010,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x7011,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(11, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x7012,
                        output: None,
                        inputs: vec![cst(0x7020, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x7020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x7020,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x7021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2, cst(22, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x7022,
                        output: None,
                        inputs: vec![cst(0x7030, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "fallthrough_switch", 0x7000, &preview_options())
        .expect("preview render");
    println!(
        "--- FALLTHROUGH SWITCH CODE ---\n{}\n-------------------------------",
        code
    );
    assert!(code.contains("switch (param_1) {"));
    assert!(code.contains("case 1:"));
    assert!(code.contains("case 2:"));
    // Goto case 2 label should be elided and replaced by fallthrough comment
    assert!(code.contains("/* fallthrough */"));
    assert!(!code.contains("goto block_7020"));
}

/// A chain of two compares on `param_1` whose arms each return a different
/// constant, with `first` in front of the first compare and `second` in front
/// of the second.
fn compare_chain_with_work(first: Vec<PcodeOp>, second: Vec<PcodeOp>) -> PcodeFunction {
    fn op(
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }
    fn block(index: u32, start_address: u64, mut ops: Vec<PcodeOp>) -> PcodeBasicBlock {
        for (seq, op) in ops.iter_mut().enumerate() {
            op.seq_num = seq as u32;
        }
        PcodeBasicBlock {
            index,
            start_address,
            successors: vec![],
            ops,
        }
    }
    fn compare(address: u64, value: i64, target: u64) -> Vec<PcodeOp> {
        let cond = uniq(0x500 + value as u64, 1);
        vec![
            op(
                address,
                PcodeOpcode::IntEqual,
                Some(cond.clone()),
                vec![reg(0x08, 4), cst(value, 4)],
            ),
            op(
                address + 1,
                PcodeOpcode::CBranch,
                None,
                vec![cst(target as i64, 8), cond],
            ),
        ]
    }
    fn returns(index: u32, address: u64, value: i64) -> PcodeBasicBlock {
        block(
            index,
            address,
            vec![op(
                address,
                PcodeOpcode::Return,
                None,
                vec![cst(0, 8), cst(value, 4)],
            )],
        )
    }
    let mut head = first;
    head.extend(compare(0x5008, 1, 0x5030));
    let mut next = second;
    next.extend(compare(0x5018, 2, 0x5040));
    PcodeFunction {
        blocks: vec![
            block(0, 0x5000, head),
            block(1, 0x5010, next),
            returns(2, 0x5020, 0),
            returns(3, 0x5030, 1),
            returns(4, 0x5040, 2),
        ],
    }
}

fn call_to(address: u64, target: u64) -> PcodeOp {
    PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::Call,
        address,
        output: None,
        inputs: vec![Varnode {
            space_id: 3,
            offset: target,
            size: 8,
            is_constant: false,
            constant_val: 0,
        }],
        asm_mnemonic: None,
    }
}

/// The switch replaces the first block's branch, not what the block does
/// before it branches.
#[test]
fn a_switch_chain_keeps_what_its_first_block_does() {
    let func = compare_chain_with_work(vec![call_to(0x5000, 0x6000)], Vec::new());
    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    let call = body.find("sub_6000(").expect(&code);
    let switch = body.find("switch (").expect(&code);
    assert!(call < switch, "{code}");
    assert_eq!(body.matches("sub_6000(").count(), 1, "{code}");
}

/// Work in front of a later compare runs only when the earlier compare
/// failed, and a `switch` has nowhere to put that.
#[test]
fn a_chain_that_does_work_between_its_compares_is_not_a_switch() {
    let func = compare_chain_with_work(Vec::new(), vec![call_to(0x5010, 0x6000)]);
    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    assert!(!body.contains("switch ("), "{code}");
    assert_eq!(body.matches("sub_6000(").count(), 1, "{code}");
    assert!(code.contains("return 2;"), "{code}");
}

/// The same for a jump-table dispatch: the block that dispatches may do work
/// first.
#[test]
fn a_jump_table_switch_keeps_what_its_block_does_first() {
    let func = direct_dispatcher_with_join(vec![call_to(0x5fff, 0x7000)]);
    let code = render_mlil_preview(&func, "direct_switch", 0x6000, &preview_options_x86())
        .expect("preview render");
    let body = &code[code.find("direct_switch(").expect(&code)..];
    let call = body.find("sub_7000(").expect(&code);
    let switch = body.find("switch (").expect(&code);
    assert!(call < switch, "{code}");
    assert_eq!(body.matches("sub_7000(").count(), 1, "{code}");
}

/// A compare chain whose arms all return, with a block in the middle of them
/// that the chain never reaches: the entry branches to it around the chain.
///
/// ```text
/// 0x5000: if (rsi) goto 0x5040
/// 0x5010: if (rcx == 1) goto 0x5050
/// 0x5020: if (rcx == 2) goto 0x5060
/// 0x5030: return 0
/// 0x5040: sub_6000(); return 3      <- not the chain's
/// 0x5050: return 1
/// 0x5060: return 2
/// ```
fn chain_with_a_block_it_does_not_own() -> PcodeFunction {
    fn op(
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }
    fn block(index: u32, start_address: u64, mut ops: Vec<PcodeOp>) -> PcodeBasicBlock {
        for (seq, op) in ops.iter_mut().enumerate() {
            op.seq_num = seq as u32;
        }
        PcodeBasicBlock {
            index,
            start_address,
            successors: vec![],
            ops,
        }
    }
    fn compare(address: u64, value: i64, target: u64) -> Vec<PcodeOp> {
        let cond = uniq(0x500 + value as u64, 1);
        vec![
            op(
                address,
                PcodeOpcode::IntEqual,
                Some(cond.clone()),
                vec![reg(0x08, 4), cst(value, 4)],
            ),
            op(
                address + 1,
                PcodeOpcode::CBranch,
                None,
                vec![cst(target as i64, 8), cond],
            ),
        ]
    }
    fn returns(address: u64, value: i64) -> PcodeOp {
        op(
            address,
            PcodeOpcode::Return,
            None,
            vec![cst(0, 8), cst(value, 4)],
        )
    }
    PcodeFunction {
        blocks: vec![
            block(
                0,
                0x5000,
                vec![op(
                    0x5000,
                    PcodeOpcode::CBranch,
                    None,
                    vec![cst(0x5040, 8), reg(0x30, 1)],
                )],
            ),
            block(1, 0x5010, compare(0x5010, 1, 0x5050)),
            block(2, 0x5020, compare(0x5020, 2, 0x5060)),
            block(3, 0x5030, vec![returns(0x5030, 0)]),
            block(4, 0x5040, vec![call_to(0x5040, 0x6000), returns(0x5041, 3)]),
            block(5, 0x5050, vec![returns(0x5050, 1)]),
            block(6, 0x5060, vec![returns(0x5060, 2)]),
        ],
    }
}

/// With no join, a recovered switch claims every block up to the last one an
/// arm reached. A block in that range that is not the switch's has to be
/// printed all the same.
#[test]
fn a_block_between_the_arms_that_the_switch_does_not_hold_is_still_printed() {
    let func = chain_with_a_block_it_does_not_own();
    let mut options = preview_options();
    options.force_linear_structuring = true;
    let code = render_mlil_preview(&func, "switchy", 0x5000, &options).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    assert_eq!(body.matches("sub_6000(").count(), 1, "{code}");
    assert!(body.contains("return 3;"), "{code}");
    assert!(!body.contains("goto "), "{code}");
}

fn raw_op(
    address: u64,
    opcode: PcodeOpcode,
    output: Option<Varnode>,
    inputs: Vec<Varnode>,
) -> PcodeOp {
    PcodeOp {
        seq_num: 0,
        opcode,
        address,
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn raw_block(index: u32, start_address: u64, mut ops: Vec<PcodeOp>) -> PcodeBasicBlock {
    for (seq, op) in ops.iter_mut().enumerate() {
        op.seq_num = seq as u32;
    }
    PcodeBasicBlock {
        index,
        start_address,
        successors: vec![],
        ops,
    }
}

/// What a compare block usually holds besides its branch is the compare: a
/// register nothing reads again. That is no reason to give up the switch.
#[test]
fn a_later_compare_that_leaves_a_register_nobody_reads_is_still_a_switch() {
    let scratch = raw_op(
        0x5010,
        PcodeOpcode::Copy,
        Some(reg(0x40, 4)),
        vec![reg(0x08, 4)],
    );
    let func = compare_chain_with_work(Vec::new(), vec![scratch]);
    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    assert!(body.contains("switch ("), "{code}");
    assert!(body.contains("return 2;"), "{code}");
}

/// A register the later compare block sets and the arm behind it returns:
/// the write is read, so the chain is not turned into a switch that has no
/// place for it.
#[test]
fn a_later_compare_that_sets_what_an_arm_returns_is_not_a_switch() {
    let carried = raw_op(
        0x5010,
        PcodeOpcode::Copy,
        Some(reg(0x00, 4)),
        vec![cst(77, 4)],
    );
    let mut func = compare_chain_with_work(Vec::new(), vec![carried]);
    func.blocks[4].ops[0].inputs[1] = reg(0x00, 4);
    func.blocks[2].ops.insert(
        0,
        raw_op(
            0x5020,
            PcodeOpcode::Copy,
            Some(reg(0x00, 4)),
            vec![cst(55, 4)],
        ),
    );
    func.blocks[2].ops[1].inputs[1] = reg(0x00, 4);
    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    assert!(!body.contains("switch ("), "{code}");
    assert!(body.contains("return 77;"), "{code}");
    assert!(body.contains("return 55;"), "{code}");
}

/// Two arms that call something and meet at a block that sets the return
/// value, and a default that sets its own and jumps past that block to the
/// return.
fn chain_whose_default_sets_its_own_return_value() -> PcodeFunction {
    fn compare(address: u64, value: i64, target: u64) -> Vec<PcodeOp> {
        let cond = uniq(0x500 + value as u64, 1);
        vec![
            raw_op(
                address,
                PcodeOpcode::IntEqual,
                Some(cond.clone()),
                vec![reg(0x08, 4), cst(value, 4)],
            ),
            raw_op(
                address + 1,
                PcodeOpcode::CBranch,
                None,
                vec![cst(target as i64, 8), cond],
            ),
        ]
    }
    fn jumps(address: u64, target: u64) -> PcodeOp {
        raw_op(
            address,
            PcodeOpcode::Branch,
            None,
            vec![cst(target as i64, 8)],
        )
    }
    fn sets_return_value(address: u64, value: i64) -> PcodeOp {
        raw_op(
            address,
            PcodeOpcode::Copy,
            Some(reg(0x00, 4)),
            vec![cst(value, 4)],
        )
    }
    PcodeFunction {
        blocks: vec![
            raw_block(0, 0x5000, compare(0x5000, 1, 0x5030)),
            raw_block(1, 0x5010, compare(0x5010, 2, 0x5040)),
            raw_block(
                2,
                0x5020,
                vec![sets_return_value(0x5020, 90), jumps(0x5021, 0x5060)],
            ),
            raw_block(
                3,
                0x5030,
                vec![call_to(0x5030, 0x6000), jumps(0x5031, 0x5050)],
            ),
            raw_block(
                4,
                0x5040,
                vec![call_to(0x5040, 0x7000), jumps(0x5041, 0x5050)],
            ),
            raw_block(5, 0x5050, vec![sets_return_value(0x5050, 40)]),
            raw_block(
                6,
                0x5060,
                vec![raw_op(
                    0x5060,
                    PcodeOpcode::Return,
                    None,
                    vec![cst(0, 8), reg(0x00, 4)],
                )],
            ),
        ],
    }
}

/// A default that is one block setting the return value and jumping to the
/// return is still an arm: stepping over it to the return loses what it set.
#[test]
fn a_default_that_only_sets_the_return_value_keeps_it() {
    let func = chain_whose_default_sets_its_own_return_value();
    let code =
        render_mlil_preview(&func, "switchy", 0x5000, &preview_options()).expect("preview render");
    let body = &code[code.find("switchy(").expect(&code)..];
    assert!(body.contains("90"), "{code}");
    assert!(body.contains("40"), "{code}");
    assert_eq!(body.matches("sub_6000(").count(), 1, "{code}");
    assert_eq!(body.matches("sub_7000(").count(), 1, "{code}");
}
