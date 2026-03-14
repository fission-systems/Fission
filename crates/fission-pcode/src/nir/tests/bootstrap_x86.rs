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
