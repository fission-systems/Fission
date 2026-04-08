use super::*;

#[test]
fn preview_supports_instruction_local_conditional_branch_targets() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x5000,
                    output: None,
                    inputs: vec![cst(2, 1), reg(0x206, 1)],
                    asm_mnemonic: Some("JZ <pcode+2>".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x5000,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: Some("RET 0".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x5000,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: Some("RET 1".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "rel_cbranch", 0x5000, &preview_options())
        .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}

#[test]
fn preview_supports_instruction_local_unconditional_branch_targets() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6000,
                    output: None,
                    inputs: vec![cst(2, 1)],
                    asm_mnemonic: Some("BRANCH <pcode+2>".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x6000,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: Some("RET 0".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x6000,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: Some("RET 1".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "rel_branch", 0x6000, &preview_options())
        .expect("preview render");
    // The single-predecessor label inlining pass eliminates the goto+label pair
    // since block_6000_dup2 has exactly one incoming reference (the unconditional
    // forward branch).  The unreachable `return 0;` is also removed, leaving only
    // `return 1;` as the sole surviving statement.
    assert!(code.contains("return 1;"), "{code}");
}

#[test]
fn preview_supports_instruction_local_unconditional_branch_targets_over_8() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6100,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6100,
                    output: None,
                    inputs: vec![cst(12, 1)],
                    asm_mnemonic: Some("BRANCH <pcode+12>".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6100,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 12,
                    opcode: PcodeOpcode::Return,
                    address: 0x6100,
                    output: None,
                    inputs: vec![cst(0, 4), cst(2, 4)],
                    asm_mnemonic: Some("RET 2".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "rel_branch_over_8",
        0x6100,
        &preview_options(),
    )
    .expect("preview render");
    // Depending on CFG normalization, the explicit goto may collapse into a direct return block.
    assert!(
        code.contains("goto block_6100_dup12;") || code.contains("return 2;"),
        "{code}"
    );
    assert!(code.contains("return 2;"), "{code}");
}

#[test]
fn preview_supports_instruction_local_conditional_branch_targets_over_8() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6200,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6200,
                    output: None,
                    inputs: vec![cst(12, 1), reg(0x206, 1)],
                    asm_mnemonic: Some("JZ <pcode+12>".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6200,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x6200,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: Some("RET 0".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6200,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 12,
                    opcode: PcodeOpcode::Return,
                    address: 0x6200,
                    output: None,
                    inputs: vec![cst(0, 4), cst(3, 4)],
                    asm_mnemonic: Some("RET 3".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "rel_cbranch_over_8",
        0x6200,
        &preview_options(),
    )
    .expect("preview render");
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 3;"), "{code}");
}

#[test]
fn preview_supports_instruction_local_unconditional_branch_targets_backward() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6300,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x6300,
                    output: None,
                    inputs: vec![cst(0, 4), cst(9, 4)],
                    asm_mnemonic: Some("RET 9".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6300,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6300,
                    output: None,
                    inputs: vec![cst(-2, 4)],
                    asm_mnemonic: Some("BRANCH <pcode-2>".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "rel_branch_backward",
        0x6300,
        &preview_options(),
    )
    .expect("preview render");
    // Backward local branches may surface as an explicit goto or a structured infinite loop.
    assert!(
        code.contains("goto block_6300;") || code.contains("while (1)"),
        "{code}"
    );
    assert!(code.contains("return 9;"), "{code}");
}
