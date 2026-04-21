use super::*;

#[test]
fn preview_supports_pe_x86_single_block() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x401000,
            successors: vec![],
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
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4000,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
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
                successors: vec![],
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
                successors: vec![],
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
            successors: vec![],
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

#[test]
fn preview_tolerates_branchind_without_targets() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::BranchInd,
                address: 0x405000,
                output: None,
                inputs: vec![reg(0x00, 4)],
                asm_mnemonic: Some("JMP EAX".to_string()),
            }],
        }],
    };

    let code = render_mlil_preview(
        &func,
        "x86_branchind_unsupported",
        0x405000,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_branchind("), "{code}");
}

#[test]
fn preview_branchind_with_successors_sets_default_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405010,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405010,
                    output: None,
                    inputs: vec![reg(0x00, 4)],
                    asm_mnemonic: Some("JMP EAX".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x405030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405030,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Switch {
            targets,
            default_target,
            ..
        } => {
            assert_eq!(targets, vec![0x405020, 0x405030]);
            assert_eq!(default_target, Some(0x405020));
        }
        other => panic!("expected switch terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_with_duplicate_successors_preserves_case_ordinals() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405110,
                successors: vec![1, 1, 2, 1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405110,
                    output: None,
                    inputs: vec![reg(0x00, 4)],
                    asm_mnemonic: Some("JMP EAX".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x405130,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405130,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Switch {
            targets,
            default_target,
            ..
        } => {
            assert_eq!(targets, vec![0x405120, 0x405120, 0x405130, 0x405120]);
            assert_eq!(default_target, Some(0x405120));
        }
        other => panic!("expected switch terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_without_successors_recovers_constant_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405100,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405100,
                    output: None,
                    inputs: vec![cst(0x405120, 8)],
                    asm_mnemonic: Some("JMP [CONST]".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(2, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets, vec![0x405120]);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }
}

#[test]
fn preview_tolerates_unresolved_direct_branch_target() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x406000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Branch,
                address: 0x406000,
                output: None,
                inputs: vec![cst(0x405000, 8)],
                asm_mnemonic: Some("JMP 0x405000".to_string()),
            }],
        }],
    };

    let code = render_mlil_preview(
        &func,
        "x86_unresolved_direct_branch",
        0x406000,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_branchind("), "{code}");
}

#[test]
fn preview_unresolved_direct_branch_with_single_successor_uses_successor_target() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x406100,
                successors: vec![1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x406100,
                    output: None,
                    inputs: vec![cst(0x499999, 8)],
                    asm_mnemonic: Some("JMP unresolved".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406110,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406110,
                    output: None,
                    inputs: vec![cst(0, 4), cst(3, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406110),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_branch_target_copy_wrapper_recovers_direct_target() {
    let wrapped_target = reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x406200,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x406200,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![cst(0x406220, 8)],
                        asm_mnemonic: Some("MOV target, 0x406220".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x406201,
                        output: None,
                        inputs: vec![wrapped_target],
                        asm_mnemonic: Some("JMP wrapped_target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406220,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406220,
                    output: None,
                    inputs: vec![cst(0, 4), cst(4, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406220),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_cbranch_target_copy_wrapper_recovers_direct_target() {
    let wrapped_target = reg(0x00, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x407100,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x407100,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![cst(0x407120, 8)],
                        asm_mnemonic: Some("MOV target, 0x407120".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x407101,
                        output: None,
                        inputs: vec![wrapped_target, reg(0x206, 1)],
                        asm_mnemonic: Some("JNZ wrapped_target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x407110,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407110,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407120,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407120,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407120);
            assert_eq!(false_target, Some(0x407110));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_branch_target_intadd_wrapper_recovers_direct_target() {
    let wrapped_target = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let base_target = Varnode {
        space_id: 1,
        offset: 0x406300,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4062e0,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4062e0,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![base_target, cst(0x20, 8)],
                        asm_mnemonic: Some("LEA target, base+0x20".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4062e1,
                        output: None,
                        inputs: vec![wrapped_target],
                        asm_mnemonic: Some("JMP target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x406320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x406320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(5, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Goto(target) => assert_eq!(target, 0x406320),
        other => panic!("expected goto terminator, got {other:?}"),
    }
}

#[test]
fn preview_cbranch_target_intadd_wrapper_recovers_direct_target() {
    let wrapped_target = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let base_target = Varnode {
        space_id: 1,
        offset: 0x407300,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4072e0,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4072e0,
                        output: Some(wrapped_target.clone()),
                        inputs: vec![base_target, cst(0x20, 8)],
                        asm_mnemonic: Some("LEA target, base+0x20".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4072e1,
                        output: None,
                        inputs: vec![wrapped_target, reg(0x206, 1)],
                        asm_mnemonic: Some("JNZ target".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4072f0,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4072f0,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407320);
            assert_eq!(false_target, Some(0x4072f0));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_branchind_single_target_degrades_to_dispatcher_surface() {
    let switch_var = Varnode {
        space_id: 3,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let target_addr = Varnode {
        space_id: 1,
        offset: 0x405220,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x405200,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0x405200,
                        output: Some(switch_var.clone()),
                        inputs: vec![cst(0, 8), target_addr],
                        asm_mnemonic: Some("LOAD target from table".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::BranchInd,
                        address: 0x405201,
                        output: None,
                        inputs: vec![switch_var],
                        asm_mnemonic: Some("JMP_IND".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x405220,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x405220,
                    output: None,
                    inputs: vec![cst(0, 4), cst(6, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets, vec![0x405220]);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }

    let code = render_mlil_preview(
        &func,
        "dispatcher_single_target",
        0x405200,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_dispatcher_indirect("), "{code}");
}

#[test]
fn preview_branchind_self_loop_global_load_prefers_dispatcher_surface() {
    let switch_var = uniq(0x900, 8);

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x405300,
            successors: vec![0],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x405300,
                    output: Some(switch_var.clone()),
                    inputs: vec![cst(0, 8), cst(0x401380, 8)],
                    asm_mnemonic: Some("LOAD dispatcher slot".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x405301,
                    output: None,
                    inputs: vec![switch_var],
                    asm_mnemonic: Some("JMP_IND".to_string()),
                },
            ],
        }],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Unsupported {
            evidence,
            target_expr,
        } => {
            assert_eq!(evidence.surface, IndirectControlSurface::DispatcherLike);
            assert_eq!(
                evidence.failure_family,
                UnsupportedControlFamily::NonStructuralDispatcher
            );
            assert_eq!(evidence.successor_targets.len(), 1);
            assert!(target_expr.is_some());
        }
        other => panic!("expected dispatcher surface, got {other:?}"),
    }
    let stats = builder.preview_build_stats();
    assert_eq!(stats.dispatcher_shape_recovered_count, 1);
    assert_eq!(stats.indirect_target_set_refined_count, 1);

    let code = render_mlil_preview(
        &func,
        "dispatcher_self_loop_global_load",
        0x405300,
        &preview_options_x86(),
    )
    .expect("preview render");
    assert!(code.contains("__fission_dispatcher_indirect("), "{code}");
    assert!(!code.contains("switch ("), "{code}");
}

#[test]
fn preview_unresolved_cbranch_uses_unique_non_fallthrough_successor() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x407000,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x407000,
                    output: None,
                    inputs: vec![cst(0x4AAAAA, 8), reg(0x206, 1)],
                    asm_mnemonic: Some("JNZ unresolved".to_string()),
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x407010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x407020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x407020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let options = preview_options_x86();
    let mut builder = PreviewBuilder::new(&func, &options, None);
    match builder
        .lower_block_terminator(0)
        .expect("terminator lowering")
    {
        LoweredTerminator::Cond {
            true_target,
            false_target,
            ..
        } => {
            assert_eq!(true_target, 0x407020);
            assert_eq!(false_target, Some(0x407010));
        }
        other => panic!("expected conditional terminator, got {other:?}"),
    }
}

#[test]
fn preview_build_stats_records_structuring_duration() {
    let cond = uniq(0x361, 1);
    let direct_target = Varnode {
        space_id: 1,
        offset: 0x5020,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5000,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5001,
                        output: None,
                        inputs: vec![direct_target, cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
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
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let _ = render_mlil_preview(
        &func,
        "x86_structuring_stats",
        0x5000,
        &preview_options_x86(),
    )
    .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.max_structuring_scc_component_size, 1);
    assert!(stats.structuring_scc_component_count >= 1);
    assert!(stats.structuring_duration_ms <= stats.build_duration_ms);
}

#[test]
fn preview_build_stats_records_render_duration() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x503000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x503000,
                output: None,
                inputs: vec![cst(0, 4), cst(7, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let start = std::time::Instant::now();
    let _ = render_mlil_preview(
        &func,
        "x86_render_duration",
        0x503000,
        &preview_options_x86(),
    )
    .expect("preview render");
    let elapsed_ms = start.elapsed().as_millis() as usize;
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert!(stats.render_duration_ms <= elapsed_ms);
}

#[test]
fn preview_build_stats_records_rendered_code_len() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x504000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x504000,
                output: None,
                inputs: vec![cst(0, 4), cst(9, 4)],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "x86_render_len", 0x504000, &preview_options_x86())
        .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.rendered_code_len, code.len());
}

#[test]
fn preview_build_stats_records_max_structuring_scc_component_size() {
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x505000,
                successors: vec![1, 2],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x505000,
                    output: None,
                    inputs: vec![cst(0x505020, 4), reg(0x206, 1)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x505010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x505010,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x505020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x505020,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let _ = render_mlil_preview(&func, "x86_scc_size", 0x505000, &preview_options_x86())
        .expect("preview render");
    let stats = take_last_preview_build_stats().expect("preview build stats");
    assert_eq!(stats.max_structuring_scc_component_size, 1);
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
            successors: vec![],
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
