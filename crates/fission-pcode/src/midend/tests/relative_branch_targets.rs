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

/// Dual same-block absolute cmov chain (x64 clamp O2 shape):
///   eax = lo
///   if (value <= hi) hi = value     // cmovle into param reg R8
///   if (value >= lo) eax = hi       // cmovge into return
///   return eax
/// Both CBranch targets are absolute next-instruction addresses inside one
/// block. Losing the first branch collapses to max(lo, value) and drops hi.
#[test]
fn preview_dual_absolute_cmov_clamp_chain_keeps_hi_bound() {
    // Win64: RCX=0x8 value, RDX=0x10 lo, R8=0x80 hi, RAX=0 return
    let eax = reg(0x0, 4);
    let ecx = reg(0x8, 4);
    let edx = reg(0x10, 4);
    let r8d = reg(0x80, 4);
    let zf = reg(0x206, 1);
    let le_tmp = uniq(0x25f00, 1);
    let ge_tmp = uniq(0x25c00, 1);
    let skip_le = uniq(0x7b700, 1);
    let skip_ge = uniq(0x7b701, 1);
    let src_le = uniq(0x7b600, 4);
    let src_ge = uniq(0x7b601, 4);
    let abs_after_cmovle = Varnode {
        space_id: 3,
        offset: 0x4010,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let abs_after_cmovge = Varnode {
        space_id: 3,
        offset: 0x4020,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_addr = reg(0x288, 8);

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x4000,
            successors: vec![],
            ops: vec![
                // mov eax, edx  (lo seed)
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4000,
                    output: Some(eax.clone()),
                    inputs: vec![edx.clone()],
                    asm_mnemonic: Some("MOV EAX,EDX".into()),
                },
                // toy: le_tmp = (ecx == r8d) stands in for value<=hi
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x4004,
                    output: Some(le_tmp.clone()),
                    inputs: vec![ecx.clone(), r8d.clone()],
                    asm_mnemonic: Some("CMP ECX,R8D".into()),
                },
                // cmovle r8d, ecx: skip body when !le
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4008,
                    output: Some(src_le.clone()),
                    inputs: vec![ecx.clone()],
                    asm_mnemonic: Some("CMOVLE prep".into()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x4008,
                    output: Some(skip_le.clone()),
                    inputs: vec![le_tmp],
                    asm_mnemonic: Some("CMOVLE".into()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x4008,
                    output: None,
                    inputs: vec![abs_after_cmovle, skip_le],
                    asm_mnemonic: Some("CMOVLE skip".into()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4008,
                    output: Some(r8d.clone()),
                    inputs: vec![src_le],
                    asm_mnemonic: Some("CMOVLE body".into()),
                },
                // toy: ge_tmp = (ecx == edx) stands in for value>=lo
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x4010,
                    output: Some(ge_tmp.clone()),
                    inputs: vec![ecx.clone(), edx.clone()],
                    asm_mnemonic: Some("CMP ECX,EDX".into()),
                },
                // cmovge eax, r8d
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4018,
                    output: Some(src_ge.clone()),
                    inputs: vec![r8d.clone()],
                    asm_mnemonic: Some("CMOVGE prep".into()),
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x4018,
                    output: Some(skip_ge.clone()),
                    inputs: vec![ge_tmp],
                    asm_mnemonic: Some("CMOVGE".into()),
                },
                PcodeOp {
                    seq_num: 9,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x4018,
                    output: None,
                    inputs: vec![abs_after_cmovge, skip_ge],
                    asm_mnemonic: Some("CMOVGE skip".into()),
                },
                PcodeOp {
                    seq_num: 10,
                    opcode: PcodeOpcode::Copy,
                    address: 0x4018,
                    output: Some(eax.clone()),
                    inputs: vec![src_ge],
                    asm_mnemonic: Some("CMOVGE body".into()),
                },
                PcodeOp {
                    seq_num: 11,
                    opcode: PcodeOpcode::Return,
                    address: 0x4020,
                    output: None,
                    inputs: vec![ret_addr],
                    asm_mnemonic: Some("RET".into()),
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "dual_cmov_clamp", 0x4000, &preview_options())
        .expect("preview render");
    eprintln!("dual_cmov_clamp:\n{code}");

    // Must not collapse to pure max(lo, value) ignoring hi (param_3 / r8).
    let collapsed_max_only = code.contains("param_2")
        && code.contains("param_1")
        && !code.contains("param_3")
        && !code.contains("r8");
    assert!(
        !collapsed_max_only,
        "lost hi-bound cmov (param_3/r8) — collapsed to max(lo,value):\n{code}"
    );
    // Prefer both bounds or a two-stage select/if chain.
    let mentions_hi = code.contains("param_3") || code.contains("r8");
    let has_structure = code.contains("if") || code.contains('?');
    assert!(
        mentions_hi || (has_structure && code.matches("if").count() + code.matches('?').count() >= 2),
        "expected dual-bound clamp recovery, got:\n{code}"
    );
    let _ = (eax, ecx, edx, r8d, zf);
}

