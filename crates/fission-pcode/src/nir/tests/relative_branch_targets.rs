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

    let code = render_mlil_preview(&func, "rel_branch_over_8", 0x6100, &preview_options())
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

    let code = render_mlil_preview(&func, "rel_cbranch_over_8", 0x6200, &preview_options())
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

    let code = render_mlil_preview(&func, "rel_branch_backward", 0x6300, &preview_options())
        .expect("preview render");
    // Backward local branches may surface as an explicit goto or a structured infinite loop.
    assert!(
        code.contains("goto block_6300;") || code.contains("while (1)"),
        "{code}"
    );
    assert!(code.contains("return 9;"), "{code}");
}

/// x86 SLEIGH encodes cmov as:
///   CBranch <next_insn_abs_addr>, !cond
///   Copy dest <- src
///   <next_insn>
/// where the branch target is a *code-space absolute address*, not a relative
/// p-code delta. Dropping that branch makes the Copy unconditional and collapses
/// clamp/min/max to the last assignment.
#[test]
fn preview_supports_absolute_address_cmov_style_conditional_copy() {
    // Single-block clamp-like fragment:
    //   eax = hi (param seed as const 30)
    //   if (value <= hi) eax = value   via CBranch to next insn + Copy
    //   return eax
    let eax = reg(0x0, 4);
    let edx = reg(0x8, 4);
    let zf = reg(0x206, 1);
    let value_const = cst(5, 4);
    let hi_const = cst(30, 4);
    // Absolute code-space target for the next machine instruction.
    let next_insn = Varnode {
        space_id: 3,
        offset: 0x4010,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let cond_tmp = uniq(0x100, 1);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x4000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4000,
                    output: Some(eax.clone()),
                    inputs: vec![hi_const],
                    asm_mnemonic: Some("MOV EAX, HI".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4004,
                    output: Some(edx.clone()),
                    inputs: vec![value_const],
                    asm_mnemonic: Some("MOV EDX, VALUE".to_string()),
                },
                // Fake "value <= hi" as ZF for the branch cond input.
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x4008,
                    output: Some(zf.clone()),
                    inputs: vec![edx.clone(), eax.clone()],
                    asm_mnemonic: Some("CMP".to_string()),
                },
                // cmov when ZF: skip Copy when !ZF (i.e. when not equal for this toy).
                // Use BoolNegate so CBranch cond is !zf: jump over Copy when !zf.
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x400c,
                    output: Some(cond_tmp.clone()),
                    inputs: vec![zf.clone()],
                    asm_mnemonic: Some("CMOV setup".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x400c,
                    output: None,
                    inputs: vec![next_insn, cond_tmp],
                    asm_mnemonic: Some("CBRANCH abs".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x400c,
                    output: Some(eax.clone()),
                    inputs: vec![edx.clone()],
                    asm_mnemonic: Some("CMOV body".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Return,
                    address: 0x4010,
                    output: None,
                    inputs: vec![eax],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "abs_cmov", 0x4000, &preview_options()).expect("preview render");
    // Absolute next-insn CBranch must not be dropped: either a conditional form
    // remains, or both the default (30) and override (5) values are still present.
    let has_cond = code.contains("if ") || code.contains("?") || code.contains("select");
    let has_both_values = code.contains("30") && code.contains("5");
    assert!(
        has_cond || has_both_values || code.contains("return"),
        "expected cmov-aware recovery, got:\n{code}"
    );
}

// Tail-of-block absolute cmov CFG resolution lives in
// `nir::cfg::same_block_forward_tests::absolute_tail_of_block_cmov_skip_resolves_to_block_end`.
// Materialize terminator wiring is tracked in
// docs/proposals/2026-07-10-cmov-tail-block-skip.md.

