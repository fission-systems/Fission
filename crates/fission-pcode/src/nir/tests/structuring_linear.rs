use super::*;
#[test]
fn lower_linear_body_caches_repeated_requests() {
    let ptr = uniq(0x470, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4700,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4700,
                    output: None,
                    inputs: vec![cst(0x4720, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4710,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4710,
                        output: Some(ptr.clone()),
                        inputs: vec![cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4711,
                        output: None,
                        inputs: vec![cst(0x4720, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4720,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4720,
                    output: None,
                    inputs: vec![cst(0, 4), ptr],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    let first = builder
        .lower_linear_body(1, LinearExit::Join(2))
        .expect("first lowering");
    assert!(builder.has_linear_body_cache(1, LinearExit::Join(2)));
    let cache_len = builder.linear_body_cache.len();
    let second = builder
        .lower_linear_body(1, LinearExit::Join(2))
        .expect("second lowering");
    assert_eq!(first, second);
    assert_eq!(builder.linear_body_cache.len(), cache_len);
}

#[test]

fn multi_block_preview_absorbs_shared_trivial_forwarding_return_tail() {
    let cond = uniq(0x3a0, 1);
    let ptr = uniq(0x3a1, 8);
    let phi = uniq(0x3a2, 4);
    let retv = uniq(0x3a3, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3650,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3650,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3651,
                        output: None,
                        inputs: vec![cst(0x3670, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3660,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3660,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3661,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3662,
                        output: None,
                        inputs: vec![cst(0x3680, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3670,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3670,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3671,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3672,
                        output: None,
                        inputs: vec![cst(0x3680, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3680,
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::MultiEqual,
                        address: 0x3680,
                        output: Some(phi.clone()),
                        inputs: vec![cst(0, 4), cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3681,
                        output: Some(retv.clone()),
                        inputs: vec![phi],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3682,
                        output: None,
                        inputs: vec![cst(0x3690, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x3690,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3690,
                    output: None,
                    inputs: vec![cst(0, 8), retv],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "if_else_tail_fn", 0x3650, &preview_options())
        .expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("local_10 = 1;"));
    assert!(code.contains("local_10 = 2;"));
    assert!(!code.contains("goto block_3680;"));
    assert!(!code.contains("goto block_3690;"));
}
