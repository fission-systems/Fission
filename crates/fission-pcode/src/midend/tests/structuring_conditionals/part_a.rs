use super::super::*;
use fission_midend_structuring::{
    canonicalize_region_target_for_exit_for_test, find_shared_tail_entries_for_region_for_test,
    has_linear_body_cache, lower_linear_body, lower_linear_body_for_region_recovery_detailed,
};

#[test]
fn multi_block_preview_lowers_simple_if_without_failing() {
    let cond = uniq(0x300, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3000,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3001,
                        output: None,
                        inputs: vec![cst(0x3020, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3010,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3020,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "branchy", 0x3000, &preview_options()).expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("return 0;"));
    assert!(code.contains("return 1;"));
}

#[test]
fn x86_try_lower_if_still_structures_canonical_if() {
    let cond = uniq(0x430, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4300,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4300,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4301,
                        output: None,
                        inputs: vec![cst(0x4320, 4), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4310,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4310,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4320,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4320,
                    output: None,
                    inputs: vec![cst(0, 4), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_if", 0x4300, &preview_options_x86())
        .expect("preview render");
    // 32-bit mode: register_param is skipped (is_64bit guard), so the REGISTER_SPACE
    // varnode at offset 0x08 resolves to hardware name "rcx" via x64_ghidra_reg_name.
    assert!(
        code.contains("if (!rcx) {") || code.contains("if (rcx) {"),
        "expected if-branch in code:\n{code}"
    );
    assert!(code.contains("return 0;"), "{code}");
    assert!(code.contains("return 1;"), "{code}");
}

#[test]

fn x86_pathological_try_lower_if_falls_back_without_hanging() {
    let cond0 = uniq(0x480, 1);
    let cond1 = uniq(0x481, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4800,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4800,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4801,
                        output: None,
                        inputs: vec![cst(0x4820, 4), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4810,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4810,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x09, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4811,
                        output: None,
                        inputs: vec![cst(0x4810, 4), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4820,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4820,
                    output: None,
                    inputs: vec![cst(0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "x86_path_if", 0x4800, &preview_options_x86())
        .expect("preview render");
    assert!(code.contains("return "), "{code}");
    assert!(
        code.contains("do {") || code.contains("block_4810") || code.contains("goto block_4810;"),
        "{code}"
    );
}

#[test]

fn multi_block_preview_lowers_conditional_goto_style_if() {
    let cond = uniq(0x340, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3400,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3400,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3401,
                        output: None,
                        inputs: vec![cst(0x3420, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3410,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3410,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3420,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3420,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "cond_goto_if", 0x3400, &preview_options())
        .expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("return 0;"));
    assert!(code.contains("return 1;"));
}

#[test]
fn nested_conditionals_preserve_joined_return_register_value() {
    let value = reg(0x08, 4);
    let lo = reg(0x10, 4);
    let hi = reg(0x80, 4);
    let eax = reg(0x00, 4);
    let rax = reg(0x00, 8);
    let cond_lo = uniq(0x510, 1);
    let cond_hi = uniq(0x520, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5100,
                successors: vec![1, 2],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5100,
                        output: Some(eax.clone()),
                        inputs: vec![value.clone()],
                        asm_mnemonic: Some("mov eax,ecx".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x5100,
                        output: Some(rax.clone()),
                        inputs: vec![eax.clone()],
                        asm_mnemonic: Some("mov eax,ecx".to_string()),
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5100,
                        output: Some(cond_lo.clone()),
                        inputs: vec![reg(0x30, 1)],
                        asm_mnemonic: Some("cmp/jge".to_string()),
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5101,
                        output: None,
                        inputs: vec![cst(0x5120, 8), cond_lo],
                        asm_mnemonic: Some("jge".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5110,
                successors: vec![5],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5110,
                        output: Some(eax.clone()),
                        inputs: vec![lo],
                        asm_mnemonic: Some("mov eax,edx".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x5110,
                        output: Some(rax.clone()),
                        inputs: vec![eax.clone()],
                        asm_mnemonic: Some("mov eax,edx".to_string()),
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5112,
                        output: None,
                        inputs: vec![cst(0x5150, 8)],
                        asm_mnemonic: Some("jmp".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5120,
                successors: vec![3, 4],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5120,
                        output: Some(cond_hi.clone()),
                        inputs: vec![reg(0x31, 1)],
                        asm_mnemonic: Some("cmp/jle".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5121,
                        output: None,
                        inputs: vec![cst(0x5140, 8), cond_hi],
                        asm_mnemonic: Some("jle".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5130,
                successors: vec![5],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5130,
                        output: Some(eax.clone()),
                        inputs: vec![hi],
                        asm_mnemonic: Some("mov eax,r8d".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x5130,
                        output: Some(rax.clone()),
                        inputs: vec![eax.clone()],
                        asm_mnemonic: Some("mov eax,r8d".to_string()),
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5132,
                        output: None,
                        inputs: vec![cst(0x5150, 8)],
                        asm_mnemonic: Some("jmp".to_string()),
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x5140,
                successors: vec![5],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5140,
                        output: Some(eax.clone()),
                        inputs: vec![value],
                        asm_mnemonic: Some("mov eax,ecx".to_string()),
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x5140,
                        output: Some(rax.clone()),
                        inputs: vec![eax.clone()],
                        asm_mnemonic: Some("mov eax,ecx".to_string()),
                    },
                ],
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
                    inputs: vec![cst(0, 8), rax],
                    asm_mnemonic: Some("ret".to_string()),
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "clamp_like", 0x5100, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("param_1") && code.contains("param_2") && code.contains("param_3"),
        "expected all three return candidates to survive:\n{code}"
    );
    assert!(
        code.contains("if ") || code.contains(" ? "),
        "expected conditional structure for clamp-like return selection:\n{code}"
    );
}

#[test]
fn multi_block_preview_lowers_canonical_if_else() {
    let cond = uniq(0x350, 1);
    let ptr = uniq(0x360, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3500,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3500,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3501,
                        output: None,
                        inputs: vec![cst(0x3520, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3510,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3510,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3511,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3512,
                        output: None,
                        inputs: vec![cst(0x3530, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3520,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3520,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3521,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(2, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3530,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3530,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "if_else_fn", 0x3500, &preview_options())
        .expect("preview render");
    println!("CODE IS:\n{}", code);
    let is_ternary = code.contains("local_10 = !param_1 ? 1 : 2;")
        || code.contains("local_10 = param_1 ? 2 : 1;");
    let is_ifelse = (code.contains("if (!param_1) {") && code.contains("local_10 = 1;"))
        || (code.contains("if (param_1) {") && code.contains("local_10 = 2;"));
    assert!(
        is_ternary || is_ifelse,
        "Expected ternary select or canonical if-else. Code:\n{}",
        code
    );
    assert!(!code.contains("goto block_3510;"));
    assert!(!code.contains("goto block_3520;"));
}

#[test]
fn multi_block_preview_prefers_short_circuit_or_over_nested_plain_if() {
    let cond0 = uniq(0x354, 1);
    let cond1 = uniq(0x355, 1);
    let ptr = uniq(0x356, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3540,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3540,
                        output: Some(cond0.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3541,
                        output: None,
                        inputs: vec![cst(0x3570, 8), cond0],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3550,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3550,
                        output: Some(cond1.clone()),
                        inputs: vec![reg(0x09, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3551,
                        output: None,
                        inputs: vec![cst(0x3570, 8), cond1],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3560,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x3560,
                    output: None,
                    inputs: vec![cst(0x3580, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3570,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3570,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3571,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3572,
                        output: None,
                        inputs: vec![cst(0x3580, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x3580,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3580,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "short_or_fn", 0x3540, &preview_options())
        .expect("preview render");
    assert!(code.contains("||"), "{code}");
    assert!(code.contains("local_10 = 1;"), "{code}");
    assert!(!code.contains("goto block_3550;"), "{code}");
}

#[test]
fn multi_block_preview_lowers_if_else_with_multi_block_then_region() {
    let cond = uniq(0x370, 1);
    let ptr = uniq(0x380, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3600,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3600,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3601,
                        output: None,
                        inputs: vec![cst(0x3630, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3610,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3610,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3611,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3620,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3620,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3621,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3622,
                        output: None,
                        inputs: vec![cst(0x3640, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3630,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3630,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3631,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3632,
                        output: None,
                        inputs: vec![cst(0x3640, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x3640,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3640,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "if_else_chain_fn", 0x3600, &preview_options())
        .expect("preview render");
    assert!(code.contains("if (!param_1) {") || code.contains("if (param_1) {"));
    assert!(code.contains("local_10 = 1;"));
    assert!(code.contains("local_10 = 2;"));
    assert!(code.contains("} else {"));
    assert!(code.contains("local_10 = 3;"));
    assert!(!code.contains("goto block_3620;"));
    assert!(!code.contains("goto block_3630;"));
}

#[test]

fn multi_block_preview_folds_short_circuit_and() {
    let cond_a = uniq(0x390, 1);
    let cond_b = uniq(0x391, 1);
    let ptr = uniq(0x392, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3700,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3700,
                        output: Some(cond_a.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3701,
                        output: None,
                        inputs: vec![cst(0x3730, 8), cond_a],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3710,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3710,
                        output: Some(cond_b.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3711,
                        output: None,
                        inputs: vec![cst(0x3730, 8), cond_b],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3720,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3720,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3721,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3722,
                        output: None,
                        inputs: vec![cst(0x3730, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3730,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3730,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "short_and_fn", 0x3700, &preview_options())
        .expect("preview render");
    assert!(code.contains("&&"));
    assert!(code.contains("local_10 = 7;"));
    assert!(!code.contains("goto block_3730;"));
}

#[test]
fn multi_block_preview_folds_short_circuit_or() {
    let cond_a = uniq(0x3a0, 1);
    let cond_b = uniq(0x3a1, 1);
    let ptr = uniq(0x3a2, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x3800,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3800,
                        output: Some(cond_a.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3801,
                        output: None,
                        inputs: vec![cst(0x3830, 8), cond_a],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x3810,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x3810,
                        output: Some(cond_b.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x3811,
                        output: None,
                        inputs: vec![cst(0x3830, 8), cond_b],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x3820,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x3820,
                    output: None,
                    inputs: vec![cst(0x3840, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x3830,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x3830,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x3831,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(9, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x3832,
                        output: None,
                        inputs: vec![cst(0x3840, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x3840,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x3840,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "short_or_fn", 0x3800, &preview_options())
        .expect("preview render");
    assert!(code.contains("||"), "{code}");
    assert!(code.contains("local_10 = 9;"));
    assert!(!code.contains("goto block_3830;"));
}

#[test]
fn region_recovery_succeeds_on_one_arm_forwarding_join() {
    let cond = uniq(0x4c0, 1);
    let side = uniq(0x4c1, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4c00,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4c00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4c01,
                        output: None,
                        inputs: vec![cst(0x4c20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4c10,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4c10,
                        output: Some(side.clone()),
                        inputs: vec![cst(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4c11,
                        output: None,
                        inputs: vec![cst(0x4c40, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4c20,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4c20,
                    output: None,
                    inputs: vec![cst(0x4c30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4c30,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4c30,
                    output: None,
                    inputs: vec![cst(0x4c40, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x4c40,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4c40,
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
    let lowered = lower_linear_body_for_region_recovery_detailed(&mut builder,
        0, LinearExit::Join(4), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_succeeds_on_trampoline_tail_shared_join() {
    let cond = uniq(0x4d0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4d00,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4d00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4d01,
                        output: None,
                        inputs: vec![cst(0x4d20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4d10,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d10,
                    output: None,
                    inputs: vec![cst(0x4d30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4d20,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d20,
                    output: None,
                    inputs: vec![cst(0x4d30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4d30,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4d30,
                    output: None,
                    inputs: vec![cst(0x4d40, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 4,
                start_address: 0x4d40,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4d40,
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
    let lowered = lower_linear_body_for_region_recovery_detailed(&mut builder,
        0, LinearExit::Join(4), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}

#[test]
fn region_recovery_succeeds_on_two_arm_nearby_join() {
    let cond = uniq(0x4e0, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4e00,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4e00,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4e01,
                        output: None,
                        inputs: vec![cst(0x4e20, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4e10,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4e10,
                    output: None,
                    inputs: vec![cst(0x4e30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4e20,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x4e20,
                    output: None,
                    inputs: vec![cst(0x4e30, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4e30,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4e30,
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
    let lowered = lower_linear_body_for_region_recovery_detailed(&mut builder,
        0, LinearExit::Join(3), None)
        .expect("region detailed lowering should not error");
    assert!(matches!(lowered, LinearBodyLoweringOutcome::Lowered(_)));
}
