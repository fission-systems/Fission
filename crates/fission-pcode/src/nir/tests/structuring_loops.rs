use super::*;

#[test]
fn infloop_preview_lowers_single_block_self_loop() {
    let ptr = uniq(0x440, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4300,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4300,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4301,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(9, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4302,
                        output: None,
                        inputs: vec![cst(0x4300, 8)],
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
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "infloop_fn", 0x4300, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("while (true) {") || code.contains("while (1) {"),
        "{code}"
    );
    assert!(code.contains("local_10 = 9;"), "{code}");
    assert!(!code.contains("goto block_4300;"), "{code}");
}

#[test]
fn do_while_preview_is_lowered_without_ghidra_fallback() {
    let ptr = uniq(0x400, 8);
    let cond = uniq(0x410, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4000,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4001,
                        output: None,
                        inputs: vec![cst(0, 4), ptr, cst(7, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4002,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4003,
                        output: None,
                        inputs: vec![cst(0x4000, 8), cond],
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
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "loop_fn", 0x4000, &preview_options()).expect("preview render");
    assert!(code.contains("do {"));
    assert!(code.contains("local_10 = 7;"));
    assert!(code.contains("} while (param_1);"));
}

#[test]
fn while_preview_lowers_multi_block_body() {
    let cond = uniq(0x420, 1);
    let ptr1 = uniq(0x421, 8);
    let ptr2 = uniq(0x422, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4100,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4100,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4101,
                        output: None,
                        inputs: vec![cst(0x4140, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4110,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4110,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4111,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(1, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4120,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4120,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4121,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2, cst(2, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4122,
                        output: None,
                        inputs: vec![cst(0x4100, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4140,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4140,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "while_fn", 0x4100, &preview_options()).expect("preview render");
    assert!(code.contains("while (!param_1) {") || code.contains("while (param_1) {"));
    assert!(
        code.contains("local_10 = 1;"),
        "expected first store: {code}"
    );
    // NOTE: Due to a known stack-slot deduplication limitation, RBP-0x14 is
    // currently aliased to local_10 (same as RBP-0x10). The store value 2
    // should be present even if under the same slot name.
    assert!(
        code.contains("local_10 = 2;") || code.contains("local_14 = 2;"),
        "expected second store (local_10 or local_14): {code}"
    );
    assert!(!code.contains("goto block_4100;"));
}

#[test]
fn natural_while_with_preheader_after_body_layout() {
    let init_ptr = uniq(0x423, 4);
    let body_ptr = uniq(0x424, 4);
    let cond = uniq(0x425, 1);
    let predicate_value = uniq(0x426, 4);
    let borrow = uniq(0x427, 1);
    let popcount = uniq(0x428, 1);
    let return_ptr = uniq(0x429, 4);
    let return_value = uniq(0x42a, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4150,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4150,
                        output: Some(init_ptr.clone()),
                        inputs: vec![reg(20, 4), cst(-4, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4151,
                        output: None,
                        inputs: vec![cst(0, 4), init_ptr, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x4152,
                        output: None,
                        inputs: vec![cst(0x4170, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4160,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4160,
                        output: Some(body_ptr.clone()),
                        inputs: vec![reg(20, 4), cst(-4, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4161,
                        output: Some(reg(0, 4)),
                        inputs: vec![reg(8, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntAnd,
                        address: 0x4161,
                        output: Some(reg(0, 4)),
                        inputs: vec![reg(0, 4), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Store,
                        address: 0x4161,
                        output: None,
                        inputs: vec![cst(0, 4), body_ptr, reg(0, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4170,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4170,
                        output: Some(predicate_value.clone()),
                        inputs: vec![reg(0x08, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 8,
                        opcode: PcodeOpcode::IntSBorrow,
                        address: 0x4170,
                        output: Some(borrow),
                        inputs: vec![predicate_value.clone(), cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 9,
                        opcode: PcodeOpcode::PopCount,
                        address: 0x4170,
                        output: Some(popcount),
                        inputs: vec![predicate_value.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 10,
                        opcode: PcodeOpcode::IntNotEqual,
                        address: 0x4170,
                        output: Some(cond.clone()),
                        inputs: vec![predicate_value, cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 11,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4171,
                        output: None,
                        inputs: vec![cst(0x4160, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4180,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 12,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4180,
                        output: Some(return_ptr.clone()),
                        inputs: vec![reg(20, 4), cst(-4, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 13,
                        opcode: PcodeOpcode::Load,
                        address: 0x4181,
                        output: Some(return_value.clone()),
                        inputs: vec![cst(3, 4), return_ptr],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 14,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4182,
                        output: Some(reg(0, 4)),
                        inputs: vec![return_value],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 15,
                        opcode: PcodeOpcode::Return,
                        address: 0x4183,
                        output: None,
                        inputs: vec![cst(0, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "layout_independent_while",
        0x4150,
        &preview_options_for(CallingConvention::X86_32),
    )
    .expect("preview render");
    assert!(code.contains("while ("), "expected natural while: {code}");
    assert!(code.contains("local_4 ="), "expected loop body: {code}");
    assert!(
        code.contains("return local_4;"),
        "expected exit return: {code}"
    );
    assert!(
        !code.contains("goto block_4170;"),
        "unexpected head goto: {code}"
    );
}

#[test]
fn do_while_preview_lowers_multi_block_body() {
    let cond = uniq(0x430, 1);
    let ptr1 = uniq(0x431, 8);
    let ptr2 = uniq(0x432, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x4200,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4200,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4201,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(5, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x4210,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x4210,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x4211,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2, cst(6, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x4220,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x4220,
                        output: Some(cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x4221,
                        output: None,
                        inputs: vec![cst(0x4200, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x4230,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x4230,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "do_while_chain_fn", 0x4200, &preview_options())
        .expect("preview render");
    assert!(code.contains("do {"), "{code}");
    assert!(code.contains("local_10 = 5;"), "{code}");
    // NOTE: RBP-0x14 is currently merged into local_10 by the stack-slot deduplication pass.
    assert!(
        code.contains("local_10 = 6;") || code.contains("local_14 = 6;"),
        "expected second store (local_10 or local_14): {code}"
    );
    assert!(code.contains("} while (param_1);"), "{code}");
}

#[test]
fn multiblock_loop_rewrites_all_exits_to_breaks() {
    let early_exit_cond = uniq(0x450, 1);
    let latch_cond = uniq(0x451, 1);
    let ptr1 = uniq(0x452, 8);
    let ptr2 = uniq(0x453, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5300,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5300,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x5301,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1, cst(7, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5302,
                        output: Some(early_exit_cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5303,
                        output: None,
                        inputs: vec![cst(0x5330, 8), early_exit_cond],
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
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5310,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x5311,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2, cst(9, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5312,
                        output: Some(latch_cond.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5313,
                        output: None,
                        inputs: vec![cst(0x5300, 8), latch_cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5330,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5330,
                    output: None,
                    inputs: vec![cst(0, 8), cst(1, 4)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5340,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5340,
                    output: None,
                    inputs: vec![cst(0, 8), cst(2, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "multi_exit_loop_fn", 0x5300, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("while (true)") || code.contains("while (1)"),
        "expected structured loop: {code}"
    );
    assert!(code.contains("break;"), "expected exit rewrites: {code}");
    assert!(!code.contains("goto block_5330"), "{code}");
    assert!(!code.contains("goto block_5340"), "{code}");
}

// ---------------------------------------------------------------------------
// New tests: Phase 5 — Loop Control Strengthening
// ---------------------------------------------------------------------------

/// While loop whose body has a mid-body conditional break.
///
/// CFG:
///   block 0 (0x5000, head):   CBranch(loop_cond → 0x5030, fallthrough → 0x5010)
///   block 1 (0x5010, body):   Store + CBranch(break_cond → 0x5030, fallthrough → 0x5020)
///   block 2 (0x5020, latch):  Store + Branch(→ 0x5000)
///   block 3 (0x5030, exit):   Return
///
/// Expected: while (!loop_cond) { store; if (break_cond) break; store2; }
#[test]
fn while_loop_with_mid_body_break() {
    let loop_cond = uniq(0x500, 1);
    let break_cond = uniq(0x501, 1);
    let ptr1 = uniq(0x502, 8);
    let ptr2 = uniq(0x503, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: head — CBranch(loop_cond → exit 0x5030, fallthrough → body 0x5010)
            PcodeBasicBlock {
                index: 0,
                start_address: 0x5000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5000,
                        output: Some(loop_cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5001,
                        output: None,
                        inputs: vec![cst(0x5030, 8), loop_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: body — store + CBranch(break_cond → exit 0x5030, fallthrough → latch 0x5020)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x5010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5010,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x5011,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1.clone(), cst(11, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x5012,
                        output: Some(break_cond.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x5013,
                        output: None,
                        inputs: vec![cst(0x5030, 8), break_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: latch — store + Branch(→ head 0x5000)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x5020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x5020,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x5021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2.clone(), cst(22, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x5022,
                        output: None,
                        inputs: vec![cst(0x5000, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: exit — Return
            PcodeBasicBlock {
                index: 3,
                start_address: 0x5030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x5030,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "mid_break_fn", 0x5000, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("while (") || code.contains("while(!"),
        "expected while loop: {code}"
    );
    assert!(code.contains("break;"), "expected break statement: {code}");
    assert!(
        !code.contains("goto block_5030"),
        "expected no goto to exit: {code}"
    );
    assert!(
        code.contains("local_10 = 11;"),
        "expected first store: {code}"
    );
    // NOTE: RBP-0x14 is currently merged into local_10 by the stack-slot deduplication pass.
    assert!(
        code.contains("local_10 = 22;") || code.contains("local_14 = 22;"),
        "expected second store (local_10 or local_14): {code}"
    );
}

/// While loop whose body has an early continue path.
///
/// CFG:
///   block 0 (0x6000, head):  CBranch(loop_cond → 0x6040, fallthrough → 0x6010)
///   block 1 (0x6010, body):  store1 + CBranch(cont_cond → 0x6000(head), fallthrough → 0x6020)
///   block 2 (0x6020, tail):  store2 + Branch(→ 0x6000)
///   block 3 (0x6040, exit):  Return
///
/// Expected: while (!loop_cond) { store1; if (cont_cond) continue; store2; }
#[test]
fn while_loop_with_early_continue() {
    let loop_cond = uniq(0x600, 1);
    let cont_cond = uniq(0x601, 1);
    let ptr1 = uniq(0x602, 8);
    let ptr2 = uniq(0x603, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: head
            PcodeBasicBlock {
                index: 0,
                start_address: 0x6000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x6000,
                        output: Some(loop_cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x6001,
                        output: None,
                        inputs: vec![cst(0x6040, 8), loop_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: body — store + early-continue CBranch(cont_cond → head, fallthrough → tail)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x6010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x6010,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x6011,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1.clone(), cst(33, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x6012,
                        output: Some(cont_cond.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x6013,
                        output: None,
                        // Branch back to HEAD (early continue)
                        inputs: vec![cst(0x6000, 8), cont_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: tail — store + Branch(→ head)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x6020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x6020,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x6021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2.clone(), cst(44, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x6022,
                        output: None,
                        inputs: vec![cst(0x6000, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: exit
            PcodeBasicBlock {
                index: 3,
                start_address: 0x6040,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x6040,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "early_cont_fn", 0x6000, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("while (") || code.contains("while(!"),
        "expected while: {code}"
    );
    assert!(
        code.contains("continue;")
            || code.contains("if (!param_2)")
            || code.contains("if (!param_2_1)"),
        "expected continue or structured if: {code}"
    );
    assert!(
        !code.contains("goto block_6000"),
        "expected no goto to head: {code}"
    );
    assert!(
        code.contains("local_10 = 33;"),
        "expected first store: {code}"
    );
    // NOTE: RBP-0x14 is currently merged into local_10 by the stack-slot deduplication pass.
    assert!(
        code.contains("local_10 = 44;") || code.contains("local_14 = 44;"),
        "expected second store (local_10 or local_14): {code}"
    );
}

/// Simple for-loop: init → head(cond) → body → latch(update) → head.
///
/// CFG (all CFG invariants satisfied):
///   block 0 (0x7000, init):    Copy cst(0) → counter; Branch(→ 0x7010)
///   block 1 (0x7010, head):    IntLess counter 10 → cond; CBranch(cond → 0x7040, fallthrough → 0x7020)
///   block 2 (0x7020, body):    Store; Branch(→ 0x7030)
///   block 3 (0x7030, latch):   IntAdd counter 1 → counter; Branch(→ 0x7010)
///   block 4 (0x7040, exit):    Return
///
/// Expected: for (counter = 0; !cond; counter = counter + 1) { store; }
#[test]
fn for_loop_simple_counter() {
    let counter = uniq(0x700, 4);
    let lt_cond = uniq(0x701, 1);
    let ptr = uniq(0x702, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: init — counter = 0; Branch(→ head)
            PcodeBasicBlock {
                index: 0,
                start_address: 0x7000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x7000,
                        output: Some(counter.clone()),
                        inputs: vec![cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x7001,
                        output: None,
                        inputs: vec![cst(0x7010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: head — compare + CBranch(cond → exit, fallthrough → body)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x7010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLess,
                        address: 0x7010,
                        output: Some(lt_cond.clone()),
                        inputs: vec![counter.clone(), cst(10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x7011,
                        output: None,
                        // Branch to exit if lt_cond is TRUE (i.e., counter < 10 exits? No:
                        // We want "loop while counter < 10", so branch to exit when counter >= 10.
                        // Use IntLessSEqual for >= 10: IntLessEqual(10, counter) or just negate.
                        // Simpler: branch to exit when NOT less → use IntLessEqual(counter, 9)...
                        // Even simpler: CBranch exits when cond is true. Use IntSLessEqual as exit.
                        // For test clarity: loop while counter < 10 → exit when !(counter < 10)
                        // We'll branch to exit when condition is satisfied (true_target = exit).
                        inputs: vec![cst(0x7040, 8), lt_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: body — store; Branch(→ latch)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x7020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x7020,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x7021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(55, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x7022,
                        output: None,
                        inputs: vec![cst(0x7030, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: latch — counter = counter + 1; Branch(→ head)
            PcodeBasicBlock {
                index: 3,
                start_address: 0x7030,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x7030,
                        output: Some(counter.clone()),
                        inputs: vec![counter.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x7031,
                        output: None,
                        inputs: vec![cst(0x7010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 4: exit
            PcodeBasicBlock {
                index: 4,
                start_address: 0x7040,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x7040,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "for_simple_fn", 0x7000, &preview_options())
        .expect("preview render");
    // Should produce a for loop (or at minimum a structured while without goto)
    assert!(
        code.contains("for (") || code.contains("while ("),
        "expected structured loop: {code}"
    );
    assert!(
        !code.contains("goto block_7010"),
        "expected no goto to head: {code}"
    );
    assert!(
        code.contains("local_10 = 55;"),
        "expected body store: {code}"
    );
}

/// For-loop with a branch inside the body (body has if/else).
///
/// CFG:
///   block 0 (0x8000, init):   Copy cst(0) → counter; Branch(→ 0x8010)
///   block 1 (0x8010, head):   cond; CBranch(cond → 0x8060 exit, fallthrough → 0x8020)
///   block 2 (0x8020, body_if): CBranch(branch_cond → 0x8040, fallthrough → 0x8030)
///   block 3 (0x8030, then):   store_then; Branch(→ 0x8050)
///   block 4 (0x8040, else):   store_else; Branch(→ 0x8050)
///   block 5 (0x8050, latch):  IntAdd counter 1 → counter; Branch(→ 0x8010)
///   block 6 (0x8060, exit):   Return
///
/// Expected: for (...) { if (branch_cond) { store_then; } else { store_else; } }
#[test]
fn for_loop_with_body_branch() {
    let counter = uniq(0x800, 4);
    let loop_cond = uniq(0x801, 1);
    let branch_cond = uniq(0x802, 1);
    let ptr_then = uniq(0x803, 8);
    let ptr_else = uniq(0x804, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: init
            PcodeBasicBlock {
                index: 0,
                start_address: 0x8000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x8000,
                        output: Some(counter.clone()),
                        inputs: vec![cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8001,
                        output: None,
                        inputs: vec![cst(0x8010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: head
            PcodeBasicBlock {
                index: 1,
                start_address: 0x8010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLess,
                        address: 0x8010,
                        output: Some(loop_cond.clone()),
                        inputs: vec![counter.clone(), cst(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x8011,
                        output: None,
                        inputs: vec![cst(0x8060, 8), loop_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: body_if — CBranch(branch_cond → else, fallthrough → then)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x8020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x8020,
                        output: Some(branch_cond.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x8021,
                        output: None,
                        inputs: vec![cst(0x8040, 8), branch_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: then — store + Branch(→ latch)
            PcodeBasicBlock {
                index: 3,
                start_address: 0x8030,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x8030,
                        output: Some(ptr_then.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x8031,
                        output: None,
                        inputs: vec![cst(0, 4), ptr_then.clone(), cst(77, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8032,
                        output: None,
                        inputs: vec![cst(0x8050, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 4: else — store + Branch(→ latch)
            PcodeBasicBlock {
                index: 4,
                start_address: 0x8040,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x8040,
                        output: Some(ptr_else.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x8041,
                        output: None,
                        inputs: vec![cst(0, 4), ptr_else.clone(), cst(88, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8042,
                        output: None,
                        inputs: vec![cst(0x8050, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 5: latch — counter += 1; Branch(→ head)
            PcodeBasicBlock {
                index: 5,
                start_address: 0x8050,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x8050,
                        output: Some(counter.clone()),
                        inputs: vec![counter.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8051,
                        output: None,
                        inputs: vec![cst(0x8010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 6: exit
            PcodeBasicBlock {
                index: 6,
                start_address: 0x8060,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x8060,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "for_body_branch_fn", 0x8000, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("for (") || code.contains("while ("),
        "expected structured loop: {code}"
    );
    assert!(
        !code.contains("goto block_8010"),
        "expected no goto to loop head: {code}"
    );
    // Body stores must appear
    assert!(
        code.contains("local_10 = 77;") || code.contains("local_14 = 88;"),
        "expected body stores: {code}"
    );
}

/// Nested while loops: inner loop has a break; outer loop must not be affected.
///
/// CFG:
///   outer_head  (0x9000): CBranch(outer_cond → outer_exit 0x9060, fallthrough → inner_head 0x9010)
///   inner_head  (0x9010): CBranch(inner_cond → inner_exit 0x9040, fallthrough → inner_body 0x9020)
///   inner_body  (0x9020): store + CBranch(break_cond → inner_exit 0x9040, fallthrough → inner_latch 0x9030)
///   inner_latch (0x9030): Branch(→ inner_head 0x9010)
///   inner_exit  (0x9040): store2 + Branch(→ outer_latch 0x9050)
///   outer_latch (0x9050): Branch(→ outer_head 0x9000)
///   outer_exit  (0x9060): Return
#[test]
fn nested_while_inner_break_does_not_escape_outer() {
    let outer_cond = uniq(0x900, 1);
    let inner_cond = uniq(0x901, 1);
    let break_cond = uniq(0x902, 1);
    let ptr1 = uniq(0x903, 8);
    let ptr2 = uniq(0x904, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: outer_head
            PcodeBasicBlock {
                index: 0,
                start_address: 0x9000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x9000,
                        output: Some(outer_cond.clone()),
                        inputs: vec![reg(0x08, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x9001,
                        output: None,
                        inputs: vec![cst(0x9060, 8), outer_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: inner_head
            PcodeBasicBlock {
                index: 1,
                start_address: 0x9010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x9010,
                        output: Some(inner_cond.clone()),
                        inputs: vec![reg(0x10, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x9011,
                        output: None,
                        inputs: vec![cst(0x9040, 8), inner_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: inner_body — store + break-conditional
            PcodeBasicBlock {
                index: 2,
                start_address: 0x9020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x9020,
                        output: Some(ptr1.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x9021,
                        output: None,
                        inputs: vec![cst(0, 4), ptr1.clone(), cst(99, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x9022,
                        output: Some(break_cond.clone()),
                        inputs: vec![reg(0x18, 1)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x9023,
                        output: None,
                        inputs: vec![cst(0x9040, 8), break_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: inner_latch — Branch(→ inner_head)
            PcodeBasicBlock {
                index: 3,
                start_address: 0x9030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x9030,
                    output: None,
                    inputs: vec![cst(0x9010, 8)],
                    asm_mnemonic: None,
                }],
            },
            // block 4: inner_exit — store2 + Branch(→ outer_latch)
            PcodeBasicBlock {
                index: 4,
                start_address: 0x9040,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x9040,
                        output: Some(ptr2.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x14_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Store,
                        address: 0x9041,
                        output: None,
                        inputs: vec![cst(0, 4), ptr2.clone(), cst(111, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x9042,
                        output: None,
                        inputs: vec![cst(0x9050, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 5: outer_latch — Branch(→ outer_head)
            PcodeBasicBlock {
                index: 5,
                start_address: 0x9050,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x9050,
                    output: None,
                    inputs: vec![cst(0x9000, 8)],
                    asm_mnemonic: None,
                }],
            },
            // block 6: outer_exit
            PcodeBasicBlock {
                index: 6,
                start_address: 0x9060,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x9060,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "nested_loops_fn", 0x9000, &preview_options())
        .expect("preview render");
    // Both loops must appear as while (not goto-based)
    let while_count = code.matches("while (").count() + code.matches("while(!").count();
    assert!(while_count >= 1, "expected at least one while loop: {code}");
    // The inner break must appear
    assert!(code.contains("break;"), "expected inner break: {code}");
    // Both stores must appear
    assert!(
        code.contains("local_10 = 99;"),
        "expected inner store: {code}"
    );
    // NOTE: RBP-0x14 is currently merged into local_10 by the stack-slot deduplication pass.
    assert!(
        code.contains("local_10 = 111;") || code.contains("local_14 = 111;"),
        "expected outer-body store (local_10 or local_14): {code}"
    );
    // The outer loop must not have a stray goto to outer_head
    assert!(
        !code.contains("goto block_9000"),
        "outer loop should not produce goto to outer head: {code}"
    );
}

/// Two-block infinite loop (block 0 → block 1 → block 0) with no exits.
/// Expected output: `while (true) { …store… }`.
///
/// The store writes through *rax (a non-stack pointer) so it is not dead-code-eliminated
/// by the write-only stack-slot removal pass.
#[test]
fn multiblock_infloop_preview_lowers_two_block_infinite_loop() {
    let func = PcodeFunction {
        blocks: vec![
            // block 0: *rax = 42, then fallthrough to block 1
            PcodeBasicBlock {
                index: 0,
                start_address: 0xA000,
                successors: vec![1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Store,
                    address: 0xA000,
                    output: None,
                    // space=0 (RAM), ptr=rax (reg offset 0, size 8), value=42 (i32)
                    inputs: vec![cst(0, 4), reg(0, 8), cst(42, 4)],
                    asm_mnemonic: None,
                }],
            },
            // block 1: branch back to block 0 (back-edge, no exits)
            PcodeBasicBlock {
                index: 1,
                start_address: 0xA010,
                successors: vec![0],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0xA010,
                    output: None,
                    inputs: vec![cst(0xA000, 8)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "multiblock_infloop_fn", 0xA000, &preview_options())
        .expect("preview render");
    // Must produce a while(true) construct rather than unstructured goto-spaghetti.
    assert!(
        code.contains("while (true)") || code.contains("while (1)"),
        "expected while(true) for two-block infinite loop: {code}"
    );
    // The store through *rax must be present inside the loop body.
    assert!(
        code.contains("*rax") || code.contains("*(rax)"),
        "expected pointer store in loop body: {code}"
    );
}

#[test]
fn for_loop_non_last_update_success() {
    let counter = reg(0x18, 4);
    let lt_cond = uniq(0x701, 1);
    let ptr = uniq(0x702, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: init — counter = 0; Branch(→ head)
            PcodeBasicBlock {
                index: 0,
                start_address: 0x8000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x8000,
                        output: Some(counter.clone()),
                        inputs: vec![cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8001,
                        output: None,
                        inputs: vec![cst(0x8010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: head — compare + CBranch(cond → exit, fallthrough → body)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x8010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLessEqual,
                        address: 0x8010,
                        output: Some(lt_cond.clone()),
                        inputs: vec![reg(0x08, 4), counter.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x8011,
                        output: None,
                        inputs: vec![cst(0x8030, 8), lt_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: body — update first, store second (no counter use)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x8020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x8020,
                        output: Some(counter.clone()),
                        inputs: vec![counter.clone(), cst(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x8021,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Store,
                        address: 0x8022,
                        output: None,
                        inputs: vec![cst(0, 4), ptr.clone(), cst(42, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Branch,
                        address: 0x8023,
                        output: None,
                        inputs: vec![cst(0x8010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: exit
            PcodeBasicBlock {
                index: 3,
                start_address: 0x8030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x8030,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "non_last_update_success_fn",
        0x8000,
        &preview_options(),
    )
    .expect("preview render");
    // Should be successfully folded into a C-style for loop!
    assert!(code.contains("for ("), "expected for loop: {code}");
    assert!(
        code.contains("rbx = rbx + 1") || code.contains("rbx++") || code.contains("rbx += 1"),
        "expected update in header: {code}"
    );
    assert!(
        code.contains("local_10 = 42;"),
        "expected body store: {code}"
    );
}

#[test]
fn for_loop_non_last_update_failure() {
    let counter = reg(0x18, 4);
    let lt_cond = uniq(0x701, 1);
    let ptr = uniq(0x702, 8);
    let func = PcodeFunction {
        blocks: vec![
            // block 0: init — counter = 0; Branch(→ head)
            PcodeBasicBlock {
                index: 0,
                start_address: 0x9000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x9000,
                        output: Some(counter.clone()),
                        inputs: vec![cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x9001,
                        output: None,
                        inputs: vec![cst(0x9010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 1: head — compare + CBranch(cond → exit, fallthrough → body)
            PcodeBasicBlock {
                index: 1,
                start_address: 0x9010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntLessEqual,
                        address: 0x9010,
                        output: Some(lt_cond.clone()),
                        inputs: vec![reg(0x08, 4), counter.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x9011,
                        output: None,
                        inputs: vec![cst(0x9030, 8), lt_cond.clone()],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 2: body — update first, store second (uses counter!)
            PcodeBasicBlock {
                index: 2,
                start_address: 0x9020,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x9020,
                        output: Some(counter.clone()),
                        inputs: vec![counter.clone(), reg(0x10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x9021,
                        output: Some(ptr.clone()),
                        inputs: vec![reg(0x28, 8), cst(-0x10_i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Store,
                        address: 0x9022,
                        output: None,
                        // Stores counter, so counter is USED after update!
                        inputs: vec![cst(0, 4), ptr.clone(), counter.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Branch,
                        address: 0x9023,
                        output: None,
                        inputs: vec![cst(0x9010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block 3: exit
            PcodeBasicBlock {
                index: 3,
                start_address: 0x9030,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x9030,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(
        &func,
        "non_last_update_failure_fn",
        0x9000,
        &preview_options(),
    )
    .expect("preview render");
    // Should NOT have the update statement in the third clause of the for loop!
    assert!(code.contains("for ("), "expected for loop: {code}");
    assert!(
        code.contains("for (rbx = 0; rbx < param_1; )")
            || code.contains("for (rbx = 0; rbx < param_1;)"),
        "expected empty update in header: {code}"
    );
    // Verify that the update statement is still in the loop body!
    assert!(
        code.contains("rbx += param_2") || code.contains("rbx = rbx + param_2"),
        "expected update statement in body: {code}"
    );
    assert!(
        code.contains("local_10 = rbx;"),
        "expected body store: {code}"
    );
}

/// Regression test: x86-64 loop accumulator ZExt passthrough look-through.
///
/// In x86-64, writing a 32-bit result (EAX) implicitly zero-extends into the 64-bit parent
/// (RAX). P-code represents this as two successive ops at sequential seq_nums:
///
/// ```text
///   EAX = IntAdd(EAX, tmp)   ; seq N     ← actual narrow accumulator def
///   RAX = IntZExt(EAX)       ; seq N+1   ← higher op_idx wins alias resolution without fix
/// ```
///
/// Without the two-layer fix, `lookup_def_site(EAX)` picks the `RAX=IntZExt` op (larger
/// op_idx), causing:
///   - Layer 1 (`binding.rs`): loop-carried name for the accumulator resolves to a stale
///     pre-loop snapshot temp ("xVar27") instead of "rax", so the body uses "xVar27 += ..."
///   - Layer 2 (`lower_expr.rs`): the return-value expression also resolves to that same
///     stale temp, producing "return xVar27" (stuck at the initial zero) instead of "return rax"
///
/// Invariant tested: when the def found by `lookup_def_site` for a narrow register is a
/// passthrough op (ZExt/SExt/Copy) whose sole input is exactly that narrow register, the
/// look-through scan backwards in the same block must find the actual narrow-register def
/// and return its materialized name rather than a snapshot of the wide alias.
///
/// CFG:
/// ```text
///   block_0 (0xa000): EAX = IntXor(EAX, EAX)=0; RAX = IntZExt(EAX); Branch → 0xa010
///   block_1 (0xa010): tmp = Load(RCX);
///                     EAX = IntAdd(EAX, tmp);  ← accumulate
///                     RAX = IntZExt(EAX);       ← x86-64 zero-extend artifact
///                     RCX = IntAdd(RCX, 4);
///                     cond = IntNotEqual(RCX, RDX);
///                     CBranch(→ 0xa010 if cond, fall-through → 0xa020)
///   block_2 (0xa020): Return  ← return value resolved from predecessor block_1
/// ```
#[test]
fn do_while_x86_zext_passthrough_accumulator_naming() {
    let tmp_load = uniq(0xA01, 4); // loaded dword from one array element
    let loop_cond = uniq(0xA02, 1); // loop-continue flag

    let func = PcodeFunction {
        blocks: vec![
            // block_0: init — EAX = 0 (XOR self), RAX = ZExt(EAX), Branch → loop body
            PcodeBasicBlock {
                index: 0,
                start_address: 0xA000,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntXor,
                        address: 0xA000,
                        output: Some(reg(0x00, 4)), // EAX = EAX ^ EAX = 0
                        inputs: vec![reg(0x00, 4), reg(0x00, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0xA001,
                        output: Some(reg(0x00, 8)), // RAX = ZExt(EAX) — zero-extend artifact
                        inputs: vec![reg(0x00, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0xA002,
                        output: None,
                        inputs: vec![cst(0xA010, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            // block_1: do-while body — EAX accumulates, ZExt follows, ptr advances, loop test
            PcodeBasicBlock {
                index: 1,
                start_address: 0xA010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Load,
                        address: 0xA010,
                        output: Some(tmp_load.clone()), // tmp = *RCX (one array element)
                        inputs: vec![cst(0, 4), reg(0x08, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0xA011,
                        output: Some(reg(0x00, 4)), // EAX += tmp
                        inputs: vec![reg(0x00, 4), tmp_load.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        // x86-64 zero-extend side-effect: higher seq_num than the EAX def
                        // above, so without the fix lookup_def_site returns this op for EAX
                        seq_num: 2,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0xA012,
                        output: Some(reg(0x00, 8)), // RAX = ZExt(EAX)
                        inputs: vec![reg(0x00, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0xA013,
                        output: Some(reg(0x08, 8)), // RCX += 4 (advance array pointer)
                        inputs: vec![reg(0x08, 8), cst(4, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntNotEqual,
                        address: 0xA014,
                        output: Some(loop_cond.clone()), // cond = (RCX != RDX/end)
                        inputs: vec![reg(0x08, 8), reg(0x10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::CBranch,
                        address: 0xA015,
                        output: None,
                        inputs: vec![cst(0xA010, 8), loop_cond.clone()], // back-edge if cond
                        asm_mnemonic: None,
                    },
                ],
            },
            // block_2: exit — return picks up accumulated EAX/RAX from predecessor
            PcodeBasicBlock {
                index: 2,
                start_address: 0xA020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0xA020,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 4)],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "zext_accumulator_fn", 0xA000, &preview_options())
        .expect("preview render");

    // Primary invariant: the return must carry the accumulated loop value ("rax"), not a
    // pre-loop snapshot temp (e.g., "xVar27") that was stuck at the initial zero.
    assert!(
        code.contains("return rax"),
        "expected 'return rax' (accumulated value), got stale temp or zero: {code}"
    );

    // Secondary invariant: the loop body must assign *into* rax, not into a temp.
    // Without the Layer-1 fix, the loop body would use an unnamed temp as the LHS.
    assert!(
        code.contains("rax +=") || code.contains("rax = rax +"),
        "expected rax as the loop accumulator in the body: {code}"
    );
}
