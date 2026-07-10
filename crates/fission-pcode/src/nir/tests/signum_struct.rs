use super::*;

/// x86-32 O0 signum shape:
///   if (v > 0) return 1;
///   if (v < 0) return -1;
///   return 0;
#[test]
fn preview_signum_diamond_returns_one_for_positive() {
    let eax = reg(0x0, 4);
    let zf = reg(0x206, 1);
    let sf = reg(0x207, 1);
    // param on stack: use register param simulation via entry read of "stack" as
    // constant path - use ECX as pseudo param for simpler 32-bit-like flow.
    // Better: load-like: copy param into unique then compare.
    // Use eax as the comparison value register (set from constant 0 test).
    // For structuring we need multi-block CFG:
    // b0: cbranch LE -> b2, fall b1
    // b1: eax=1; goto b5
    // b2: cbranch SF-clear (not neg / jns) -> b4, fall b3
    // b3: eax=-1; goto b5
    // b4: eax=0
    // b5: return eax

    // Condition LE as ZF | SF for simplicity (IntSLess or Eq).
    let le_flag = uniq(0x100, 1);
    let not_sf = uniq(0x101, 1);

    let blocks = vec![
        // block 0
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![1, 2],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x1000,
                    output: Some(sf.clone()),
                    inputs: vec![reg(0x0, 4), cst(0, 4)], // eax < 0 — wrong for LE entry
                    asm_mnemonic: None,
                },
                // Use a simpler approach: CBranch on a unique set by IntSLess of param
                // Actually for test just use constant-free: compare eax (param via reg)
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntEqual,
                    address: 0x1000,
                    output: Some(zf.clone()),
                    inputs: vec![reg(0x0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::BoolOr,
                    address: 0x1000,
                    output: Some(le_flag.clone()),
                    inputs: vec![zf.clone(), sf.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x1000,
                    output: None,
                    inputs: vec![
                        Varnode {
                            space_id: 3,
                            offset: 0x1020,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        le_flag.clone(),
                    ],
                    asm_mnemonic: None,
                },
            ],
        },
        // block 1: eax = 1; goto join
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![5],
            ops: vec![
                PcodeOp {
                    seq_num: 10,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1010,
                    output: Some(eax.clone()),
                    inputs: vec![cst(1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 11,
                    opcode: PcodeOpcode::Branch,
                    address: 0x1010,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: 3,
                        offset: 0x1050,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                },
            ],
        },
        // block 2: if not negative (jns) -> zero arm
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: vec![3, 4],
            ops: vec![
                PcodeOp {
                    seq_num: 20,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x1020,
                    output: Some(sf.clone()),
                    inputs: vec![reg(0x0, 4), cst(0, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 21,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x1020,
                    output: Some(not_sf.clone()),
                    inputs: vec![sf.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 22,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x1020,
                    output: None,
                    inputs: vec![
                        Varnode {
                            space_id: 3,
                            offset: 0x1040,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        not_sf,
                    ],
                    asm_mnemonic: None,
                },
            ],
        },
        // block 3: eax = -1; goto join
        PcodeBasicBlock {
            index: 3,
            start_address: 0x1030,
            successors: vec![5],
            ops: vec![
                PcodeOp {
                    seq_num: 30,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1030,
                    output: Some(eax.clone()),
                    inputs: vec![cst(-1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 31,
                    opcode: PcodeOpcode::Branch,
                    address: 0x1030,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: 3,
                        offset: 0x1050,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                },
            ],
        },
        // block 4: eax = 0
        PcodeBasicBlock {
            index: 4,
            start_address: 0x1040,
            successors: vec![5],
            ops: vec![PcodeOp {
                seq_num: 40,
                opcode: PcodeOpcode::Copy,
                address: 0x1040,
                output: Some(eax.clone()),
                inputs: vec![cst(0, 4)],
                asm_mnemonic: None,
            }],
        },
        // block 5: return eax
        PcodeBasicBlock {
            index: 5,
            start_address: 0x1050,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 50,
                opcode: PcodeOpcode::Return,
                address: 0x1050,
                output: None,
                inputs: vec![eax],
                asm_mnemonic: None,
            }],
        },
    ];
    // Set successors already set
    let func = PcodeFunction { blocks };
    let mut options = preview_options_x86();
    options.pe_x64_only = false;
    options.is_64bit = false;
    options.pointer_size = 4;
    let code = render_mlil_preview(&func, "signum_like", 0x1000, &options).expect("render");
    eprintln!("signum_like:\n{code}");
    assert!(
        code.contains("return 1") || code.contains("return 1;"),
        "positive arm must return 1, got:\n{code}"
    );
    assert!(
        code.contains("return -1") || code.contains("0xffffffff") || code.contains("-1"),
        "negative arm must return -1, got:\n{code}"
    );
    assert!(
        code.contains("return 0"),
        "zero arm must return 0, got:\n{code}"
    );
}
