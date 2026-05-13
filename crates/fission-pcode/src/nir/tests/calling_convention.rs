/// Tests for ABI-aware register parameter naming.
///
/// `register_name_with_param` maps Ghidra REGISTER-space offsets to either
/// `("param_N", Some(N-1))` for parameter registers or `(hw_name, None)` for others.
/// The distinction depends on the active `CallingConvention`.
use super::*;
use crate::nir::AbiState;

// ── Windows x64 ────────────────────────────────────────────────────────────────

#[test]
fn win64_rcx_is_param_1() {
    let (name, idx) = register_name_with_param(0x08, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn win64_rdx_is_param_2() {
    let (name, idx) = register_name_with_param(0x10, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn win64_r8_is_param_3() {
    let (name, idx) = register_name_with_param(0x80, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn win64_r9_is_param_4() {
    let (name, idx) = register_name_with_param(0x88, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn win64_subregister_aliases_map_to_param_slots() {
    let abi = AbiState::new(CallingConvention::WindowsX64, true, 8, 0);
    assert_eq!(abi.param_slot_for_name("ecx"), Some(0));
    assert_eq!(abi.param_slot_for_name("cx"), Some(0));
    assert_eq!(abi.param_slot_for_name("cl"), Some(0));
    assert_eq!(abi.param_slot_for_name("r8d"), Some(2));
    assert_eq!(abi.param_slot_for_name("r9b"), Some(3));
}

#[test]
fn win64_rdi_is_not_a_param() {
    let (name, idx) = register_name_with_param(0x38, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rdi");
    assert_eq!(idx, None);
}

#[test]
fn win64_rsi_is_not_a_param() {
    let (name, idx) = register_name_with_param(0x30, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rsi");
    assert_eq!(idx, None);
}

// ── System V AMD64 ─────────────────────────────────────────────────────────────

#[test]
fn sysv_rdi_is_param_1() {
    let (name, idx) = register_name_with_param(0x38, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn sysv_rsi_is_param_2() {
    let (name, idx) = register_name_with_param(0x30, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn sysv_rdx_is_param_3() {
    let (name, idx) = register_name_with_param(0x10, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn sysv_rcx_is_param_4() {
    let (name, idx) = register_name_with_param(0x08, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn sysv_r8_is_param_5() {
    let (name, idx) = register_name_with_param(0x80, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_5");
    assert_eq!(idx, Some(4));
}

#[test]
fn sysv_r9_is_param_6() {
    let (name, idx) = register_name_with_param(0x88, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_6");
    assert_eq!(idx, Some(5));
}

#[test]
fn sysv_subregister_aliases_map_to_param_slots() {
    let abi = AbiState::new(CallingConvention::SystemVAmd64, true, 8, 0);
    assert_eq!(abi.param_slot_for_name("edi"), Some(0));
    assert_eq!(abi.param_slot_for_name("si"), Some(1));
    assert_eq!(abi.param_slot_for_name("edx"), Some(2));
    assert_eq!(abi.param_slot_for_name("ecx"), Some(3));
    assert_eq!(abi.param_slot_for_name("r8w"), Some(4));
    assert_eq!(abi.param_slot_for_name("r9d"), Some(5));
}

// ── AArch64 PCS ───────────────────────────────────────────────────────────────

#[test]
fn aarch64_x0_to_x7_are_params() {
    for slot in 0..8usize {
        let offset = 0x4000 + (slot as u64 * 8);
        let (name, idx) = register_name_with_param(offset, 8, CallingConvention::AArch64).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn aarch64_compact_x0_offset_is_param() {
    let (name, idx) = register_name_with_param(0x00, 8, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));

    let (name, idx) = register_name_with_param(0x00, 4, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn aarch64_big_endian_w_register_halves_are_params() {
    let (name, idx) = register_name_with_param(0x4004, 4, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));

    let (name, idx) = register_name_with_param(0x400c, 4, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));

    let ret = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x4004,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_register_for_abi(
        &ret,
        CallingConvention::AArch64
    ));
}

#[test]
fn aarch64_subregister_aliases_map_to_param_slots() {
    let abi = AbiState::new(CallingConvention::AArch64, true, 8, 0);
    assert_eq!(abi.param_slot_for_name("x0"), Some(0));
    assert_eq!(abi.param_slot_for_name("w0"), Some(0));
    assert_eq!(abi.param_slot_for_name("x7"), Some(7));
    assert_eq!(abi.param_slot_for_name("w7"), Some(7));
    assert_eq!(abi.param_slot_for_name("x8"), None);
}

#[test]
fn aarch64_return_register_is_named_and_recognized() {
    assert_eq!(register_name(0x4000, 8), "x0");
    assert_eq!(register_name(0x4000, 4), "w0");
    let x0 = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_register_for_abi(
        &x0,
        CallingConvention::AArch64
    ));
    assert!(!is_primary_return_register(&x0));
}

#[test]
fn aarch64_be_unique_subrange_projection_uses_low_value_view() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    options.is_big_endian = true;

    let param = reg(0x4004, 4);
    let mixed = uniq(0x100, 8);
    let shifted = uniq(0x110, 8);
    let shifted_low_be = Varnode {
        offset: 0x114,
        size: 4,
        ..shifted.clone()
    };
    let x9 = reg(0x4048, 8);
    let w0_be = reg(0x4004, 4);
    let ret_target = reg(0, 8);
    let x30 = reg(0x40f0, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Piece,
                    address: 0x1000,
                    output: Some(mixed.clone()),
                    inputs: vec![param.clone(), param],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntRight,
                    address: 0x1004,
                    output: Some(shifted.clone()),
                    inputs: vec![mixed, cst(27, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x1008,
                    output: Some(x9.clone()),
                    inputs: vec![shifted_low_be],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100c,
                    output: Some(w0_be),
                    inputs: vec![x9],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1010,
                    output: Some(ret_target),
                    inputs: vec![x30.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Return,
                    address: 0x1010,
                    output: None,
                    inputs: vec![x30],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "be_unique_low_view", 0x1000, &options).expect("preview render");
    assert!(code.contains("/ 134217728"), "{code}");
    assert!(!code.contains(">> 32"), "{code}");
}

// ── ARM32 PCS ─────────────────────────────────────────────────────────────────

#[test]
fn arm32_r0_to_r3_are_params() {
    for slot in 0..4usize {
        let offset = 0x20 + (slot as u64 * 4);
        let (name, idx) = register_name_with_param(offset, 4, CallingConvention::Arm32).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn arm32_non_param_registers_are_named() {
    let (name, idx) = register_name_with_param(0x30, 4, CallingConvention::Arm32).unwrap();
    assert_eq!(name, "r4");
    assert_eq!(idx, None);
    assert_eq!(register_name(0x54, 4), "sp");
    assert_eq!(register_name(0x58, 4), "lr");
    assert_eq!(register_name(0x5c, 4), "pc");
}

#[test]
fn arm32_param_slots_work_for_32bit_abi_state() {
    let abi = AbiState::new(CallingConvention::Arm32, false, 4, 0);
    assert_eq!(abi.param_slot_for_name("r0"), Some(0));
    assert_eq!(abi.param_slot_for_name("r3"), Some(3));
    assert_eq!(abi.param_slot_for_name("r4"), None);
}

#[test]
fn arm32_return_register_is_named_and_recognized() {
    let (name, idx) = register_name_with_param(0x20, 4, CallingConvention::Arm32).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
    let r0 = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_register_for_abi(
        &r0,
        CallingConvention::Arm32
    ));
    assert!(!is_primary_return_register(&r0));
}

#[test]
fn arm32_bx_lr_returns_primary_r0_not_link_target() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = uniq(0x2000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100038,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x100038,
                    output: Some(r0.clone()),
                    inputs: vec![r1, r0],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x10003c,
                    output: Some(ret_target.clone()),
                    inputs: vec![lr, cst(0xffff_fffe, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Call,
                    address: 0x10003c,
                    output: None,
                    inputs: vec![ret_target.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x10003c,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "op_add", 0x100038, &options).expect("preview render");
    assert!(
        code.contains("uint op_add(uint param_1, uint param_2)"),
        "{code}"
    );
    assert!(code.contains("return param_2 + param_1;"), "{code}");
    assert!(!code.contains("sub_"), "{code}");
    assert!(!code.contains("return lr"), "{code}");
}

#[test]
fn arm32_direct_call_recovers_r0_argument() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x100014,
                    output: Some(r0.clone()),
                    inputs: vec![r0.clone(), cst(1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Call,
                    address: 0x100018,
                    output: None,
                    inputs: vec![cst(0x100000, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x10001c,
                    output: None,
                    inputs: vec![cst(0, 4), r0],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "recursive_fib", 0x100000, &options).expect("preview render");
    assert!(code.contains("sub_100000(param_1 - 1)"), "{code}");
}

#[test]
fn arm32_direct_call_materializes_r0_result() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x100018,
                    output: None,
                    inputs: vec![cst(0x100100, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x10001c,
                    output: Some(r0.clone()),
                    inputs: vec![r0.clone(), r1],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x100020,
                    output: None,
                    inputs: vec![cst(0, 4), r0],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "caller", 0x100000, &options).expect("preview render");
    assert!(code.contains("return sub_100100() + param_2;"), "{code}");
}

#[test]
fn arm32_r1_r0_pair_materializes_u64_return() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = uniq(0x2000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100040,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x100040,
                    output: Some(r0.clone()),
                    inputs: vec![r0.clone(), cst(1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x100044,
                    output: Some(r1.clone()),
                    inputs: vec![r1, cst(2, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x100048,
                    output: Some(ret_target.clone()),
                    inputs: vec![lr, cst(0xffff_fffe, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x100048,
                    output: None,
                    inputs: vec![ret_target.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x100048,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "u64_pair", 0x100040, &options).expect("preview render");
    assert!(
        code.contains("ulonglong u64_pair(uint param_1, uint param_2)"),
        "{code}"
    );
    assert!(
        code.contains("return (ulonglong)(param_2 + 2) << 32 | (ulonglong)(param_1 + 1);"),
        "{code}"
    );
}

#[test]
fn arm32_address_in_r1_does_not_force_u64_return() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;
    options
        .relocation_names
        .insert(0x100044, "math_sink".to_string());

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = uniq(0x2000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100040,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x100040,
                    output: Some(r0.clone()),
                    inputs: vec![r0.clone(), cst(1, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Load,
                    address: 0x100044,
                    output: Some(r1),
                    inputs: vec![cst(0, 4), cst(0x2000, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x100048,
                    output: Some(ret_target.clone()),
                    inputs: vec![lr, cst(0xffff_fffe, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x100048,
                    output: None,
                    inputs: vec![ret_target.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x100048,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "math_like", 0x100040, &options).expect("preview render");
    assert!(code.contains("uint math_like(uint param_1)"), "{code}");
    assert!(code.contains("return param_1 + 1;"), "{code}");
    assert!(!code.contains("ulonglong math_like"), "{code}");
    assert!(
        !code.contains("return (ulonglong)&math_sink << 32"),
        "{code}"
    );
}

#[test]
fn arm32_branchind_tail_call_recovers_function_pointer_call() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x24,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r2 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x28,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r3 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x2c,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let target = uniq(0x3000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x10005c,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x10005c,
                    output: Some(r3.clone()),
                    inputs: vec![r0.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100060,
                    output: Some(r0.clone()),
                    inputs: vec![r1.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100064,
                    output: Some(r1),
                    inputs: vec![r2],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x100068,
                    output: None,
                    inputs: vec![cst(0x3e, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x100068,
                    output: Some(target.clone()),
                    inputs: vec![r3, cst(0xffff_fffe, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x100068,
                    output: None,
                    inputs: vec![target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "apply_op", 0x10005c, &options).expect("preview render");
    assert!(
        code.contains("return ((code *)param_1)(param_2, param_3);"),
        "{code}"
    );
    assert!(!code.contains("sub_3e"), "{code}");
    assert!(!code.contains("__fission_branchind"), "{code}");
}

#[test]
fn arm32_link_register_target_without_r0_def_is_void_return() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::Arm32;
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x58,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = uniq(0x2000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x10003c,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAnd,
                    address: 0x10003c,
                    output: Some(ret_target.clone()),
                    inputs: vec![lr, cst(0xffff_fffe, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Call,
                    address: 0x10003c,
                    output: None,
                    inputs: vec![ret_target.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x10003c,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "leaf_void", 0x10003c, &options).expect("preview render");
    assert!(code.contains("void leaf_void(void)"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("sub_"), "{code}");
    assert!(!code.contains("return lr"), "{code}");
}

#[test]
fn aarch64_return_link_register_input_is_control_target_not_value() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let x30 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100000,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Return,
                address: 0x100000,
                output: None,
                inputs: vec![cst(0, 8), x30],
                asm_mnemonic: None,
            }],
        }],
    };

    let code = render_mlil_preview(&func, "leaf_void", 0x100000, &options).expect("preview render");
    assert!(code.contains("void leaf_void(void)"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("return x30;"), "{code}");
}

#[test]
fn aarch64_return_target_copy_input_is_control_target_not_value() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let x30 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100000,
                    output: Some(ret_target.clone()),
                    inputs: vec![x30],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x100000,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "leaf_void", 0x100000, &options).expect("preview render");
    assert!(code.contains("void leaf_void(void)"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("return x30;"), "{code}");
}

#[test]
fn aarch64_ret_link_register_copy_is_not_return_value() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let x0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let w0 = Varnode {
        size: 4,
        ..x0.clone()
    };
    let w1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4008,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let x30 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let tmp = uniq(0x1000, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x10004c,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x10004c,
                    output: Some(tmp.clone()),
                    inputs: vec![w1, w0],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x10004c,
                    output: Some(x0),
                    inputs: vec![tmp],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100050,
                    output: Some(ret_target),
                    inputs: vec![x30.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Return,
                    address: 0x100050,
                    output: None,
                    inputs: vec![cst(0, 8), x30],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "op_add", 0x10004c, &options).expect("preview render");
    assert!(code.contains("return param_2 + param_1;"), "{code}");
    assert!(!code.contains("return x30;"), "{code}");
}

#[test]
fn aarch64_return_only_join_inlines_predecessor_return_values() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let x0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let w0 = Varnode {
        size: 4,
        ..x0.clone()
    };
    let x30 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let cond = uniq(0x2000, 1);
    let add_tmp = uniq(0x2100, 4);
    let xor_tmp = uniq(0x2200, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2, 1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x1000,
                    output: None,
                    inputs: vec![cst(0x1020, 8), cond],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![3],
                ops: vec![
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1010,
                        output: Some(add_tmp.clone()),
                        inputs: vec![w0.clone(), cst(10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1010,
                        output: Some(x0.clone()),
                        inputs: vec![add_tmp],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1014,
                        output: None,
                        inputs: vec![cst(0x1030, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: vec![3],
                ops: vec![
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1020,
                        output: Some(xor_tmp.clone()),
                        inputs: vec![w0, cst(0xaaaa, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1020,
                        output: Some(x0),
                        inputs: vec![xor_tmp],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x1030,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1030,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::Return,
                        address: 0x1030,
                        output: None,
                        inputs: vec![cst(0, 8), x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "return_join", 0x1000, &options).expect("preview render");
    assert!(code.contains("param_1 + 10"), "{code}");
    assert!(code.contains("param_1 ^ 43690"), "{code}");
    assert!(code.contains("return xVar"), "{code}");
    assert!(!code.contains("block_1030:"), "{code}");
    assert!(!code.contains("goto block_1030"), "{code}");
}

fn aarch64_preview_options() -> MlilPreviewOptions {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    options
}

fn aarch64_reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

#[test]
fn callother_uses_userop_intrinsic_target_not_sub_fallback() {
    let options = aarch64_preview_options();
    let w0 = aarch64_reg(0x4000, 4);
    let w1 = aarch64_reg(0x4008, 4);
    let out = uniq(0x500, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100068,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::CallOther,
                    address: 0x100068,
                    output: Some(out.clone()),
                    inputs: vec![cst(126, 4), w0, w1],
                    asm_mnemonic: Some("synthetic userop".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x10006c,
                    output: None,
                    inputs: vec![out],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code =
        render_mlil_preview(&func, "userop_expr", 0x100068, &options).expect("preview render");
    assert!(code.contains("__pcodeop_126(param_1, param_2)"), "{code}");
    assert!(!code.contains("sub_126"), "{code}");
}

#[test]
fn aarch64_branchind_tail_call_recovers_function_pointer_call() {
    let options = aarch64_preview_options();

    let x0 = aarch64_reg(0x4000, 8);
    let w0 = aarch64_reg(0x4000, 4);
    let w1 = aarch64_reg(0x4008, 4);
    let w2 = aarch64_reg(0x4010, 4);
    let x3 = aarch64_reg(0x4018, 8);
    let branch_target = aarch64_reg(0, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100068,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100068,
                    output: Some(x3.clone()),
                    inputs: vec![x0],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x10006c,
                    output: Some(w0.clone()),
                    inputs: vec![w1.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100070,
                    output: Some(w1),
                    inputs: vec![w2],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100074,
                    output: Some(branch_target.clone()),
                    inputs: vec![x3],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::BranchInd,
                    address: 0x100074,
                    output: None,
                    inputs: vec![branch_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "apply_op", 0x100068, &options).expect("preview render");
    assert!(
        code.contains("return ((code *)param_1)(param_2, param_3);"),
        "{code}"
    );
    assert!(!code.contains("__fission_branchind"), "{code}");
}

#[test]
fn aarch64_same_block_wide_const_copy_overrides_dominating_register_alias() {
    let options = aarch64_preview_options();

    let w0 = aarch64_reg(0x4000, 4);
    let x0 = aarch64_reg(0x4000, 8);
    let x8 = aarch64_reg(0x4040, 8);
    let w8 = aarch64_reg(0x4040, 4);
    let x30 = aarch64_reg(0x40f0, 8);
    let ret_target = aarch64_reg(0, 8);
    let cond = uniq(0x5100, 1);
    let modulo_like = uniq(0x5200, 4);
    let xor_tmp = uniq(0x5300, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x1000,
                        output: Some(modulo_like.clone()),
                        inputs: vec![w0.clone(), cst(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1000,
                        output: Some(x8.clone()),
                        inputs: vec![modulo_like],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1004,
                        output: None,
                        inputs: vec![cst(0x1010, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1010,
                        output: Some(x8),
                        inputs: vec![cst(0xaaaa, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1014,
                        output: Some(xor_tmp.clone()),
                        inputs: vec![w0, w8],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1014,
                        output: Some(x0),
                        inputs: vec![xor_tmp],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1018,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1018,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::Return,
                        address: 0x1018,
                        output: None,
                        inputs: vec![cst(0, 8), x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "const_alias", 0x1000, &options).expect("preview render");
    assert!(code.contains("param_1 ^ 43690"), "{code}");
    assert!(
        !code.contains("param_1 ^ (uint)(param_1 - 5)") && !code.contains("param_1 ^ xVar"),
        "{code}"
    );
}

#[test]
fn aarch64_join_return_preserves_predecessor_local_const_alias() {
    let options = aarch64_preview_options();

    let w0 = aarch64_reg(0x4000, 4);
    let x0 = aarch64_reg(0x4000, 8);
    let x8 = aarch64_reg(0x4040, 8);
    let w8 = aarch64_reg(0x4040, 4);
    let x30 = aarch64_reg(0x40f0, 8);
    let ret_target = aarch64_reg(0, 8);
    let cond = uniq(0x6100, 1);
    let stale_x8 = uniq(0x6200, 4);
    let add_tmp = uniq(0x6300, 4);
    let xor_tmp = uniq(0x6400, 4);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2, 1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x1000,
                        output: Some(stale_x8.clone()),
                        inputs: vec![w0.clone(), cst(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1000,
                        output: Some(x8.clone()),
                        inputs: vec![stale_x8],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1004,
                        output: None,
                        inputs: vec![cst(0x1020, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![3],
                ops: vec![
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1010,
                        output: Some(add_tmp.clone()),
                        inputs: vec![w0.clone(), cst(10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1010,
                        output: Some(x0.clone()),
                        inputs: vec![add_tmp],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1014,
                        output: None,
                        inputs: vec![cst(0x1030, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: vec![3],
                ops: vec![
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1020,
                        output: Some(x8),
                        inputs: vec![cst(0xaaaa, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x1024,
                        output: Some(xor_tmp.clone()),
                        inputs: vec![w0, w8],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 8,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1024,
                        output: Some(x0),
                        inputs: vec![xor_tmp],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 3,
                start_address: 0x1030,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 9,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1030,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 10,
                        opcode: PcodeOpcode::Return,
                        address: 0x1030,
                        output: None,
                        inputs: vec![cst(0, 8), x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "join_const_alias", 0x1000, &options).expect("preview render");
    assert!(code.contains("param_1 + 10"), "{code}");
    assert!(code.contains("param_1 ^ 43690"), "{code}");
    assert!(
        !code.contains("param_1 ^ (uint)(param_1 - 5)") && !code.contains("param_1 ^ xVar"),
        "{code}"
    );
}

#[test]
fn aarch64_instruction_local_conditional_merge_returns_both_values() {
    let mut options = preview_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let w0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let x0 = Varnode {
        size: 8,
        ..w0.clone()
    };
    let w1 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4008,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let x8 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4040,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let w8 = Varnode {
        size: 4,
        ..x8.clone()
    };
    let cond = uniq(0x5200, 1);
    let diff = uniq(0x7b600, 4);
    let selected = uniq(0x39c00, 4);
    let negated = uniq(0x39c00, 4);
    let x30 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x40f0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x100054,
                successors: vec![2, 1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x100054,
                        output: Some(diff.clone()),
                        inputs: vec![w0, w1],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x100054,
                        output: Some(x8.clone()),
                        inputs: vec![diff.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Int2Comp,
                        address: 0x100058,
                        output: Some(negated),
                        inputs: vec![w8.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x100058,
                        output: None,
                        inputs: vec![cst(2, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x100058,
                successors: vec![2],
                ops: vec![PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100058,
                    output: Some(selected.clone()),
                    inputs: vec![w8],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x100058,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x100058,
                        output: Some(x0),
                        inputs: vec![selected],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x10005c,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::Return,
                        address: 0x10005c,
                        output: None,
                        inputs: vec![cst(0, 8), x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "op_sub", 0x100054, &options).expect("preview render");
    assert!(!code.contains("__fission_branchind"), "{code}");
    assert!(!code.contains("var_39c00"), "{code}");
    assert!(code.contains("return -(param_1 - param_2);"), "{code}");
    assert!(code.contains("return param_1 - param_2;"), "{code}");
}

// ── Non-param registers must always use hardware names ─────────────────────────

#[test]
fn rax_is_never_a_param() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
        CallingConvention::AArch64,
    ] {
        if abi == CallingConvention::AArch64 {
            let (name, idx) = register_name_with_param(0x00, 8, abi).unwrap();
            assert_eq!(name, "param_1");
            assert_eq!(idx, Some(0));
        } else {
            let (name, idx) = register_name_with_param(0x00, 8, abi).unwrap();
            assert_eq!(name, "rax", "rax should stay 'rax' in {abi:?}");
            assert_eq!(idx, None, "rax must not be a param in {abi:?}");
        }
    }
}

#[test]
fn rsp_is_never_a_param() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
        CallingConvention::AArch64,
    ] {
        if abi == CallingConvention::AArch64 {
            assert!(register_name_with_param(0x20, 8, abi).is_none());
        } else {
            let (name, idx) = register_name_with_param(0x20, 8, abi).unwrap();
            assert_eq!(name, "rsp");
            assert_eq!(idx, None);
        }
    }
}

#[test]
fn unknown_offset_returns_none() {
    for abi in [
        CallingConvention::WindowsX64,
        CallingConvention::SystemVAmd64,
        CallingConvention::AArch64,
    ] {
        assert!(register_name_with_param(0xDEAD, 8, abi).is_none());
    }
}

// ── x64_ghidra_reg_name is always ABI-independent ─────────────────────────────

#[test]
fn ghidra_reg_name_is_hardware_canonical() {
    assert_eq!(x64_ghidra_reg_name(0x00), Some("rax"));
    assert_eq!(x64_ghidra_reg_name(0x08), Some("rcx"));
    assert_eq!(x64_ghidra_reg_name(0x10), Some("rdx"));
    assert_eq!(x64_ghidra_reg_name(0x30), Some("rsi"));
    assert_eq!(x64_ghidra_reg_name(0x38), Some("rdi"));
    assert_eq!(x64_ghidra_reg_name(0x80), Some("r8"));
    assert_eq!(x64_ghidra_reg_name(0x88), Some("r9"));
    assert_eq!(x64_ghidra_reg_name(0xDEAD), None);
}

#[test]
fn abi_state_classifies_win64_home_slot() {
    let abi = AbiState::new(CallingConvention::WindowsX64, true, 8, 0x40);
    assert_eq!(
        abi.classify_stack_slot_origin(StackBase::Rsp, 0x40),
        NirBindingOrigin::HomeSlot(0x40)
    );
    assert_eq!(
        abi.classify_stack_slot_origin(StackBase::Rsp, 0x20),
        NirBindingOrigin::StackOffset(0x20)
    );
}

#[test]
fn abi_state_recovers_win64_stack_arg_index() {
    let abi = AbiState::new(CallingConvention::WindowsX64, true, 8, 0x40);
    assert_eq!(abi.stack_argument_index(0x20), Some(0));
    assert_eq!(abi.stack_argument_index(0x28), Some(1));
    assert_eq!(abi.stack_argument_index(0x18), None);
}
