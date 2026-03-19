use super::*;

#[test]
fn preview_supports_instruction_local_conditional_branch_targets() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
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
    assert!(code.contains("goto block_6000_dup2;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}
