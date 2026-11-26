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
    GuestInsn, JitCompiler, JitStackLimits, MAX_JIT_STACK_BUFFER_BYTES, MAX_JIT_TB_FRAME_BYTES,
    MAX_JIT_TB_STACK_SLOT_BYTES, can_direct_host_reg_access,
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
    // SAFETY: a null `emu_ptr` (and null buffers) is inside the callback
    // contract; every callback must return its neutral value without
    // dereferencing anything.
    unsafe {
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
    }

    let mut emu = make_test_emu();
    let ram = emu.state.ram_space();
    let emu_ptr: *mut Emulator = &mut emu;
    // SAFETY: from here on `emu` is reached only through `emu_ptr` (so the
    // pointer stays valid), buffers are live locals, and the oversized sizes
    // are exactly what the callbacks must refuse before forming a slice.
    unsafe {
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
        assert!((*emu_ptr).halt_requested);
        (*emu_ptr).halt_requested = false;

        // Unaligned `args_ptr` (1 byte offset into byte array) must not trigger alignment UB.
        let mut unaligned = [0u8; 17];
        unaligned[1..9].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        let unaligned_ptr = unaligned.as_ptr().add(1) as *const u64;
        assert_eq!(jit_call_other(emu_ptr, 0, unaligned_ptr, 1, 8), 0);
    }
}

#[test]
fn jit_cache_null_pointer_rejection_and_flush_clearing() {
    let cache = JitCache::new();
    let sentinel: usize = 0xDEAD_BEE0;
    // SAFETY: neither pointer is ever called: the null one is refused and the
    // sentinel is only read back and flushed.
    unsafe {
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
    }
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hard_chain_host(0x5000), Some(sentinel as *const u8));
    cache.flush_all();
    assert!(cache.is_empty());
    assert!(cache.hard_chain_host(0x5000).is_none());
}

/// A wide `COPY` between two register-space varnodes of `size` bytes, each at
/// its own offset so no op is dead.
fn wide_copy(seq: u32, out_offset: u64, in_offset: u64, size: u32) -> PcodeOp {
    PcodeOp {
        seq_num: seq,
        opcode: PcodeOpcode::Copy,
        address: 0x1000,
        output: Some(reg_vn(4, out_offset, size)),
        inputs: vec![reg_vn(4, in_offset, size)],
        asm_mnemonic: None,
    }
}

#[test]
fn compiler_rejects_individually_valid_wide_slots_whose_aggregate_exceeds_the_frame_budget() {
    let mut compiler = JitCompiler::new().expect("JITCompiler");
    let mut wide_ops = Vec::new();
    // Every slot passes the per-slot limit; together they ask one host frame
    // for 64 * 4 KiB = 256 KiB of stack.
    let ops: Vec<PcodeOp> = (0..64u32)
        .map(|i| {
            wide_copy(
                i,
                0x10_0000 + u64::from(i) * 0x2000,
                0x20_0000 + u64::from(i) * 0x2000,
                MAX_JIT_STACK_BUFFER_BYTES,
            )
        })
        .collect();
    let err = compiler
        .compile_translation_block(
            &[GuestInsn {
                pc: 0x1000,
                len: 4,
                ops,
            }],
            4,
            2,
            false,
            &mut wide_ops,
        )
        .expect_err("a TB whose stack slots sum past the frame budget must not compile");
    let message = format!("{err:#}");
    assert!(
        message.contains("aggregate"),
        "rejection must name the aggregate budget, got: {message}"
    );
    assert!(
        wide_ops.is_empty(),
        "a rejected TB must not leave side-table entries"
    );
}

#[test]
fn jit_frames_larger_than_a_guard_page_are_stack_probed() {
    use cranelift_codegen::settings::ProbestackStrategy;
    use cranelift_module::Module;

    // Without probes, a frame larger than the guard page can step over it and
    // write below the stack; with inline probes it faults on the guard page.
    let compiler = JitCompiler::new().expect("JITCompiler");
    let flags = compiler.module.isa().flags();
    assert!(
        flags.enable_probestack(),
        "JIT frames must be stack-probed so a large frame cannot skip the guard page"
    );
    assert_eq!(
        flags.probestack_strategy(),
        ProbestackStrategy::Inline,
        "outline probes need a `__cranelift_probestack` symbol the JIT does not provide"
    );
}

mod self_replacing_os {
    use super::*;
    use fission_emulator::os::{BareMetalEnv, HleResult, OsEnvironment};
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static ENV_DROPPED: AtomicBool = AtomicBool::new(false);
    pub static DROPPED_WHILE_DISPATCHING: AtomicBool = AtomicBool::new(false);
    pub static DISPATCHED: AtomicBool = AtomicBool::new(false);

    /// An OS layer whose userop handler swaps the emulator's OS layer out from
    /// under itself -- safe code, and legal through the public `os` field.
    pub struct SelfReplacingEnv;

    impl Drop for SelfReplacingEnv {
        fn drop(&mut self) {
            ENV_DROPPED.store(true, Ordering::SeqCst);
        }
    }

    impl OsEnvironment for SelfReplacingEnv {
        fn patch_imports(
            &self,
            _state: &mut MachineState,
            _binary: &LoadedBinary,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn resolve_stub(&self, _binary: &LoadedBinary, _magic_addr: u64) -> Option<String> {
            None
        }

        fn dispatch_hle(&self, _emu: &mut Emulator, _func_name: &str) -> anyhow::Result<HleResult> {
            Ok(HleResult::Continue)
        }

        fn dispatch_userop(
            &self,
            emu: &mut Emulator,
            _userop_name: &str,
            _input_vals: &[u64],
            _output_size: u32,
        ) -> anyhow::Result<HleResult> {
            DISPATCHED.store(true, Ordering::SeqCst);
            let replacement: Box<dyn OsEnvironment> = Box::new(BareMetalEnv::new());
            emu.os = replacement.into();
            // `self` must still be alive here: the dispatcher is running one of
            // its methods. Only the flag is read, never `self`.
            DROPPED_WHILE_DISPATCHING.store(ENV_DROPPED.load(Ordering::SeqCst), Ordering::SeqCst);
            Ok(HleResult::Continue)
        }
    }
}

#[test]
fn os_layer_replaced_during_its_own_callother_dispatch_outlives_the_call() {
    use self_replacing_os::*;
    use std::sync::atomic::Ordering;

    let mut emu = make_test_emu();
    let userop_id = 0xD15A;
    emu.userop_map
        .insert(userop_id, "replace_os_layer".to_string());
    let env: Box<dyn fission_emulator::os::OsEnvironment> = Box::new(SelfReplacingEnv);
    emu.os = env.into();

    // SAFETY: `emu` is live and not otherwise borrowed for the call; no args.
    let control =
        unsafe { jit_call_other(&mut emu as *mut Emulator, userop_id, std::ptr::null(), 0, 0) };
    assert_eq!(control, 0);
    assert!(DISPATCHED.load(Ordering::SeqCst), "handler never ran");
    assert!(
        !DROPPED_WHILE_DISPATCHING.load(Ordering::SeqCst),
        "the OS layer was freed while one of its methods was still executing"
    );
    assert!(
        ENV_DROPPED.load(Ordering::SeqCst),
        "the replacement must still take effect once dispatch returns"
    );
}

#[test]
fn stack_limits_can_only_be_tightened() {
    let mut compiler = JitCompiler::new().expect("JITCompiler");
    assert_eq!(compiler.stack_limits(), JitStackLimits::default());
    compiler.tighten_stack_limits(JitStackLimits {
        tb_slot_bytes: u32::MAX,
        tb_frame_bytes: u32::MAX,
    });
    assert_eq!(
        compiler.stack_limits(),
        JitStackLimits {
            tb_slot_bytes: MAX_JIT_TB_STACK_SLOT_BYTES,
            tb_frame_bytes: MAX_JIT_TB_FRAME_BYTES,
        }
    );
    compiler.tighten_stack_limits(JitStackLimits {
        tb_slot_bytes: 64,
        tb_frame_bytes: 128,
    });
    assert_eq!(compiler.stack_limits().tb_slot_bytes, 64);
    assert_eq!(compiler.stack_limits().tb_frame_bytes, 128);
}

#[test]
fn aggregate_budget_accounts_alignment_at_the_boundary() {
    // 9-byte slots occupy 16 bytes each once aligned. With a 32-byte budget,
    // two fit (32) and three (48) must not -- though 3 * 9 = 27 would.
    let mut compiler = JitCompiler::new().expect("JITCompiler");
    compiler.tighten_stack_limits(JitStackLimits {
        tb_slot_bytes: 32,
        tb_frame_bytes: MAX_JIT_TB_FRAME_BYTES,
    });
    let mut wide_ops = Vec::new();
    let compile = |compiler: &mut JitCompiler, n: u32, wide_ops: &mut Vec<PcodeOp>| {
        let ops = (0..n)
            .map(|i| wide_copy(i, 0x10_0000 + u64::from(i) * 0x40, 0x20_0000, 9))
            .collect();
        compiler.compile_translation_block(
            &[GuestInsn {
                pc: 0x1000,
                len: 4,
                ops,
            }],
            4,
            2,
            false,
            wide_ops,
        )
    };
    compile(&mut compiler, 2, &mut wide_ops).expect("two aligned 16-byte slots fit 32 bytes");
    let err = compile(&mut compiler, 3, &mut wide_ops).expect_err("48 aligned bytes > 32");
    assert!(format!("{err:#}").contains("aggregate"), "{err:#}");
}

#[test]
fn frame_over_budget_is_refused_before_install_and_leaves_the_compiler_clean() {
    let mut emu = make_test_emu();
    let reg_space = emu.state.register_space();
    let unique_space = emu.state.unique_space();
    let mut compiler = JitCompiler::new().expect("JITCompiler");
    // Taint mode makes emission push the op into `wide_ops`, so a refusal
    // after emission has something to leave behind.
    compiler.shadow = fission_emulator::ShadowMode::Taint;
    compiler.tighten_stack_limits(JitStackLimits {
        tb_slot_bytes: MAX_JIT_TB_STACK_SLOT_BYTES,
        tb_frame_bytes: 256,
    });
    let mut wide_ops = Vec::new();
    // One 1 KiB temporary: passes the slot budget, cannot fit a 256-byte frame.
    let err = compiler
        .compile_translation_block(
            &[GuestInsn {
                pc: 0x3000,
                len: 4,
                ops: vec![wide_copy(0, 0x10_0000, 0x20_0000, 1024)],
            }],
            reg_space,
            unique_space,
            false,
            &mut wide_ops,
        )
        .expect_err("a frame over budget must not be installed");
    assert!(format!("{err:#}").contains("frame budget"), "{err:#}");
    assert!(
        wide_ops.is_empty(),
        "a refused block must not leave entries its code would have indexed"
    );

    // The same compiler builds and runs the next block correctly: no SSA or
    // IR state from the refused one survives into it.
    emu.state
        .write_space(reg_space, 0x40, &0x0123_4567_89AB_CDEFu64.to_le_bytes())
        .expect("seed");
    let host_ptr = compiler
        .compile_translation_block(
            &[GuestInsn {
                pc: 0x3000,
                len: 4,
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x3000,
                    output: Some(reg_vn(reg_space, 0x48, 8)),
                    inputs: vec![reg_vn(reg_space, 0x40, 8)],
                    asm_mnemonic: None,
                }],
            }],
            reg_space,
            unique_space,
            false,
            &mut wide_ops,
        )
        .expect("a small block compiles after a refusal");
    assert!(compiler.last_compiled_frame_bytes() <= 256);
    let f: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(host_ptr) };
    assert_eq!(f(&mut emu as *mut Emulator), 0x3004);
    assert_eq!(
        emu.state.read_space(reg_space, 0x48, 8).expect("read"),
        0x0123_4567_89AB_CDEFu64.to_le_bytes()
    );
}

/// Registers `[0x10_0000, ...)` / `[0x20_0000, ...)` lie outside the host
/// mirror, so these copies exercise the bulk-callback stack-slot path.
fn seeded_wide_block(emu: &mut Emulator, widths: &[u32]) -> Vec<PcodeOp> {
    let reg_space = emu.state.register_space();
    let mut ops = Vec::new();
    for (i, &width) in widths.iter().enumerate() {
        let src = 0x20_0000 + i as u64 * 0x1000;
        let dst = 0x10_0000 + i as u64 * 0x1000;
        let bytes: Vec<u8> = (0..width)
            .map(|b| (b as u8) ^ (i as u8).wrapping_mul(37))
            .collect();
        emu.state
            .write_space(reg_space, src, &bytes)
            .expect("seed source");
        ops.push(PcodeOp {
            seq_num: i as u32,
            opcode: PcodeOpcode::Copy,
            address: 0x4000,
            output: Some(reg_vn(reg_space, dst, width)),
            inputs: vec![reg_vn(reg_space, src, width)],
            asm_mnemonic: None,
        });
    }
    ops
}

#[test]
fn many_wide_ops_within_budget_run_and_agree_with_the_interpreter() {
    // Mixed legitimate widths (XMM/YMM/ZMM and odd sizes) plus enough 1 KiB
    // temporaries to land just under the 16 KiB aggregate budget.
    let mut widths = vec![16u32, 32, 64, 9, 24, 48, 4096];
    let used: u32 = widths.iter().map(|w| w.div_ceil(8) * 8).sum();
    let filler = (MAX_JIT_TB_STACK_SLOT_BYTES - used) / 1024;
    widths.extend(std::iter::repeat_n(1024, filler as usize));
    let total: u32 = widths.iter().map(|w| w.div_ceil(8) * 8).sum();
    assert!(total <= MAX_JIT_TB_STACK_SLOT_BYTES && total + 1024 > MAX_JIT_TB_STACK_SLOT_BYTES);

    let mut jit_emu = make_test_emu();
    let mut interp_emu = make_test_emu();
    let ops = seeded_wide_block(&mut jit_emu, &widths);
    let interp_ops = seeded_wide_block(&mut interp_emu, &widths);
    assert_eq!(ops.len(), interp_ops.len());
    let insn = |ops: Vec<PcodeOp>| GuestInsn {
        pc: 0x4000,
        len: 4,
        ops,
    };

    let mut compiler = JitCompiler::new().expect("JITCompiler");
    let mut wide_ops = Vec::new();
    let host_ptr = compiler
        .compile_translation_block(
            &[insn(ops)],
            jit_emu.state.register_space(),
            jit_emu.state.unique_space(),
            false,
            &mut wide_ops,
        )
        .expect("a block inside the aggregate budget compiles");
    let frame = compiler.last_compiled_frame_bytes();
    assert!(
        frame >= total && frame <= MAX_JIT_TB_FRAME_BYTES,
        "frame {frame} must hold the {total} slot bytes and stay within budget"
    );
    let f: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(host_ptr) };
    assert_eq!(f(&mut jit_emu as *mut Emulator), 0x4004);

    interp_emu
        .interpret_translation_block(&[insn(interp_ops)])
        .expect("interpreter runs the same block");

    let reg_space = jit_emu.state.register_space();
    for (i, &width) in widths.iter().enumerate() {
        let dst = 0x10_0000 + i as u64 * 0x1000;
        let expected: Vec<u8> = (0..width)
            .map(|b| (b as u8) ^ (i as u8).wrapping_mul(37))
            .collect();
        let jit = jit_emu
            .state
            .read_space(reg_space, dst, width as usize)
            .expect("jit dst");
        let interp = interp_emu
            .state
            .read_space(reg_space, dst, width as usize)
            .expect("interp dst");
        assert_eq!(jit, expected, "JIT copy {i} ({width} bytes)");
        assert_eq!(interp, jit, "engines disagree on copy {i} ({width} bytes)");
    }
}

mod run_loop {
    use super::*;
    use fission_emulator::os::BareMetalEnv;
    use fission_emulator::pcode::page_map::prot;

    const CODE_BASE: u64 = 0x1000_0000;
    const STACK_TOP: u64 = 0x2000_0000;

    /// x86-64, with a 16-byte SSE load and store (both stack-slot temporaries):
    ///
    /// ```text
    ///  0: 0f 10 45 00   movups xmm0, [rbp]
    ///  4: 0f 11 45 10   movups [rbp+0x10], xmm0
    ///  8: 83 c0 01      add eax, 1
    ///  b: 83 e9 01      sub ecx, 1
    ///  e: 75 f0         jnz 0
    /// 10: eb fe         jmp 0x10
    /// ```
    const CODE: &[u8] = &[
        0x0F, 0x10, 0x45, 0x00, 0x0F, 0x11, 0x45, 0x10, 0x83, 0xC0, 0x01, 0x83, 0xE9, 0x01, 0x75,
        0xF0, 0xEB, 0xFE,
    ];

    pub fn run(limits: Option<JitStackLimits>) -> (Emulator, Vec<u8>) {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
        let binary = LoadedBinary::from_file(&path).expect("fixture");
        let load_spec = binary.load_spec().expect("load spec").clone();
        let sleigh =
            fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
                &load_spec,
            )
            .expect("frontends")
            .into_iter()
            .next()
            .expect("frontend");
        let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
            .expect("arch");
        let mut state = MachineState::new();
        let ram = state.ram_space();
        state.page_map.map_region(
            CODE_BASE,
            0x1000,
            prot::VALID | prot::READ | prot::EXEC,
            true,
        );
        state.page_map.map_region(
            STACK_TOP - 0x10000,
            0x10000,
            prot::VALID | prot::READ | prot::WRITE,
            true,
        );
        state.write_space(ram, CODE_BASE, CODE).expect("code");
        let rbp = STACK_TOP - 0x1000;
        let pattern: Vec<u8> = (0..16u8).map(|b| b.wrapping_mul(29) ^ 0x5A).collect();
        state.write_space(ram, rbp, &pattern).expect("pattern");

        let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(BareMetalEnv::new()))
            .expect("emulator")
            .with_max_inst(Some(600));
        if let (Some(limits), Some(jit)) = (limits, emu.jit.as_mut()) {
            jit.tighten_stack_limits(limits);
        }
        emu.pc = CODE_BASE;
        emu.write_register_u64("RAX", 0).expect("rax");
        emu.write_register_u64("RCX", 100).expect("rcx");
        emu.write_register_u64("RBP", rbp).expect("rbp");
        emu.write_register_u64("RSP", STACK_TOP - 0x2000)
            .expect("rsp");
        let _ = emu.run();
        (emu, pattern)
    }
}

#[test]
fn blocks_refused_for_their_frame_run_through_the_interpreter_with_the_same_result() {
    let (mut compiled, pattern) = run_loop::run(None);
    let (mut refused, _) = run_loop::run(Some(JitStackLimits {
        tb_slot_bytes: MAX_JIT_TB_STACK_SLOT_BYTES,
        // Smaller than any real frame: every block is refused after layout.
        tb_frame_bytes: 16,
    }));

    assert!(
        compiled.metrics.tbs_compiled > 0,
        "baseline must use the JIT"
    );
    assert_eq!(
        refused.metrics.tbs_compiled, 0,
        "every block must be refused"
    );
    assert!(
        refused.interpreted_blocks > compiled.interpreted_blocks,
        "refused blocks must run through the interpreter, not be skipped"
    );
    assert_eq!(refused.inst_count, compiled.inst_count);
    assert_eq!(refused.pc, compiled.pc);
    for reg in ["RAX", "RCX", "RBP"] {
        assert_eq!(
            refused.read_register_u64(reg).expect("reg"),
            compiled.read_register_u64(reg).expect("reg"),
            "{reg}"
        );
    }
    assert_eq!(compiled.read_register_u64("RAX").expect("rax"), 100);
    let ram = compiled.state.ram_space();
    let rbp = compiled.read_register_u64("RBP").expect("rbp");
    for emu in [&mut compiled, &mut refused] {
        assert_eq!(
            emu.state
                .read_space(ram, rbp + 0x10, 16)
                .expect("stored xmm0"),
            pattern
        );
    }
}
