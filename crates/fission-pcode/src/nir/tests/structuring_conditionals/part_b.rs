use super::super::*;
#[test]
fn region_recovery_succeeds_on_multi_hop_trampoline_join() {
    let cond = uniq(0x4f0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4f00,
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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
                successors: vec![],
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

#[test]
fn region_follow_discovery_accepts_non_monotonic_acyclic_window() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6300,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6300,
                    output: None,
                    inputs: vec![cst(0x6330, 8), reg(0x08, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6310,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6310,
                    output: None,
                    inputs: vec![cst(0x6340, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6320,
                    output: None,
                    inputs: vec![cst(0x6350, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6330,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6330,
                    output: None,
                    inputs: vec![cst(0x6310, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6340,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6340,
                    output: None,
                    inputs: vec![cst(0x6360, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 5,
                start_address: 0x6350,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6350,
                    output: None,
                    inputs: vec![cst(0x6360, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 6,
                start_address: 0x6360,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6360,
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
    let (shared, subtype) = builder.find_shared_tail_entries_for_region_for_test(0, 3, 1, 6);
    assert_eq!(subtype, None);
    assert_eq!(shared.first().copied(), Some(4));
}

#[test]
fn region_follow_discovery_rejects_local_cycle_without_index_shortcut() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6400,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6400,
                    output: None,
                    inputs: vec![cst(0x6420, 8), reg(0x08, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6410,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6410,
                    output: None,
                    inputs: vec![cst(0x6430, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6420,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6420,
                    output: None,
                    inputs: vec![cst(0x6430, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6430,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6430,
                    output: None,
                    inputs: vec![cst(0x6420, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x6440,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6440,
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
    let (shared, subtype) = builder.find_shared_tail_entries_for_region_for_test(0, 2, 1, 4);
    assert!(shared.is_empty());
    // Since Fission now detects the irreducible loop mathematically and isolates the
    // irregular back-edge, the remaining DAG falls back smoothly via SideEntryOrExit
    // without triggering the generic ComplexArmShape loop-tangling rejection.
    assert_eq!(subtype, Some("SideEntryOrExit"));
}

#[test]
fn test_return_duplication_removes_gotos_on_shared_returns() {
    let cond1 = uniq(0x650, 1);
    let cond2 = uniq(0x651, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6500,
                successors: vec![3, 1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6500,
                    output: None,
                    inputs: vec![cst(0x6530, 8), cond1],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6510,
                successors: vec![3, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x6510,
                    output: None,
                    inputs: vec![cst(0x6530, 8), cond2],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6520,
                successors: vec![0],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x6520,
                    output: None,
                    inputs: vec![cst(0x6500, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6530,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6530,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let code =
        render_mlil_preview(&func, "shared_return_loop", 0x6500, &options).expect("preview render");

    // With Return Duplication, the return block is cloned, allowing early returns to be structured without gotos
    assert!(
        !code.contains("goto"),
        "expected no gotos in structured loop output:\n{}",
        code
    );
    assert!(
        code.contains("return "),
        "expected return statement in output:\n{}",
        code
    );
}
