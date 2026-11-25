//! Host memory safety regression tests for Cranelift JIT compilation,
//! FFI callbacks, register-mirror bounds, and code-cache lifecycle.

use fission_emulator::jit::cache::{JitBlock, JitCache};
use fission_emulator::jit::callbacks::{
    MAX_JIT_BULK_BYTES, MAX_JIT_CALLOTHER_ARGS, jit_call_other, jit_callother_result, jit_chain,
    jit_count_insn, jit_count_pcode, jit_exit_tb, jit_hle_trap, jit_host_reg_base,
    jit_insn_checkpoint, jit_observe_block, jit_observe_insn, jit_read_bytes, jit_read_memory,
    jit_read_register, jit_read_space, jit_taint_output, jit_wide_op, jit_write_bytes,
    jit_write_memory, jit_write_register, jit_write_space,
};
use fission_emulator::jit::compiler::{
    GuestInsn, JitCompiler, MAX_JIT_STACK_BUFFER_BYTES, can_direct_host_reg_access,
};
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::state::{HOST_REG_FILE_SIZE, MachineState};
use fission_emulator::{ArchInfo, Emulator};
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};
use std::path::PathBuf;
use std::sync::Arc;

fn make_test_emu() -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load real test ELF");
    let mut state = MachineState::new();
    let _info =
        fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("load_elf");
    let load_spec = binary.load_spec().expect("load spec").clone();
    let sleigh =
        fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
            &load_spec,
        )
        .expect("sleigh frontend candidates")
        .into_iter()
        .next()
        .expect("at least one sleigh frontend");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch info");
    Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator")
}

fn reg_vn(space_id: u64, offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn const_vn(val: i64, size: u32) -> Varnode {
    Varnode {
        space_id: 0,
        offset: 0,
        size,
        is_constant: true,
        constant_val: val,
    }
}

#[test]
fn host_reg_mirror_bounds_reject_oob_overflow_and_non_power_of_two_widths() {
    let mirror = HOST_REG_FILE_SIZE as u64;
    for &sz in &[1u32, 2, 4, 8] {
        assert!(can_direct_host_reg_access(0, sz));
        assert!(can_direct_host_reg_access(mirror - u64::from(sz), sz));
        assert!(!can_direct_host_reg_access(mirror - u64::from(sz) + 1, sz));
        assert!(!can_direct_host_reg_access(mirror, sz));
    }

    // Odd / unsupported host widths never emit raw direct loads or stores.
    for &bad_sz in &[0u32, 3, 5, 6, 7, 9, 16] {
        assert!(!can_direct_host_reg_access(0, bad_sz));
        assert!(!can_direct_host_reg_access(mirror - 8, bad_sz));
    }

    // Arithmetic overflow at u64::MAX must never wrap inside HOST_REG_FILE_SIZE.
    assert!(!can_direct_host_reg_access(u64::MAX, 1));
    assert!(!can_direct_host_reg_access(u64::MAX - 3, 4));
    assert!(!can_direct_host_reg_access(u64::MAX - 7, 8));
}

#[test]
fn compiled_jit_edge_and_odd_register_accesses_preserve_host_bounds_and_wx() {
    let mut emu = make_test_emu();
    let reg_space = emu.state.register_space();
    let unique_space = emu.state.unique_space();

    // Seed exact bytes near and past the end of `host_reg_file`.
    let tail_byte_off = (HOST_REG_FILE_SIZE - 1) as u64;
    let odd_3byte_off = (HOST_REG_FILE_SIZE - 16) as u64;
    let beyond_off = (HOST_REG_FILE_SIZE + 32) as u64;

    emu.state
        .write_space(reg_space, tail_byte_off, &[0xAB])
        .expect("seed tail byte");
    emu.state
        .write_space(reg_space, odd_3byte_off, &[0x11, 0x22, 0x33])
        .expect("seed 3-byte reg");

    // Block:
    //  1) Read 1-byte register at `HOST_REG_FILE_SIZE - 1`, zero-extend to 8 bytes at offset 0.
    //  2) Copy 3-byte register at `HOST_REG_FILE_SIZE - 16` into `beyond_off` (outside mirror).
    //  3) Write 1-byte constant `0x5C` back to `tail_byte_off`.
    let ops = vec![
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntZExt,
            address: 0x2000,
            output: Some(reg_vn(reg_space, 0, 8)),
            inputs: vec![reg_vn(reg_space, tail_byte_off, 1)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Copy,
            address: 0x2000,
            output: Some(reg_vn(reg_space, beyond_off, 3)),
            inputs: vec![reg_vn(reg_space, odd_3byte_off, 3)],
            asm_mnemonic: None,
        },
        PcodeOp {
            seq_num: 2,
            opcode: PcodeOpcode::Copy,
            address: 0x2000,
            output: Some(reg_vn(reg_space, tail_byte_off, 1)),
            inputs: vec![const_vn(0x5C, 1)],
            asm_mnemonic: None,
        },
    ];

    let mut compiler = JitCompiler::new().expect("JITCompiler");
    let mut wide_ops = Vec::new();
    let host_ptr = compiler
        .compile_translation_block(
            &[GuestInsn {
                pc: 0x2000,
                len: 4,
                ops,
            }],
            reg_space,
            unique_space,
            false,
            &mut wide_ops,
        )
        .expect("compile bounded register edge ops");
    assert!(!host_ptr.is_null());
    assert!(wide_ops.is_empty());

    let f: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(host_ptr) };
    let next_pc = f(&mut emu as *mut Emulator);
    assert_eq!(next_pc, 0x2004);

    assert_eq!(
        emu.state.read_space(reg_space, 0, 8).expect("read ext"),
        0xABu64.to_le_bytes()
    );
    assert_eq!(
        emu.state
            .read_space(reg_space, beyond_off, 3)
            .expect("read out-of-mirror 3B reg"),
        vec![0x11, 0x22, 0x33]
    );
    assert_eq!(
        emu.state
            .read_space(reg_space, tail_byte_off, 1)
            .expect("read tail byte"),
        vec![0x5C]
    );
}

#[test]
fn compiler_rejects_oversized_stack_slots_and_callother_argc() {
    let mut compiler = JitCompiler::new().expect("JITCompiler");
    let mut wide_ops = Vec::new();
    let oversized = MAX_JIT_STACK_BUFFER_BYTES + 8;

    for huge in [oversized, u32::MAX] {
        let copy_op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1000,
            output: Some(reg_vn(4, 0, huge)),
            inputs: vec![reg_vn(4, 64, huge)],
            asm_mnemonic: None,
        };
        assert!(
            compiler
                .compile_translation_block(
                    &[GuestInsn {
                        pc: 0x1000,
                        len: 4,
                        ops: vec![copy_op],
                    }],
                    4,
                    2,
                    false,
                    &mut wide_ops,
                )
                .is_err(),
            "oversized Copy ({huge} bytes) must fail cleanly at compile time"
        );

        let load_op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Load,
            address: 0x1000,
            output: Some(reg_vn(4, 0, huge)),
            inputs: vec![const_vn(5, 8), const_vn(0x1000, 8)],
            asm_mnemonic: None,
        };
        assert!(
            compiler
                .compile_translation_block(
                    &[GuestInsn {
                        pc: 0x1000,
                        len: 4,
                        ops: vec![load_op],
                    }],
                    4,
                    2,
                    false,
                    &mut wide_ops,
                )
                .is_err(),
            "oversized Load ({huge} bytes) must fail cleanly"
        );

        let store_op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Store,
            address: 0x1000,
            output: None,
            inputs: vec![const_vn(5, 8), const_vn(0x1000, 8), reg_vn(4, 0, huge)],
            asm_mnemonic: None,
        };
        assert!(
            compiler
                .compile_translation_block(
                    &[GuestInsn {
                        pc: 0x1000,
                        len: 4,
                        ops: vec![store_op],
                    }],
                    4,
                    2,
                    false,
                    &mut wide_ops,
                )
                .is_err(),
            "oversized Store ({huge} bytes) must fail cleanly"
        );
    }

    // CallOther with more than MAX_JIT_CALLOTHER_ARGS operands must fail cleanly.
    let mut callother_inputs = vec![const_vn(1, 4)];
    for i in 0..=MAX_JIT_CALLOTHER_ARGS {
        callother_inputs.push(const_vn(i as i64, 8));
    }
    let callother_op = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::CallOther,
        address: 0x1000,
        output: None,
        inputs: callother_inputs,
        asm_mnemonic: None,
    };
    assert!(
        compiler
            .compile_translation_block(
                &[GuestInsn {
                    pc: 0x1000,
                    len: 4,
                    ops: vec![callother_op],
                }],
                4,
                2,
                false,
                &mut wide_ops,
            )
            .is_err(),
        "oversized CallOther argc must fail compilation"
    );
}

#[test]
fn callbacks_reject_null_unaligned_and_oversized_ffi_inputs() {
    let null_emu: *mut Emulator = std::ptr::null_mut();
    assert_eq!(jit_read_space(null_emu, 0, 0, 8), 0);
    jit_write_space(null_emu, 0, 0, 8, 1);
    assert_eq!(jit_wide_op(null_emu, 0), 1);
    jit_read_bytes(null_emu, 0, 0, std::ptr::null_mut(), 8);
    jit_write_bytes(null_emu, 0, 0, std::ptr::null(), 8);
    jit_count_insn(null_emu, 0x1000, 4);
    jit_observe_block(null_emu, 0x1000);
    jit_observe_insn(null_emu, 0x1000);
    assert_eq!(jit_count_pcode(null_emu), 1);
    assert_eq!(jit_chain(null_emu, 0x1234), 0x1234);
    assert_eq!(jit_exit_tb(null_emu, 0x5678), 0x5678);
    assert_eq!(jit_read_register(null_emu, 0, 8), 0);
    jit_write_register(null_emu, 0, 8, 1);
    assert_eq!(jit_read_memory(null_emu, 0, 8), 0);
    jit_write_memory(null_emu, 0, 8, 1);
    assert_eq!(jit_host_reg_base(null_emu), 0);
    jit_taint_output(null_emu, 0);
    jit_insn_checkpoint(null_emu, 0);
    assert_eq!(jit_call_other(null_emu, 0, std::ptr::null(), 0, 0), 1);
    assert_eq!(jit_callother_result(null_emu), 0);
    assert_eq!(jit_hle_trap(null_emu, 0), 1);

    let mut emu = make_test_emu();
    let ram = emu.state.ram_space();
    let emu_ptr: *mut Emulator = &mut emu;

    // Reject oversized bulk read/write sizes cleanly without creating UB slices.
    let mut small_buf = [0x99u8; 16];
    assert_eq!(jit_count_pcode(emu_ptr), 0);
    jit_read_bytes(
        emu_ptr,
        ram,
        0x1000,
        small_buf.as_mut_ptr(),
        (MAX_JIT_BULK_BYTES + 1) as u64,
    );
    assert_eq!(small_buf, [0x99u8; 16]);
    assert_eq!(jit_count_pcode(emu_ptr), 1);

    let mut emu2 = make_test_emu();
    let ram2 = emu2.state.ram_space();
    let emu2_ptr: *mut Emulator = &mut emu2;
    assert_eq!(jit_count_pcode(emu2_ptr), 0);
    jit_write_bytes(emu2_ptr, ram2, 0x1000, small_buf.as_ptr(), u64::MAX);
    assert_eq!(jit_count_pcode(emu2_ptr), 1);

    // Reject oversized CallOther argc safely.
    assert_eq!(
        jit_call_other(
            emu_ptr,
            0,
            std::ptr::null(),
            (MAX_JIT_CALLOTHER_ARGS + 1) as u64,
            0,
        ),
        1
    );
    assert!(emu.halt_requested);
    emu.halt_requested = false;

    // Unaligned `args_ptr` (1 byte offset into byte array) must not trigger alignment UB.
    let mut unaligned = [0u8; 17];
    unaligned[1..9].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    let unaligned_ptr = unsafe { unaligned.as_ptr().add(1) as *const u64 };
    assert_eq!(jit_call_other(emu_ptr, 0, unaligned_ptr, 1, 8), 0);
}

#[test]
fn jit_cache_null_pointer_rejection_and_flush_clearing() {
    let cache = JitCache::new();
    let sentinel: usize = 0xDEAD_BEE0;
    cache.insert(
        0x4000,
        Arc::new(JitBlock {
            guest_pc: 0x4000,
            host_func_ptr: std::ptr::null(),
            block_size: 4,
            guest_insns: 1,
            next_pc: Some(0x4004),
            pages: vec![0x4000],
            abs_exit_targets: Vec::new(),
        }),
    );
    assert!(cache.is_empty());

    cache.insert(
        0x5000,
        Arc::new(JitBlock {
            guest_pc: 0x5000,
            host_func_ptr: sentinel as *const u8,
            block_size: 8,
            guest_insns: 2,
            next_pc: Some(0x5008),
            pages: vec![0x5000],
            abs_exit_targets: Vec::new(),
        }),
    );
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hard_chain_host(0x5000), Some(sentinel as *const u8));
    cache.flush_all();
    assert!(cache.is_empty());
    assert!(cache.hard_chain_host(0x5000).is_none());
}
