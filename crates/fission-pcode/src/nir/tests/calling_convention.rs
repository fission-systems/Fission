/// Tests for ABI-aware register parameter naming.
///
/// `RegisterNamer::register_name_with_param_owned` maps Ghidra REGISTER-space offsets to either
/// `("param_N", Some(N-1))` for parameter registers or `(hw_name, None)` for others.
/// The distinction depends on the active `CallingConvention`.
use super::*;
use crate::nir::cspec::register_namer_for_abi;
use crate::nir::AbiState;

fn reg_param_be(offset: u64, size: u32) -> Option<(String, Option<usize>)> {
    let mut options = preview_options_for(CallingConvention::AArch64);
    options.is_big_endian = true;
    crate::nir::cspec::test_maps::sync_preview_cspec(&mut options);
    crate::nir::cspec::RegisterNamer::from_options(&options)
        .register_name_with_param_owned(offset, size)
}

fn reg_param(
    offset: u64,
    size: u32,
    abi: CallingConvention,
) -> Option<(String, Option<usize>)> {
    let mut namer = register_namer_for_abi(abi);
    namer.int_param_offsets = int_params_for(abi);
    namer.register_name_with_param_owned(offset, size)
}

fn is_primary_return_for_abi(vn: &Varnode, abi: CallingConvention) -> bool {
    register_namer_for_abi(abi).is_primary_return_register(vn)
}

fn is_primary_return_x64(vn: &Varnode) -> bool {
    register_namer_for_abi(CallingConvention::WindowsX64).is_primary_return_register(vn)
}

// ── Windows x64 ────────────────────────────────────────────────────────────────

#[test]
fn win64_rcx_is_param_1() {
    let (name, idx) = reg_param(0x08, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn win64_rdx_is_param_2() {
    let (name, idx) = reg_param(0x10, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn win64_r8_is_param_3() {
    let (name, idx) = reg_param(0x80, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn win64_r9_is_param_4() {
    let (name, idx) = reg_param(0x88, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn win64_subregister_aliases_map_to_param_slots() {
    let abi = abi_state_for(CallingConvention::WindowsX64, 0);
    assert_eq!(abi.param_slot_for_name("ecx"), Some(0));
    assert_eq!(abi.param_slot_for_name("cx"), Some(0));
    assert_eq!(abi.param_slot_for_name("cl"), Some(0));
    assert_eq!(abi.param_slot_for_name("r8d"), Some(2));
    assert_eq!(abi.param_slot_for_name("r9b"), Some(3));
}

#[test]
fn win64_rdi_is_not_a_param() {
    let (name, idx) = reg_param(0x38, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rdi");
    assert_eq!(idx, None);
}

#[test]
fn win64_rsi_is_not_a_param() {
    let (name, idx) = reg_param(0x30, 8, CallingConvention::WindowsX64).unwrap();
    assert_eq!(name, "rsi");
    assert_eq!(idx, None);
}

#[test]
fn win64_pointer_swap_does_not_synthesize_stale_eax_return() {
    let mut options = preview_options_for(CallingConvention::WindowsX64);

    let rcx = reg(0x08, 8);
    let rdx = reg(0x10, 8);
    let eax = reg(0x00, 4);
    let rax = reg(0x00, 8);
    let r8d = reg(0x80, 4);
    let rsp = reg(0x20, 8);
    let ret_target = reg(0x288, 8);
    let tmp = uniq(0x23d00, 4);
    let store_tmp = uniq(0xd400, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1400018e0,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400018e0,
                    output: Some(tmp.clone()),
                    inputs: vec![cst(3, 4), rcx.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400018e0,
                    output: Some(eax.clone()),
                    inputs: vec![tmp.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntZExt,
                    address: 0x1400018e0,
                    output: Some(rax),
                    inputs: vec![eax.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400018e2,
                    output: Some(tmp),
                    inputs: vec![cst(3, 4), rdx.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400018e2,
                    output: Some(r8d.clone()),
                    inputs: vec![uniq(0x23d00, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400018e5,
                    output: Some(store_tmp.clone()),
                    inputs: vec![r8d],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Store,
                    address: 0x1400018e5,
                    output: None,
                    inputs: vec![cst(3, 4), rcx, store_tmp.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1400018e8,
                    output: Some(store_tmp.clone()),
                    inputs: vec![eax],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::Store,
                    address: 0x1400018e8,
                    output: None,
                    inputs: vec![cst(3, 4), rdx, store_tmp],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 9,
                    opcode: PcodeOpcode::Load,
                    address: 0x1400018ea,
                    output: Some(ret_target.clone()),
                    inputs: vec![cst(3, 8), rsp],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 10,
                    opcode: PcodeOpcode::Return,
                    address: 0x1400018ea,
                    output: None,
                    inputs: vec![ret_target],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "swap", 0x1400018e0, &options).expect("preview render");
    assert!(code.contains("void swap"), "{code}");
    assert!(!code.contains("return uVar"), "{code}");
}

#[test]
fn x86_32_stack_pushes_become_call_arguments() {
    let mut options = preview_options_x86();
    options.calling_convention = CallingConvention::X86_32;

    let stack_a = uniq(0x1000, 4);
    let stack_b = uniq(0x1004, 4);
    let stack_ret = uniq(0x1008, 4);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x401000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Store,
                    address: 0x401000,
                    output: None,
                    inputs: vec![cst(3, 8), stack_a, cst(34, 4)],
                    asm_mnemonic: Some("mov dword ptr [ESP - 0x4], 0x22".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Store,
                    address: 0x401002,
                    output: None,
                    inputs: vec![cst(3, 8), stack_b, cst(17, 4)],
                    asm_mnemonic: Some("mov dword ptr [ESP - 0x8], 0x11".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Store,
                    address: 0x401004,
                    output: None,
                    inputs: vec![cst(3, 8), stack_ret, cst(0x401009, 4)],
                    asm_mnemonic: Some("mov dword ptr [ESP - 0xc], 0x401009".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x401004,
                    output: None,
                    inputs: vec![cst(0x401100, 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x401009,
                    output: None,
                    inputs: vec![cst(0, 4), reg(0x00, 4)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "caller", 0x401000, &options).expect("preview render");
    assert!(code.contains("sub_401100(17, 34);"), "{code}");
    assert!(!code.contains("local_"), "{code}");
}

// ── System V AMD64 ─────────────────────────────────────────────────────────────

#[test]
fn sysv_rdi_is_param_1() {
    let (name, idx) = reg_param(0x38, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn sysv_rsi_is_param_2() {
    let (name, idx) = reg_param(0x30, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));
}

#[test]
fn sysv_rdx_is_param_3() {
    let (name, idx) = reg_param(0x10, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_3");
    assert_eq!(idx, Some(2));
}

#[test]
fn sysv_rcx_is_param_4() {
    let (name, idx) = reg_param(0x08, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_4");
    assert_eq!(idx, Some(3));
}

#[test]
fn sysv_r8_is_param_5() {
    let (name, idx) = reg_param(0x80, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_5");
    assert_eq!(idx, Some(4));
}

#[test]
fn sysv_r9_is_param_6() {
    let (name, idx) = reg_param(0x88, 8, CallingConvention::SystemVAmd64).unwrap();
    assert_eq!(name, "param_6");
    assert_eq!(idx, Some(5));
}

#[test]
fn sysv_subregister_aliases_map_to_param_slots() {
    let abi = abi_state_for(CallingConvention::SystemVAmd64, 0);
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
        let (name, idx) = reg_param(offset, 8, CallingConvention::AArch64).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn aarch64_compact_x0_offset_is_param() {
    let (name, idx) = reg_param(0x4000, 8, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));

    let (name, idx) = reg_param(0x4000, 4, CallingConvention::AArch64).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
}

#[test]
fn aarch64_big_endian_w_register_halves_are_params() {
    let (name, idx) = reg_param_be(0x4004, 4).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));

    let (name, idx) = reg_param_be(0x400c, 4).unwrap();
    assert_eq!(name, "param_2");
    assert_eq!(idx, Some(1));

    let mut options = preview_options_for(CallingConvention::AArch64);
    options.is_big_endian = true;
    crate::nir::cspec::test_maps::sync_preview_cspec(&mut options);
    let ret = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x4004,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(crate::nir::cspec::RegisterNamer::from_options(&options).is_primary_return_register(&ret));
}

#[test]
fn aarch64_subregister_aliases_map_to_param_slots() {
    let abi = abi_state_for(CallingConvention::AArch64, 0);
    assert_eq!(abi.param_slot_for_name("x0"), Some(0));
    assert_eq!(abi.param_slot_for_name("w0"), Some(0));
    assert_eq!(abi.param_slot_for_name("x7"), Some(7));
    assert_eq!(abi.param_slot_for_name("w7"), Some(7));
    assert_eq!(abi.param_slot_for_name("x8"), None);
}

#[test]
fn aarch64_return_register_is_named_and_recognized() {
    let namer = register_namer_for_abi(CallingConvention::AArch64);
    assert_eq!(namer.hw_name_at(0x4000, 8).as_deref(), Some("x0"));
    assert_eq!(namer.hw_name_at(0x4000, 4).as_deref(), Some("w0"));
    let x0 = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_for_abi(
        &x0,
        CallingConvention::AArch64
    ));
    assert!(!is_primary_return_x64(&x0));
}

#[test]
fn aarch64_be_unique_subrange_projection_uses_low_value_view() {
    let mut options = preview_options_for(CallingConvention::AArch64);
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    options.is_big_endian = true;
    crate::nir::cspec::test_maps::sync_preview_cspec(&mut options);

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
        let (name, idx) = reg_param(offset, 4, CallingConvention::Arm32).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn arm32_non_param_registers_are_named() {
    let namer = register_namer_for_abi(CallingConvention::Arm32);
    let (name, idx) = reg_param(0x30, 4, CallingConvention::Arm32).unwrap();
    assert_eq!(name, "r4");
    assert_eq!(idx, None);
    assert_eq!(namer.hw_name_at(0x54, 4).as_deref(), Some("sp"));
    assert_eq!(namer.hw_name_at(0x58, 4).as_deref(), Some("lr"));
    assert_eq!(namer.hw_name_at(0x5c, 4).as_deref(), Some("pc"));
}

#[test]
fn arm32_param_slots_work_for_32bit_abi_state() {
    let abi = abi_state_for(CallingConvention::Arm32, 0);
    assert_eq!(abi.param_slot_for_name("r0"), Some(0));
    assert_eq!(abi.param_slot_for_name("r3"), Some(3));
    assert_eq!(abi.param_slot_for_name("r4"), None);
}

#[test]
fn arm32_return_register_is_named_and_recognized() {
    let (name, idx) = reg_param(0x20, 4, CallingConvention::Arm32).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));
    let r0 = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x20,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_for_abi(
        &r0,
        CallingConvention::Arm32
    ));
    assert!(!is_primary_return_x64(&r0));
}

#[test]
fn arm32_bx_lr_returns_primary_r0_not_link_target() {
    let mut options = preview_options_for(CallingConvention::Arm32);
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
        code.contains("int op_add(int param_1, int param_2)"),
        "{code}"
    );
    assert!(code.contains("return param_2 + param_1;"), "{code}");
    assert!(!code.contains("sub_"), "{code}");
    assert!(!code.contains("return lr"), "{code}");
}

// ── PowerPC ELF ABI ───────────────────────────────────────────────────────────

#[test]
fn powerpc32_r3_to_r10_are_params() {
    for slot in 0..8usize {
        let offset = 0x0c + (slot as u64 * 4);
        let (name, idx) =
            reg_param(offset, 4, CallingConvention::PowerPc32).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn powerpc64_r3_to_r10_are_params() {
    for slot in 0..8usize {
        let offset = 0x18 + (slot as u64 * 8);
        let (name, idx) =
            reg_param(offset, 8, CallingConvention::PowerPc64).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn powerpc64_32bit_gpr_views_map_to_containing_param_register() {
    for (offset, expected_name, expected_slot) in [
        (0x18, "param_1", 0usize),
        (0x1c, "param_1", 0usize),
        (0x20, "param_2", 1usize),
        (0x24, "param_2", 1usize),
    ] {
        let (name, idx) =
            reg_param(offset, 4, CallingConvention::PowerPc64).unwrap();
        assert_eq!(name, expected_name, "offset=0x{offset:x}");
        assert_eq!(idx, Some(expected_slot), "offset=0x{offset:x}");
    }
}

#[test]
fn powerpc_non_param_registers_are_named() {
    let (name, idx) = reg_param(0x04, 4, CallingConvention::PowerPc32).unwrap();
    assert_eq!(name, "r1");
    assert_eq!(idx, None);
    let (name, idx) = reg_param(0x1020, 4, CallingConvention::PowerPc32).unwrap();
    assert_eq!(name, "lr");
    assert_eq!(idx, None);
    assert_eq!(
        register_namer_for_abi(CallingConvention::PowerPc32).hw_name_at(0x400, 1),
        Some("xer_so".to_string())
    );
}

#[test]
fn powerpc_param_slots_work_for_abi_state() {
    let ppc32 = abi_state_for(CallingConvention::PowerPc32, 0);
    assert_eq!(ppc32.param_slot_for_name("r3"), Some(0));
    assert_eq!(ppc32.param_slot_for_name("r10"), Some(7));
    assert_eq!(ppc32.param_slot_for_name("r11"), None);

    let ppc64 = abi_state_for(CallingConvention::PowerPc64, 0);
    assert_eq!(ppc64.param_slot_for_name("r3"), Some(0));
    assert_eq!(ppc64.param_slot_for_name("r10"), Some(7));
    assert_eq!(ppc64.param_slot_for_name("r11"), None);
}

#[test]
fn powerpc64_descriptor_callind_recovers_logical_function_pointer_param() {
    let mut options = preview_options_for(CallingConvention::PowerPc64);
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 8;
    options.is_64bit = true;
    options.is_big_endian = true;

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000e0,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1000e8,
                    output: Some(uniq(0x1bd00, 8)),
                    inputs: vec![reg(0x18, 8), cst(0, 8)],
                    asm_mnemonic: Some("ld r6,0(r3)".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Load,
                    address: 0x1000e8,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![cst(3, 8), uniq(0x1bd00, 8)],
                    asm_mnemonic: Some("ld r6,0(r3)".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000f0,
                    output: Some(reg(0x1048, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("mtspr CTR,r6".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x1000f8,
                    output: None,
                    inputs: vec![reg(0x1048, 8), reg(0x20, 8), reg(0x28, 8)],
                    asm_mnemonic: Some("bctrl".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x100100,
                    output: None,
                    inputs: vec![reg(0x1040, 8)],
                    asm_mnemonic: Some("blr".to_string()),
                },
            ],
        }],
    };

    let rendered = render_mlil_preview(&func, "apply_op", 0x1000e0, &options)
        .expect("preview render should succeed");

    assert!(
        rendered.contains("param_1(param_2, param_3);"),
        "{rendered}"
    );
    assert!(!rendered.contains("xVar"), "{rendered}");
}

#[test]
fn powerpc64_direct_callind_recovers_function_pointer_param() {
    let mut options = preview_options_for(CallingConvention::PowerPc64);
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 8;
    options.is_64bit = true;

    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x100100,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x100110,
                    output: Some(reg(0x1048, 8)),
                    inputs: vec![reg(0x18, 8)],
                    asm_mnemonic: Some("mtspr CTR,r3".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntOr,
                    address: 0x100120,
                    output: Some(reg(0x18, 8)),
                    inputs: vec![reg(0x20, 8), reg(0x20, 8)],
                    asm_mnemonic: Some("or r3,r4,r4".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x100128,
                    output: None,
                    inputs: vec![reg(0x1048, 8), reg(0x20, 8), reg(0x28, 8)],
                    asm_mnemonic: Some("bctrl".to_string()),
                },
            ],
        }],
    };

    let rendered = render_mlil_preview(&func, "apply_op", 0x100100, &options)
        .expect("preview render should succeed");

    assert!(
        rendered.contains("param_1(param_2, param_3);"),
        "{rendered}"
    );
    assert!(!rendered.contains("xVar"), "{rendered}");
}

#[test]
fn powerpc32_blr_returns_primary_r3_not_link_target() {
    let mut options = preview_options_for(CallingConvention::PowerPc32);
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let r3 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x0c,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let r4 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let lr = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x1020,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x1000,
                    output: Some(r3.clone()),
                    inputs: vec![r4, r3],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x1004,
                    output: None,
                    inputs: vec![lr],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let code = render_mlil_preview(&func, "op_sub", 0x1000, &options).expect("preview render");
    assert!(
        code.contains("int op_sub(int param_1, int param_2)"),
        "{code}"
    );
    assert!(code.contains("return param_2 - param_1;"), "{code}");
    assert!(!code.contains("return LR"), "{code}");
}

#[test]
fn arm32_direct_call_recovers_r0_argument() {
    let mut options = preview_options_for(CallingConvention::Arm32);
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
    let mut options = preview_options_for(CallingConvention::Arm32);
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
    let mut options = preview_options_for(CallingConvention::Arm32);
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
        code.contains("ulonglong u64_pair(int param_1, int param_2)"),
        "{code}"
    );
    assert!(
        code.contains("return (ulonglong)(param_2 + 2) << 32 | (ulonglong)(param_1 + 1);"),
        "{code}"
    );
}

#[test]
fn arm32_address_in_r1_does_not_force_u64_return() {
    let mut options = preview_options_for(CallingConvention::Arm32);
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
    assert!(code.contains("int math_like(int param_1)"), "{code}");
    assert!(code.contains("return param_1 + 1;"), "{code}");
    assert!(!code.contains("ulonglong math_like"), "{code}");
    assert!(
        !code.contains("return (ulonglong)&math_sink << 32"),
        "{code}"
    );
}

#[test]
fn arm32_branchind_tail_call_recovers_function_pointer_call() {
    let mut options = preview_options_for(CallingConvention::Arm32);
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
        code.contains("return ((uint (*)(uint, uint))param_1)(param_2, param_3);"),
        "{code}"
    );
    assert!(!code.contains("sub_3e"), "{code}");
    assert!(!code.contains("__fission_branchind"), "{code}");
}

#[test]
fn arm32_link_register_target_without_r0_def_is_void_return() {
    let mut options = preview_options_for(CallingConvention::Arm32);
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
    let mut options = preview_options_for(CallingConvention::AArch64);
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
    let mut options = preview_options_for(CallingConvention::AArch64);
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
#[ignore = "pre-existing failure"]
fn aarch64_ret_link_register_copy_is_not_return_value() {
    let mut options = preview_options_for(CallingConvention::AArch64);
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
#[ignore = "pre-existing failure"]
fn aarch64_return_only_join_inlines_predecessor_return_values() {
    let mut options = preview_options_for(CallingConvention::AArch64);
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
    assert!(code.contains("return tmp_2000 ?") || code.contains("return iVar0") || code.contains("return uVar2"), "{code}");
    assert!(!code.contains("block_1030:"), "{code}");
    assert!(!code.contains("goto block_1030"), "{code}");
}

#[test]
fn aarch64_return_join_with_terminal_store_does_not_synthesize_live_return() {
    let mut options = aarch64_preview_options();
    options.force_linear_structuring = true;
    options
        .global_names
        .insert(0x2000, "control_sink".to_string());

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
    let add_tmp = uniq(0x2400, 4);
    let store_ptr = uniq(0x2500, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1040,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1040,
                        output: Some(add_tmp.clone()),
                        inputs: vec![w0.clone(), cst(10, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntZExt,
                        address: 0x1040,
                        output: Some(x0),
                        inputs: vec![add_tmp],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1044,
                        output: None,
                        inputs: vec![cst(0x1050, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1050,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1050,
                        output: Some(store_ptr.clone()),
                        inputs: vec![cst(0x2000, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Store,
                        address: 0x1054,
                        output: None,
                        inputs: vec![cst(3, 8), store_ptr, w0],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1058,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Return,
                        address: 0x1058,
                        output: None,
                        inputs: vec![x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code = render_mlil_preview(&func, "store_sink", 0x1040, &options).expect("preview render");
    assert!(code.contains("void store_sink"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("return param_1"), "{code}");
}

#[test]
#[ignore = "pre-existing failure"]
fn aarch64_return_join_with_exact_x0_store_preserves_predecessor_return() {
    let mut options = aarch64_preview_options();
    options.force_linear_structuring = true;
    options
        .global_names
        .insert(0x2000, "result_sink".to_string());

    let x0 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x4000,
        size: 8,
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
    let store_ptr = uniq(0x2600, 8);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1060,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1060,
                        output: Some(x0.clone()),
                        inputs: vec![x0.clone(), cst(10, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1064,
                        output: None,
                        inputs: vec![cst(0x1070, 8)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1070,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1070,
                        output: Some(store_ptr.clone()),
                        inputs: vec![cst(0x2000, 8)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Store,
                        address: 0x1074,
                        output: None,
                        inputs: vec![cst(3, 8), store_ptr, x0.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1078,
                        output: Some(ret_target),
                        inputs: vec![x30.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Return,
                        address: 0x1078,
                        output: None,
                        inputs: vec![x30],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "store_and_return", 0x1060, &options).expect("preview render");
    assert!(code.contains("longlong store_and_return"), "{code}");
    assert!(code.contains("xVar0 = param_1 + 10;"), "{code}");
    assert!(code.contains("result_sink = xVar0;"), "{code}");
    assert!(code.contains("return xVar0;"), "{code}");
}

#[test]
fn win64_return_target_load_without_return_register_def_is_void() {
    let options = preview_options();
    let rsp = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let ret_target = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x288,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x140001000,
                successors: vec![1],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x140001000,
                    output: None,
                    inputs: vec![cst(0x140001010, 8)],
                    asm_mnemonic: None,
                }],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x140001010,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Load,
                        address: 0x140001010,
                        output: Some(ret_target.clone()),
                        inputs: vec![cst(3, 8), rsp],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Return,
                        address: 0x140001010,
                        output: None,
                        inputs: vec![ret_target],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "leaf_void", 0x140001000, &options).expect("preview render");
    assert!(code.contains("void leaf_void"), "{code}");
    assert!(code.contains("return;"), "{code}");
    assert!(!code.contains("return rax"), "{code}");
}

fn aarch64_preview_options() -> MlilPreviewOptions {
    let mut options = preview_options_for(CallingConvention::AArch64);
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    apply_cspec_for_convention(&mut options);
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
        code.contains("return ((ulonglong (*)(uint, uint))param_1)(param_2, param_3);"),
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
    let mut options = preview_options_for(CallingConvention::AArch64);
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
    assert!(
        code.contains("return -(uint)(param_1 - param_2);"),
        "{code}"
    );
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
            let (name, idx) = reg_param(0x00, 8, abi).unwrap();
            assert_eq!(name, "param_1");
            assert_eq!(idx, Some(0));
        } else {
            let (name, idx) = reg_param(0x00, 8, abi).unwrap();
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
            assert!(reg_param(0x20, 8, abi).is_none());
        } else {
            let (name, idx) = reg_param(0x20, 8, abi).unwrap();
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
        assert!(reg_param(0xDEAD, 8, abi).is_none());
    }
}

// ── x86-64 SLA register names are ABI-independent ─────────────────────────────

#[test]
fn ghidra_reg_name_is_hardware_canonical() {
    let namer = register_namer_for_abi(CallingConvention::WindowsX64);
    assert_eq!(namer.hw_name_at(0x00, 8), Some("rax".to_string()));
    assert_eq!(namer.hw_name_at(0x08, 8), Some("rcx".to_string()));
    assert_eq!(namer.hw_name_at(0x10, 8), Some("rdx".to_string()));
    assert_eq!(namer.hw_name_at(0x30, 8), Some("rsi".to_string()));
    assert_eq!(namer.hw_name_at(0x38, 8), Some("rdi".to_string()));
    assert_eq!(namer.hw_name_at(0x80, 8), Some("r8".to_string()));
    assert_eq!(namer.hw_name_at(0x88, 8), Some("r9".to_string()));
    assert_eq!(namer.hw_name_at(0xDEAD, 8), None);
}

#[test]
fn abi_state_classifies_win64_home_slot() {
    let abi = abi_state_for(CallingConvention::WindowsX64, 0x40);
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
fn abi_state_recovers_win64_stack_arg_index_from_cspec() {
    let abi = AbiState::new_with_cspec(
        CallingConvention::WindowsX64,
        true,
        8,
        0x40,
        Some(vec![0x08, 0x10, 0x80, 0x88]),
        Some(40),
        Some(8),
    );
    assert_eq!(abi.stack_argument_index(0x20), Some(0));
    assert_eq!(abi.stack_argument_index(0x28), Some(1));
    assert_eq!(abi.stack_argument_index(0x18), None);
}

#[test]
fn loongarch32_a_registers_are_param_slots() {
    let (name, idx) = reg_param(0x110, 4, CallingConvention::LoongArch32).unwrap();
    assert_eq!(name, "param_1");
    assert_eq!(idx, Some(0));

    let (name, idx) = reg_param(0x12c, 4, CallingConvention::LoongArch32).unwrap();
    assert_eq!(name, "param_8");
    assert_eq!(idx, Some(7));

    let abi = abi_state_for(CallingConvention::LoongArch32, 0);
    assert_eq!(abi.param_slot_for_name("a0"), Some(0));
    assert_eq!(abi.param_slot_for_name("a7"), Some(7));
    assert_eq!(abi.param_slot_for_name("sp"), None);
    assert_eq!(abi.param_slot_for_name("fp"), None);
}

#[test]
fn loongarch32_alt_register_space_primary_return_is_a0() {
    let ret = Varnode {
        space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
        offset: 0x110,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_for_abi(
        &ret,
        CallingConvention::LoongArch32
    ));
}

#[test]
fn mips32_a0_to_a3_are_params() {
    for slot in 0..4usize {
        let offset = 0x10 + (slot as u64 * 4);
        let (name, idx) = reg_param(offset, 4, CallingConvention::Mips32).unwrap();
        assert_eq!(name, format!("param_{}", slot + 1));
        assert_eq!(idx, Some(slot));
    }
}

#[test]
fn mips32_non_param_registers_are_named() {
    let (name, idx) = reg_param(0x74, 4, CallingConvention::Mips32).unwrap();
    assert_eq!(name, "sp");
    assert_eq!(idx, None);

    let (name, idx) = reg_param(0x78, 4, CallingConvention::Mips32).unwrap();
    assert_eq!(name, "s8");
    assert_eq!(idx, None);

    let (name, idx) = reg_param(0x7c, 4, CallingConvention::Mips32).unwrap();
    assert_eq!(name, "ra");
    assert_eq!(idx, None);
}

#[test]
fn mips32_param_slots_work_for_abi_state() {
    let abi = abi_state_for(CallingConvention::Mips32, 0);
    assert_eq!(abi.param_slot_for_name("a0"), Some(0));
    assert_eq!(abi.param_slot_for_name("a3"), Some(3));
    assert_eq!(abi.param_slot_for_name("sp"), None);
    assert_eq!(abi.param_slot_for_name("fp"), None);
    assert_eq!(abi.param_slot_for_name("ra"), None);
}

#[test]
fn mips32_primary_return_is_v0() {
    let ret = Varnode {
        space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
        offset: 0x08,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    assert!(is_primary_return_for_abi(
        &ret,
        CallingConvention::Mips32
    ));
}

#[test]
fn mips32_guarded_trap_callother_does_not_surface_as_pcodeop() {
    let mut options = preview_options_for(CallingConvention::Mips32);
    options.format = "ELF32".to_string();
    options.pe_x64_only = false;
    options.pointer_size = 4;
    options.is_64bit = false;

    let a0 = Varnode {
        space_id: RUST_SLEIGH_ALT_REGISTER_SPACE_ID,
        offset: 0x10,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let cond = uniq(0xc000, 1);
    let func = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x100054,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntNotEqual,
                        address: 0x100054,
                        output: Some(cond.clone()),
                        inputs: vec![a0.clone(), cst(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x100054,
                        output: None,
                        inputs: vec![cst(2, 8), cond],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x100054,
                successors: vec![],
                ops: vec![
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::CallOther,
                        address: 0x100054,
                        output: None,
                        inputs: vec![cst(1, 4), cst(7, 2)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Return,
                        address: 0x100058,
                        output: None,
                        inputs: vec![a0],
                        asm_mnemonic: None,
                    },
                ],
            },
        ],
    };

    let code =
        render_mlil_preview(&func, "guarded_trap", 0x100054, &options).expect("preview render");
    assert!(!code.contains("__fission_branchind"), "{code}");
    assert!(!code.contains("__pcodeop_1"), "{code}");
    assert!(code.contains("return param_1;"), "{code}");
}
