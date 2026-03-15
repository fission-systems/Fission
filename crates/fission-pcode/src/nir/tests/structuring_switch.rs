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
