//! Small, deterministic execution cases with expected results independent of
//! the Fission engines.
//!
//! Reference sources pinned by this matrix:
//! - Ghidra 12.0.4 P-code execution and operation behavior under
//!   `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/Emulation/`.
//! - Unicorn 2.1.4 x86 execution examples in
//!   `vendor/unicorn-2.1.4/bindings/python/tests/test_x86.py`.
//! - QEMU 11.0.2 x86 TCG behavior in `vendor/qemu-11.0.2/target/i386/tcg/`.
//!
//! The tests use Fission-owned safe fixtures and fixed expected states. They
//! do not execute or depend on the vendored tools. The P-code evaluator and
//! JIT are both checked against those fixed expectations; the test-only
//! interpreter call does not change runtime engine selection.

use fission_emulator::core::Emulator;
use fission_emulator::interp::InterpExit;
use fission_emulator::jit::compiler::{GuestInsn, JitCompiler};
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::path::PathBuf;

const TEST_PC: u64 = 0x1000;
const TEST_LEN: u32 = 4;
const REGISTER_SPACE: u64 = 4;
const UNIQUE_SPACE: u64 = 2;

fn make_emulator() -> Emulator {
    make_emulator_from("x64_static_printf_malloc.elf")
}

fn make_emulator_from(fixture: &str) -> Emulator {
    // The ELF is a repository-owned, safe test fixture. Its code is not run by
    // these tests; loading it supplies the normal register and stack spaces.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
    let binary = LoadedBinary::from_file(&path).expect("load test ELF");
    let mut state = MachineState::new();
    fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("load ELF image");
    let load_spec = binary.load_spec().expect("ELF load spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("SLEIGH frontend candidates")
        .into_iter()
        .next()
        .expect("SLEIGH frontend");
    let arch = fission_emulator::arch::ArchInfo::from_language_id(
        load_spec.pair.language_id.as_str(),
        Some(&binary),
    )
    .expect("architecture descriptor");
    Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator")
}

fn imm(value: i64, size: u32) -> Varnode {
    Varnode {
        space_id: 0,
        offset: 0,
        size,
        is_constant: true,
        constant_val: value,
    }
}

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn absolute_target(address: u64) -> Varnode {
    Varnode {
        // P-code control-flow destinations in a non-constant space carry a
        // guest address in `offset`.
        space_id: 3,
        offset: address,
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
}

fn op(opcode: PcodeOpcode, output: Option<Varnode>, inputs: Vec<Varnode>) -> PcodeOp {
    PcodeOp {
        seq_num: 0,
        opcode,
        address: TEST_PC,
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn instruction(ops: Vec<PcodeOp>) -> GuestInsn {
    GuestInsn {
        pc: TEST_PC,
        len: TEST_LEN,
        ops,
    }
}

fn run_jit(emu: &mut Emulator, insn: &GuestInsn, check_memory_faults: bool) -> u64 {
    let mut compiler = JitCompiler::new().expect("Cranelift backend");
    let func = compiler
        .compile_translation_block(
            std::slice::from_ref(insn),
            REGISTER_SPACE,
            UNIQUE_SPACE,
            check_memory_faults,
            &mut Vec::new(),
        )
        .expect("compile P-code block");
    assert!(
        compiler.unimplemented_ops.is_empty(),
        "reference case contains unsupported JIT ops: {:?}",
        compiler.unimplemented_ops
    );
    let run: extern "C" fn(*mut Emulator) -> u64 = unsafe { std::mem::transmute(func) };
    let next_pc = run(emu as *mut Emulator);
    assert!(
        compiler.unimplemented_ops.is_empty(),
        "reference case encountered unsupported JIT ops: {:?}",
        compiler.unimplemented_ops
    );
    next_pc
}

fn run_interpreter(emu: &mut Emulator, insn: &GuestInsn) -> InterpExit {
    emu.interpret_translation_block(std::slice::from_ref(insn))
        .expect("interpret P-code block")
}

fn read_value(emu: &mut Emulator, space: u64, offset: u64, size: usize) -> u64 {
    let bytes = emu
        .state
        .read_space(space, offset, size)
        .expect("read state");
    bytes.iter().enumerate().fold(0u64, |value, (i, byte)| {
        value | (u64::from(*byte) << (i * 8))
    })
}

fn fallthrough_pc(exit: InterpExit) -> u64 {
    match exit {
        InterpExit::FallThrough(pc) | InterpExit::Branch(pc) => pc,
        InterpExit::Halt => panic!("unexpected halt in reference case"),
    }
}

#[test]
fn pcode_integer_boundaries_match_fixed_results_in_both_engines() {
    // Expected values follow the P-code bit-vector definitions. These rows
    // cover sign extension, shifts at the operand width, signed division,
    // signed comparison, and carry/overflow outputs.
    let insn = instruction(vec![
        op(
            PcodeOpcode::Copy,
            Some(reg(0x0F0, 4)),
            vec![imm(0x1122_3344_5566_7788, 8)],
        ),
        op(
            PcodeOpcode::IntSExt,
            Some(reg(0x100, 8)),
            vec![imm(0x80, 1)],
        ),
        op(
            PcodeOpcode::IntZExt,
            Some(reg(0x110, 8)),
            vec![imm(0xFF, 1)],
        ),
        op(
            PcodeOpcode::IntLeft,
            Some(reg(0x120, 8)),
            vec![imm(1, 8), imm(64, 8)],
        ),
        op(
            PcodeOpcode::IntRight,
            Some(reg(0x170, 8)),
            vec![imm(-1, 8), imm(64, 8)],
        ),
        op(
            PcodeOpcode::IntSRight,
            Some(reg(0x180, 8)),
            vec![imm(-2, 8), imm(64, 8)],
        ),
        // A one-byte shift-count value of 0x80 is 128, not zero or 0 modulo
        // the host word size. These catch both 7-bit masking and host-masked
        // JIT shift counts.
        op(
            PcodeOpcode::IntLeft,
            Some(reg(0x190, 1)),
            vec![imm(1, 1), imm(128, 1)],
        ),
        op(
            PcodeOpcode::IntRight,
            Some(reg(0x1A0, 1)),
            vec![imm(0xFF, 1), imm(128, 1)],
        ),
        op(
            PcodeOpcode::IntSRight,
            Some(reg(0x1B0, 1)),
            vec![imm(0x80, 1), imm(128, 1)],
        ),
        op(
            PcodeOpcode::IntSDiv,
            Some(reg(0x130, 4)),
            vec![imm(-6, 4), imm(-1, 4)],
        ),
        op(
            PcodeOpcode::IntSLessEqual,
            Some(reg(0x140, 1)),
            vec![imm(-1, 4), imm(0, 4)],
        ),
        op(
            PcodeOpcode::IntCarry,
            Some(reg(0x150, 1)),
            vec![imm(0xFF, 1), imm(1, 1)],
        ),
        op(
            PcodeOpcode::IntSCarry,
            Some(reg(0x160, 1)),
            vec![imm(0x7F, 1), imm(1, 1)],
        ),
    ]);

    let mut jit = make_emulator();
    let jit_pc = run_jit(&mut jit, &insn, false);
    let mut interpreted = make_emulator();
    let interp_pc = fallthrough_pc(run_interpreter(&mut interpreted, &insn));

    assert_eq!(jit_pc, TEST_PC + u64::from(TEST_LEN));
    assert_eq!(interp_pc, jit_pc);
    for (offset, size, expected) in [
        (0x0F0, 4, 0x5566_7788),           // COPY truncates to the output width
        (0x100, 8, 0xFFFF_FFFF_FFFF_FF80), // sext i8(-128) -> i64
        (0x110, 8, 0xFF),                  // zext u8(255) -> u64
        (0x120, 8, 0),                     // 1 << 64 -> 0 in 64 bits
        (0x170, 8, 0),                     // u64::MAX >> 64 -> 0
        (0x180, 8, u64::MAX),              // i64(-2) >> 64 -> -1
        (0x190, 1, 0),                     // u8(1) << 128 -> 0
        (0x1A0, 1, 0),                     // u8(255) >> 128 -> 0
        (0x1B0, 1, 0xFF),                  // i8(-128) >> 128 -> -1
        (0x130, 4, 6),                     // i32(-6) / i32(-1)
        (0x140, 1, 1),                     // i32(-1) <= i32(0)
        (0x150, 1, 1),                     // u8(255) + u8(1) carries
        (0x160, 1, 1),                     // i8(127) + i8(1) overflows
    ] {
        assert_eq!(read_value(&mut jit, REGISTER_SPACE, offset, size), expected);
        assert_eq!(
            read_value(&mut interpreted, REGISTER_SPACE, offset, size),
            expected
        );
    }
}

#[test]
fn absolute_conditional_branch_has_the_expected_target_and_fallthrough() {
    for (condition, expected_pc, expected_r0) in [(1, 0x2000, 0), (0, 0x1004, 0xBAD)] {
        let insn = instruction(vec![
            op(
                PcodeOpcode::CBranch,
                None,
                vec![absolute_target(0x2000), imm(condition, 1)],
            ),
            op(PcodeOpcode::Copy, Some(reg(0, 8)), vec![imm(0xBAD, 8)]),
        ]);

        let mut jit = make_emulator();
        assert_eq!(run_jit(&mut jit, &insn, false), expected_pc);
        let mut interpreted = make_emulator();
        let interp_pc = fallthrough_pc(run_interpreter(&mut interpreted, &insn));
        assert_eq!(interp_pc, expected_pc);
        assert_eq!(read_value(&mut jit, REGISTER_SPACE, 0, 8), expected_r0);
        assert_eq!(
            read_value(&mut interpreted, REGISTER_SPACE, 0, 8),
            expected_r0
        );
    }
}

#[test]
fn partial_width_store_preserves_adjacent_memory_bytes_in_both_engines() {
    let mut jit = make_emulator();
    let mut interpreted = make_emulator();
    let jit_ram = jit.state.ram_space();
    let interp_ram = interpreted.state.ram_space();
    let jit_addr = jit.read_stack_pointer() - 0x40;
    let interp_addr = interpreted.read_stack_pointer() - 0x40;
    let initial = [0x11, 0x22, 0x33, 0x44];
    jit.state
        .write_space(jit_ram, jit_addr, &initial)
        .expect("seed JIT stack bytes");
    interpreted
        .state
        .write_space(interp_ram, interp_addr, &initial)
        .expect("seed interpreter stack bytes");

    let insn = instruction(vec![op(
        PcodeOpcode::Store,
        None,
        vec![
            imm(jit_ram as i64, 4),
            imm(jit_addr as i64, 8),
            imm(0xBEEF, 2),
        ],
    )]);
    let interp_insn = instruction(vec![op(
        PcodeOpcode::Store,
        None,
        vec![
            imm(interp_ram as i64, 4),
            imm(interp_addr as i64, 8),
            imm(0xBEEF, 2),
        ],
    )]);

    assert_eq!(
        run_jit(&mut jit, &insn, true),
        TEST_PC + u64::from(TEST_LEN)
    );
    assert_eq!(
        fallthrough_pc(run_interpreter(&mut interpreted, &interp_insn)),
        TEST_PC + u64::from(TEST_LEN)
    );
    let expected = [0xEF, 0xBE, 0x33, 0x44];
    assert_eq!(
        jit.state.read_space(jit_ram, jit_addr, 4).unwrap(),
        expected
    );
    assert_eq!(
        interpreted
            .state
            .read_space(interp_ram, interp_addr, 4)
            .unwrap(),
        expected
    );
}

#[test]
fn x86_64_fixture_matches_its_expected_branch_and_memory_results() {
    // This repository-owned freestanding fixture reads one byte, compares it
    // with 'A', and exits 0 for 'A' or 1 otherwise. The expected status,
    // RDI value, and byte at the guest stack pointer are independent of the
    // P-code evaluator and JIT implementations. Unicorn's x86 test_x86.py and
    // QEMU 11.0.2's target/i386/tcg/translate.c are the pinned ISA references.
    for (input, expected_status) in [(b'A', 0), (b'B', 1)] {
        for force_interpreter in [false, true] {
            let mut emu = make_emulator_from("x64_concolic_branch_sys.elf");
            emu.force_interpreter = force_interpreter;
            emu.max_inst = Some(1_000);
            emu.seed_stdin(&[input]);
            emu.run().expect("run safe conformance fixture");

            assert_eq!(emu.exit_code, Some(expected_status));
            assert_eq!(
                emu.read_register_u64("RDI").unwrap(),
                u64::from(expected_status)
            );
            let stack_pointer = emu.read_stack_pointer();
            let ram = emu.state.ram_space();
            assert_eq!(
                emu.state.read_space(ram, stack_pointer, 1).unwrap(),
                [input]
            );
        }
    }
}
