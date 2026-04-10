use super::*;
use crate::arch::x86::{X86_REG_BASE, X86_XMM_BASE};

/// Build a UNIQUE-space varnode at the given architectural offset (stride already applied by caller).
fn arch_reg(offset: u64, size: u32) -> Varnode {
    uniq(offset, size)
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Return the offset for GPR at index `i` (0=rax … 4=rsp … 7=rdi).
fn gpr_offset(i: u64) -> u64 {
    X86_REG_BASE + i * 8
}

/// Return the offset for XMM register at index `i`.
fn xmm_offset(i: u64) -> u64 {
    X86_XMM_BASE + i * 16
}

// ── single-block tests ────────────────────────────────────────────────────────

#[test]
fn unique_rsp_is_named_rsp_in_output() {
    // Block: rsp = 0;  return rsp;
    let rsp = arch_reg(gpr_offset(4), 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140001000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140001000,
                    output: Some(rsp.clone()),
                    inputs: vec![cst(0, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140001001,
                    output: None,
                    inputs: vec![cst(0, 8), rsp],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "test_rsp", 0x140001000, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("rsp"),
        "expected 'rsp' in output, got:\n{code}"
    );
    assert!(
        !code.contains("tmp_a860"),
        "should not see tmp_a860xxxx; got:\n{code}"
    );
}

#[test]
fn unique_rax_is_named_rax_in_output() {
    let rax = arch_reg(gpr_offset(0), 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140002000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140002000,
                    output: Some(rax.clone()),
                    inputs: vec![cst(42, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140002001,
                    output: None,
                    inputs: vec![cst(0, 8), rax],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "test_rax", 0x140002000, &preview_options())
        .expect("preview render");
    assert!(code.contains("rax"), "expected 'rax'; got:\n{code}");
    assert!(!code.contains("tmp_a860"), "should not see tmp_a860xxxx; got:\n{code}");
}

#[test]
fn unique_rsp_based_store_surfaces_stack_slot() {
    let rsp = arch_reg(gpr_offset(4), 8);
    let ptr = uniq(0x120, 8);
    let val = uniq(0x128, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140002100,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x140002100,
                    output: Some(ptr.clone()),
                    inputs: vec![rsp, cst(0x70, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x140002101,
                    output: None,
                    inputs: vec![cst(0, 8), ptr.clone(), cst(7, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Load,
                    address: 0x140002102,
                    output: Some(val.clone()),
                    inputs: vec![cst(0, 8), ptr],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x140002103,
                    output: None,
                    inputs: vec![cst(0, 8), val],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "test_stack_slot", 0x140002100, &preview_options())
        .expect("preview render");
    assert!(
        code.contains("home_70"),
        "expected unique-rsp stack slot surfacing, got:\n{code}"
    );
}

// ── cross-block test ──────────────────────────────────────────────────────────

/// Block 0 writes rsp; block 1 reads rsp — the cross-block read must not appear as tmp_a8600020.
#[test]
fn unique_rsp_cross_block_is_named() {
    let rsp = arch_reg(gpr_offset(4), 8);

    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140003000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x140003000,
                        output: Some(rsp.clone()),
                        inputs: vec![cst(0x7fff_0000u64 as i64, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x140003008,
                        output: None,
                        inputs: vec![Varnode {
                            space_id: 1,
                            offset: 0x140003010,
                            size: 8,
                            is_constant: false,
                            constant_val: 0,
                        }],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140003010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Return,
                    address: 0x140003010,
                    output: None,
                    inputs: vec![cst(0, 8), rsp],
                    asm_mnemonic: None,
                }],
            },
        ],
    };

    let code = render_mlil_preview(&func, "test_rsp_cross", 0x140003000, &preview_options())
        .expect("preview render");
    assert!(code.contains("rsp"), "expected 'rsp' in cross-block output; got:\n{code}");
    assert!(
        !code.contains("tmp_a860"),
        "cross-block rsp must not appear as tmp_a860xxxx; got:\n{code}"
    );
}

// ── XMM test ─────────────────────────────────────────────────────────────────

#[test]
fn unique_xmm0_is_named_xmm0() {
    // xmm0 = 0; return 0; (the assignment should appear as xmm0 = ...)
    let xmm0 = arch_reg(xmm_offset(0), 16);
    let dummy_mem = Varnode {
        space_id: 0, // RAM space
        offset: 0x140004100,
        size: 16,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140004000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Store,
                    address: 0x140004000,
                    output: None,
                    inputs: vec![cst(1, 8), dummy_mem, xmm0],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140004002,
                    output: None,
                    inputs: vec![cst(0, 8), cst(0, 8)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "test_xmm0", 0x140004000, &preview_options())
        .expect("preview render");
    assert!(code.contains("xmm0"), "expected 'xmm0'; got:\n{code}");
    assert!(!code.contains("tmp_a868"), "should not see tmp_a868xxxx; got:\n{code}");
}
