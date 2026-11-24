//! Float p-code on the sizes real code uses: half-precision (2-byte),
//! single-precision constants, and x87 80-bit registers.
//!
//! A two- or four-byte float constant is the raw bits of an `f16` / `f32`;
//! reading them as the low bits of an `f64` gives zero or a denormal near
//! zero, so `x + 1.0` added nothing. Half-precision inputs were previously
//! read as `0.0` and never written back at all; the x87 store wrote an
//! `f64`'s bits where the 80-bit significand belongs and left the exponent
//! bytes zero, so every value stored then loaded came back as zero.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::{ArchInfo, NanPolicy};
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::eval::Evaluator;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_solver::Solver;

fn slot(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 1,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn constant(value: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 0,
        offset: value,
        size,
        is_constant: true,
        constant_val: value as i64,
    }
}

fn run(state: &mut MachineState, opcode: PcodeOpcode, inputs: &[Varnode], out: &Varnode) {
    let mut solver = Solver::new();
    let mut ev = Evaluator::new(state, &mut solver);
    ev.step(&PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(out.clone()),
        inputs: inputs.to_vec(),
        asm_mnemonic: None,
    })
    .expect("step");
    assert!(ev.unimplemented.is_none(), "{opcode:?} not implemented");
}

#[test]
fn a_single_precision_constant_is_read_as_an_f32() {
    let mut state = MachineState::new();
    let x = slot(0x100, 4);
    let out = slot(0x200, 4);
    state
        .write_space(x.space_id, x.offset, &2.0f32.to_bits().to_le_bytes())
        .unwrap();
    run(
        &mut state,
        PcodeOpcode::FloatAdd,
        &[x, constant(1.0f32.to_bits() as u64, 4)],
        &out,
    );
    let got = state.read_space(out.space_id, out.offset, 4).unwrap();
    assert_eq!(f32::from_le_bytes(got[..4].try_into().unwrap()), 3.0);
}

#[test]
fn an_x87_value_survives_being_stored_and_loaded() {
    let mut state = MachineState::new();
    let (a, b, out) = (slot(0x100, 10), slot(0x120, 10), slot(0x140, 10));
    // 1.5 = 0xC000.. x 2^0 ; 2.25 = 0x9000.. x 2^1, in x87 extended encoding.
    let enc = |frac: u64, se: u16| {
        let mut v = [0u8; 10];
        v[..8].copy_from_slice(&frac.to_le_bytes());
        v[8..].copy_from_slice(&se.to_le_bytes());
        v
    };
    state
        .write_space(a.space_id, a.offset, &enc(0xc000_0000_0000_0000, 0x3fff))
        .unwrap();
    state
        .write_space(b.space_id, b.offset, &enc(0x9000_0000_0000_0000, 0x4000))
        .unwrap();
    run(&mut state, PcodeOpcode::FloatAdd, &[a, b], &out);
    // 3.75 = 0xF000.. x 2^1
    let got = state.read_space(out.space_id, out.offset, 10).unwrap();
    assert_eq!(got, enc(0xf000_0000_0000_0000, 0x4000).to_vec());
}

fn read_u16_slot(state: &mut MachineState, vn: &Varnode) -> u16 {
    let got = state.read_space(vn.space_id, vn.offset, 2).unwrap();
    u16::from_le_bytes(got[..2].try_into().unwrap())
}

fn write_u16_slot(state: &mut MachineState, vn: &Varnode, bits: u16) {
    state
        .write_space(vn.space_id, vn.offset, &bits.to_le_bytes())
        .unwrap();
}

#[test]
fn a_half_precision_constant_and_arithmetic_round_trip() {
    let mut state = MachineState::new();
    let x = slot(0x100, 2);
    let y = slot(0x110, 2);
    let out = slot(0x200, 2);

    // 2.0h + const(1.0h) = 3.0h (0x4000 + 0x3c00 = 0x4200)
    write_u16_slot(&mut state, &x, 0x4000);
    run(
        &mut state,
        PcodeOpcode::FloatAdd,
        &[x.clone(), constant(0x3c00, 2)],
        &out,
    );
    assert_eq!(read_u16_slot(&mut state, &out), 0x4200);

    // 3.0h - 1.5h = 1.5h (0x4200 - 0x3e00 = 0x3e00)
    write_u16_slot(&mut state, &x, 0x4200);
    write_u16_slot(&mut state, &y, 0x3e00);
    run(
        &mut state,
        PcodeOpcode::FloatSub,
        &[x.clone(), y.clone()],
        &out,
    );
    assert_eq!(read_u16_slot(&mut state, &out), 0x3e00);

    // 1.5h * -2.0h = -3.0h (0x3e00 * 0xc000 = 0xc200)
    write_u16_slot(&mut state, &x, 0x3e00);
    write_u16_slot(&mut state, &y, 0xc000);
    run(
        &mut state,
        PcodeOpcode::FloatMult,
        &[x.clone(), y.clone()],
        &out,
    );
    assert_eq!(read_u16_slot(&mut state, &out), 0xc200);

    // 3.0h / 2.0h = 1.5h (0x4200 / 0x4000 = 0x3e00)
    write_u16_slot(&mut state, &x, 0x4200);
    write_u16_slot(&mut state, &y, 0x4000);
    run(&mut state, PcodeOpcode::FloatDiv, &[x.clone(), y], &out);
    assert_eq!(read_u16_slot(&mut state, &out), 0x3e00);

    // sqrt(4.0h) = 2.0h (0x4400 -> 0x4000)
    write_u16_slot(&mut state, &x, 0x4400);
    run(
        &mut state,
        PcodeOpcode::FloatSqrt,
        std::slice::from_ref(&x),
        &out,
    );
    assert_eq!(read_u16_slot(&mut state, &out), 0x4000);

    // neg(2.0h) = -2.0h, abs(-2.0h) = 2.0h
    write_u16_slot(&mut state, &x, 0x4000);
    run(&mut state, PcodeOpcode::FloatNeg, &[x], &out);
    assert_eq!(read_u16_slot(&mut state, &out), 0xc000);
    run(
        &mut state,
        PcodeOpcode::FloatAbs,
        std::slice::from_ref(&out),
        &out,
    );
    assert_eq!(read_u16_slot(&mut state, &out), 0x4000);
}

#[test]
fn half_precision_conversions_and_comparisons() {
    let mut state = MachineState::new();
    let h = slot(0x100, 2);
    let s = slot(0x110, 4);
    let d = slot(0x120, 8);
    let i4 = slot(0x130, 4);
    let b1 = slot(0x140, 1);

    // 2 -> 4 -> 2 and 2 -> 8 -> 2 on 0x3555 (~1/3)
    write_u16_slot(&mut state, &h, 0x3555);
    run(
        &mut state,
        PcodeOpcode::FloatFloat2Float,
        std::slice::from_ref(&h),
        &s,
    );
    assert_eq!(
        u32::from_le_bytes(
            state.read_space(s.space_id, s.offset, 4).unwrap()[..4]
                .try_into()
                .unwrap()
        ),
        0x3eaa_a000
    );
    run(
        &mut state,
        PcodeOpcode::FloatFloat2Float,
        std::slice::from_ref(&s),
        &h,
    );
    assert_eq!(read_u16_slot(&mut state, &h), 0x3555);

    run(
        &mut state,
        PcodeOpcode::FloatFloat2Float,
        std::slice::from_ref(&h),
        &d,
    );
    assert_eq!(
        u64::from_le_bytes(
            state.read_space(d.space_id, d.offset, 8).unwrap()[..8]
                .try_into()
                .unwrap()
        ),
        0x3fd5_5400_0000_0000
    );
    run(&mut state, PcodeOpcode::FloatFloat2Float, &[d], &h);
    assert_eq!(read_u16_slot(&mut state, &h), 0x3555);

    // Int2Float (4 -> 2) and Trunc (2 -> 4)
    state
        .write_space(i4.space_id, i4.offset, &(-7i32).to_le_bytes())
        .unwrap();
    run(
        &mut state,
        PcodeOpcode::FloatInt2Float,
        std::slice::from_ref(&i4),
        &h,
    );
    assert_eq!(read_u16_slot(&mut state, &h), 0xc700);

    write_u16_slot(&mut state, &h, 0xc300); // -3.5
    run(
        &mut state,
        PcodeOpcode::FloatTrunc,
        std::slice::from_ref(&h),
        &i4,
    );
    assert_eq!(
        i32::from_le_bytes(
            state.read_space(i4.space_id, i4.offset, 4).unwrap()[..4]
                .try_into()
                .unwrap()
        ),
        -3
    );

    // Comparisons and NaN predicate on 2-byte varnodes
    let h2 = slot(0x108, 2);
    write_u16_slot(&mut state, &h, 0x3c00); // 1.0
    write_u16_slot(&mut state, &h2, 0x4000); // 2.0
    run(
        &mut state,
        PcodeOpcode::FloatEqual,
        &[h.clone(), h.clone()],
        &b1,
    );
    assert_eq!(state.read_space(b1.space_id, b1.offset, 1).unwrap()[0], 1);
    run(
        &mut state,
        PcodeOpcode::FloatNotEqual,
        &[h.clone(), h2.clone()],
        &b1,
    );
    assert_eq!(state.read_space(b1.space_id, b1.offset, 1).unwrap()[0], 1);
    run(
        &mut state,
        PcodeOpcode::FloatLess,
        &[h.clone(), h2.clone()],
        &b1,
    );
    assert_eq!(state.read_space(b1.space_id, b1.offset, 1).unwrap()[0], 1);
    run(
        &mut state,
        PcodeOpcode::FloatLessEqual,
        &[h2, h.clone()],
        &b1,
    );
    assert_eq!(state.read_space(b1.space_id, b1.offset, 1).unwrap()[0], 0);

    write_u16_slot(&mut state, &h, 0x7e01);
    run(&mut state, PcodeOpcode::FloatNan, &[h], &b1);
    assert_eq!(state.read_space(b1.space_id, b1.offset, 1).unwrap()[0], 1);
}

const AARCH64_CODE: u64 = 0x5000_0000;

fn aarch64_machine(insns: &[u32], interpret: bool) -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/aarch64_control_flow.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(insns.len() as u64));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");

    emu.state
        .page_map
        .map_region(AARCH64_CODE, 0x1000, prot::RX, true);
    let ram = emu.state.ram_space();
    let mut bytes = Vec::with_capacity(0x40);
    for insn in insns {
        bytes.extend_from_slice(&insn.to_le_bytes());
    }
    while bytes.len() < 0x40 {
        bytes.extend_from_slice(&0xd503_201fu32.to_le_bytes()); // nop
    }
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state
        .write_space(ram, AARCH64_CODE, &bytes)
        .expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = AARCH64_CODE;
    emu
}

fn write_reg_bytes(emu: &mut Emulator, name: &str, data: &[u8]) {
    let &(space, offset, size) = emu
        .register_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing register {name}"));
    assert_eq!(data.len(), size as usize, "register {name} size mismatch");
    emu.state.write_space(space, offset, data).unwrap();
}

#[test]
fn aarch64_fp16_scalar_and_indexed_ops_match_hardware_in_both_engines() {
    // 1) fadd h0, h1, h2 (0x1ee22820)
    // 2) fmul h16, h2, v1.h[5] (0x5f119850)
    let code = [0x1ee2_2820u32, 0x5f11_9850u32];
    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = aarch64_machine(&code, interpret);
        // Preload garbage in destinations to verify 2-byte stores overwrite them.
        emu.write_register_u64("h0", 0xBEEF).unwrap();
        emu.write_register_u64("h16", 0xDEAD).unwrap();
        emu.write_register_u64("h1", 0x3134).unwrap();
        emu.write_register_u64("h2", 0x3800).unwrap();
        let mut q1 = [0u8; 16];
        // Note: setting h1 above touched q1[0..2]; preserve lane 0 = 0x3134 and
        // set lane h[5] (bytes 10..12) to 0x3e00 (1.5).
        q1[0..2].copy_from_slice(&0x3134u16.to_le_bytes());
        q1[10..12].copy_from_slice(&0x3e00u16.to_le_bytes());
        write_reg_bytes(&mut emu, "q1", &q1);

        emu.run().expect("run");
        assert_eq!(emu.inst_count, 2, "{engine}: executed both instructions");
        assert_eq!(
            emu.read_register_u64("h0").unwrap(),
            0x394d,
            "{engine}: fadd h0, h1, h2"
        );
        // After fadd, h2 = 0x3800 (0.5) and v1.h[5] = 0x3e00 (1.5) -> 0.75 = 0x3a00.
        assert_eq!(
            emu.read_register_u64("h16").unwrap(),
            0x3a00,
            "{engine}: fmul h16, h2, v1.h[5]"
        );

        // Also check issue #1065's exact h2=0x3134 * v1.h[5]=0x3e00 -> 0x33ce.
        let mut emu2 = aarch64_machine(&[0x5f11_9850u32], interpret);
        emu2.write_register_u64("h16", 0xDEAD).unwrap();
        let mut q1_b = [0u8; 16];
        q1_b[10..12].copy_from_slice(&0x3e00u16.to_le_bytes());
        write_reg_bytes(&mut emu2, "q1", &q1_b);
        emu2.write_register_u64("h2", 0x3134).unwrap();
        emu2.run().expect("run");
        assert_eq!(
            emu2.read_register_u64("h16").unwrap(),
            0x33ce,
            "{engine}: fmul h16, h2(0x3134), v1.h[5](0x3e00)"
        );
    }
}

#[test]
fn aarch64_fp16_vector_fdiv_and_conversions_match_hardware_in_both_engines() {
    // 1) fdiv v0.4h, v1.4h, v2.4h (0x2e423c20)
    // 2) fcvt s1, h0 (0x1ee24001)  [h0 is lane 0 of v0 = 0x3800 (0.5)]
    // 3) fcvt d3, h3 (0x1ee2c063)
    // 4) fcvt h4, d3 (0x1e63c064)
    // 5) scvtf h5, w0 (0x1ee20005)
    // 6) fcvtzs w2, h5 (0x1ef800a2)
    let code = [
        0x2e42_3c20u32,
        0x1ee2_4001u32,
        0x1ee2_c063u32,
        0x1e63_c064u32,
        0x1ee2_0005u32,
        0x1ef8_00a2u32,
    ];
    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = aarch64_machine(&code, interpret);
        emu.write_register_u64("d0", 0xDEAD_BEEF_DEAD_BEEF).unwrap();
        emu.write_register_u64("d1", 0x4400_4200_4000_3c00).unwrap(); // [1.0, 2.0, 3.0, 4.0]
        emu.write_register_u64("d2", 0x4000_3c00_4000_4000).unwrap(); // [2.0, 2.0, 1.0, 2.0]
        emu.write_register_u64("h3", 0x3555).unwrap(); // ~1/3
        emu.write_register_u64("w0", (-7i32) as u32 as u64).unwrap();

        emu.run().expect("run");
        assert_eq!(emu.inst_count, 6, "{engine}: ran all 6 instructions");
        assert_eq!(
            emu.read_register_u64("d0").unwrap(),
            0x4000_4200_3c00_3800,
            "{engine}: fdiv v0.4h, v1.4h, v2.4h"
        );
        assert_eq!(
            emu.read_register_u64("s1").unwrap(),
            0.5f32.to_bits() as u64,
            "{engine}: fcvt s1, h0"
        );
        assert_eq!(
            emu.read_register_u64("d3").unwrap(),
            0x3fd5_5400_0000_0000,
            "{engine}: fcvt d3, h3"
        );
        assert_eq!(
            emu.read_register_u64("h4").unwrap(),
            0x3555,
            "{engine}: fcvt h4, d3"
        );
        assert_eq!(
            emu.read_register_u64("h5").unwrap(),
            0xc700,
            "{engine}: scvtf h5, w0(-7)"
        );
        assert_eq!(
            emu.read_register_u64("w2").unwrap(),
            (-7i32) as u32 as u64,
            "{engine}: fcvtzs w2, h5"
        );
    }
}

fn run_with_policy(
    state: &mut MachineState,
    policy: NanPolicy,
    opcode: PcodeOpcode,
    inputs: &[Varnode],
    out: &Varnode,
) {
    let mut solver = Solver::new();
    let mut ev = Evaluator::new(state, &mut solver).with_nan_policy(policy);
    ev.step(&PcodeOp {
        seq_num: 0,
        opcode,
        address: 0x1000,
        output: Some(out.clone()),
        inputs: inputs.to_vec(),
        asm_mnemonic: None,
    })
    .expect("step");
    assert!(ev.unimplemented.is_none(), "{opcode:?} not implemented");
}

#[test]
fn binary_float_nan_propagation_respects_nan_policy() {
    let mut state = MachineState::new();
    let (a4, b4, out4) = (slot(0x100, 4), slot(0x110, 4), slot(0x120, 4));
    let (a8, b8, out8) = (slot(0x200, 8), slot(0x210, 8), slot(0x220, 8));

    let q1 = 0xffff_ff64u32;
    let q2 = 0xffff_ffffu32;
    let s2 = 0xffac_ffc4u32;
    let q_arm = 0xffc0_0001u32;

    for opcode in [
        PcodeOpcode::FloatAdd,
        PcodeOpcode::FloatSub,
        PcodeOpcode::FloatMult,
        PcodeOpcode::FloatDiv,
    ] {
        // FirstOperand: first NaN wins (q1 over q2, and q1 over s2).
        state
            .write_space(a4.space_id, a4.offset, &q1.to_le_bytes())
            .unwrap();
        state
            .write_space(b4.space_id, b4.offset, &q2.to_le_bytes())
            .unwrap();
        run_with_policy(
            &mut state,
            NanPolicy::FirstOperand,
            opcode,
            &[a4.clone(), b4.clone()],
            &out4,
        );
        assert_eq!(
            u32::from_le_bytes(
                state.read_space(out4.space_id, out4.offset, 4).unwrap()[..4]
                    .try_into()
                    .unwrap()
            ),
            q1,
            "FirstOperand QNaN1+QNaN2 ({opcode:?})"
        );

        state
            .write_space(a4.space_id, a4.offset, &q_arm.to_le_bytes())
            .unwrap();
        state
            .write_space(b4.space_id, b4.offset, &s2.to_le_bytes())
            .unwrap();
        run_with_policy(
            &mut state,
            NanPolicy::FirstOperand,
            opcode,
            &[a4.clone(), b4.clone()],
            &out4,
        );
        assert_eq!(
            u32::from_le_bytes(
                state.read_space(out4.space_id, out4.offset, 4).unwrap()[..4]
                    .try_into()
                    .unwrap()
            ),
            q_arm,
            "FirstOperand QNaN+SNaN ({opcode:?})"
        );

        // SignalingFirst: sNaN takes priority over qNaN and is quieted.
        run_with_policy(
            &mut state,
            NanPolicy::SignalingFirst,
            opcode,
            &[a4.clone(), b4.clone()],
            &out4,
        );
        assert_eq!(
            u32::from_le_bytes(
                state.read_space(out4.space_id, out4.offset, 4).unwrap()[..4]
                    .try_into()
                    .unwrap()
            ),
            s2 | 0x0040_0000,
            "SignalingFirst QNaN+SNaN ({opcode:?})"
        );
    }

    // 8-byte double NaN propagation
    let dq1 = 0x7ff8_0000_0000_0011u64;
    let ds2 = 0x7ff0_0000_0000_0033u64;
    state
        .write_space(a8.space_id, a8.offset, &dq1.to_le_bytes())
        .unwrap();
    state
        .write_space(b8.space_id, b8.offset, &ds2.to_le_bytes())
        .unwrap();
    run_with_policy(
        &mut state,
        NanPolicy::FirstOperand,
        PcodeOpcode::FloatSub,
        &[a8.clone(), b8.clone()],
        &out8,
    );
    assert_eq!(
        u64::from_le_bytes(
            state.read_space(out8.space_id, out8.offset, 8).unwrap()[..8]
                .try_into()
                .unwrap()
        ),
        dq1
    );
    run_with_policy(
        &mut state,
        NanPolicy::SignalingFirst,
        PcodeOpcode::FloatSub,
        &[a8, b8],
        &out8,
    );
    assert_eq!(
        u64::from_le_bytes(
            state.read_space(out8.space_id, out8.offset, 8).unwrap()[..8]
                .try_into()
                .unwrap()
        ),
        ds2 | 0x0008_0000_0000_0000
    );
}

const TEST_CODE: u64 = 0x5000_0000;

fn setup_machine(
    fixture: &str,
    code: &[u8],
    pad_word: &[u8],
    count: u64,
    interpret: bool,
) -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(count));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");

    emu.state
        .page_map
        .map_region(TEST_CODE, 0x1000, prot::RX, true);
    let ram = emu.state.ram_space();
    let mut bytes = code.to_vec();
    while bytes.len() < 0x40 {
        bytes.extend_from_slice(pad_word);
    }
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, TEST_CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = TEST_CODE;
    emu
}

fn write_xmm_lanes(emu: &mut Emulator, name: &str, lanes: [u32; 4]) {
    let &(space, offset, size) = emu
        .register_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing register {name}"));
    assert_eq!(size, 16);
    let mut bytes = [0u8; 16];
    for (i, lane) in lanes.into_iter().enumerate() {
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&lane.to_le_bytes());
    }
    emu.state.write_space(space, offset, &bytes).unwrap();
}

fn read_xmm_lanes(emu: &mut Emulator, name: &str) -> [u32; 4] {
    let &(space, offset, size) = emu
        .register_map
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing register {name}"));
    assert_eq!(size, 16);
    let bytes = emu.state.read_space(space, offset, 16).unwrap();
    [
        u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
    ]
}

#[test]
fn x86_64_float_nan_propagation_matches_intel_sdm_in_both_engines() {
    // Issue #1055 repro 1: addps xmm0, xmm1 (0f 58 c1)
    // Lane 0: (QNaN1, QNaN2) -> QNaN1 (0xffff_ff64)
    // Lane 1: (QNaN, SNaN)   -> QNaN  (0xffff_ff64, NOT SNaN!)
    // Lane 2: (SNaN, QNaN)   -> quiet(SNaN)
    // Lane 3: (finite, SNaN) -> quiet(SNaN)
    let addps = [0x0f, 0x58, 0xc1];
    let xmm0_lanes = [0xffff_ff64, 0xffff_ff64, 0x7fa0_0001, 1.0f32.to_bits()];
    let xmm1_lanes = [0xffff_ffff, 0xffac_ffc4, 0xffff_ff23, 0xffac_ffc4];
    let addps_expected = [0xffff_ff64, 0xffff_ff64, 0x7fe0_0001, 0xffec_ffc4];

    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = setup_machine(
            "x64_static_printf_malloc.elf",
            &addps,
            &[0x90],
            1,
            interpret,
        );
        write_xmm_lanes(&mut emu, "XMM0", xmm0_lanes);
        write_xmm_lanes(&mut emu, "XMM1", xmm1_lanes);
        emu.run().expect("run");
        assert_eq!(
            read_xmm_lanes(&mut emu, "XMM0"),
            addps_expected,
            "{engine}: addps xmm0, xmm1"
        );
    }

    // Issue #1055 repro 2: addsubps xmm0, xmm1 (f2 0f d0 c1)
    let addsubps = [0xf2, 0x0f, 0xd0, 0xc1];
    let as_xmm0 = [0xffff_ffff, 0xffff_ff64, 0x7fa0_0005, 0xffff_ff01];
    let as_xmm1 = [0xffff_ff23, 0xffff_ff99, 0xffff_ff10, 0x7fa0_0007];
    let as_expected = [0xffff_ffff, 0xffff_ff64, 0x7fe0_0005, 0xffff_ff01];
    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = setup_machine(
            "x64_static_printf_malloc.elf",
            &addsubps,
            &[0x90],
            1,
            interpret,
        );
        write_xmm_lanes(&mut emu, "XMM0", as_xmm0);
        write_xmm_lanes(&mut emu, "XMM1", as_xmm1);
        emu.run().expect("run");
        assert_eq!(
            read_xmm_lanes(&mut emu, "XMM0"),
            as_expected,
            "{engine}: addsubps xmm0, xmm1"
        );
    }

    // Scalar double addsd xmm0, xmm1 (f2 0f 58 c1) and subsd xmm0, xmm1 (f2 0f 5c c1)
    let addsd = [0xf2, 0x0f, 0x58, 0xc1];
    let subsd = [0xf2, 0x0f, 0x5c, 0xc1];
    let dq1 = 0x7ff8_0000_0000_0011u64;
    let dq2 = 0x7ff8_0000_0000_0022u64;
    let ds2 = 0x7ff0_0000_0000_0033u64;
    for (insn, b_val, name) in [(&addsd, dq2, "addsd"), (&subsd, ds2, "subsd")] {
        for interpret in [false, true] {
            let engine = if interpret { "interpreter" } else { "JIT" };
            let mut emu =
                setup_machine("x64_static_printf_malloc.elf", insn, &[0x90], 1, interpret);
            emu.write_register_u64("XMM0_Qa", dq1).unwrap();
            emu.write_register_u64("XMM1_Qa", b_val).unwrap();
            emu.run().expect("run");
            assert_eq!(
                emu.read_register_u64("XMM0_Qa").unwrap(),
                dq1,
                "{engine}: {name}"
            );
        }
    }
}

#[test]
fn aarch64_float_nan_propagation_matches_arm_fpprocessnans_in_both_engines() {
    let nop = 0xd503_201fu32.to_le_bytes();
    // 1) fsub s0, s1, s2 (0x1e223820): qNaN 0xffc00001 - sNaN 0xffacffc4 -> quieted sNaN 0xffecffc4
    // 2) fadd s3, s4, s5 (0x1e252883): qNaN 0xffffff64 + qNaN 0xffffffff -> first qNaN 0xffffff64
    // 3) fsub d6, d7, d8 (0x1e6838e6): qNaN64 + sNaN64 -> quieted sNaN64
    let mut code = Vec::new();
    for insn in [0x1e22_3820u32, 0x1e25_2883u32, 0x1e68_38e6u32] {
        code.extend_from_slice(&insn.to_le_bytes());
    }

    let dq1 = 0x7ff8_0000_0000_0011u64;
    let ds2 = 0x7ff0_0000_0000_0033u64;

    for interpret in [false, true] {
        let engine = if interpret { "interpreter" } else { "JIT" };
        let mut emu = setup_machine("aarch64_control_flow.elf", &code, &nop, 3, interpret);
        emu.write_register_u64("s1", 0xffc0_0001).unwrap();
        emu.write_register_u64("s2", 0xffac_ffc4).unwrap();
        emu.write_register_u64("s4", 0xffff_ff64).unwrap();
        emu.write_register_u64("s5", 0xffff_ffff).unwrap();
        emu.write_register_u64("d7", dq1).unwrap();
        emu.write_register_u64("d8", ds2).unwrap();

        emu.run().expect("run");
        assert_eq!(emu.inst_count, 3, "{engine}: ran all 3 instructions");
        assert_eq!(
            emu.read_register_u64("s0").unwrap(),
            0xffec_ffc4,
            "{engine}: fsub s0, s1, s2"
        );
        assert_eq!(
            emu.read_register_u64("s3").unwrap(),
            0xffff_ff64,
            "{engine}: fadd s3, s4, s5"
        );
        assert_eq!(
            emu.read_register_u64("d6").unwrap(),
            ds2 | 0x0008_0000_0000_0000,
            "{engine}: fsub d6, d7, d8"
        );
    }
}

/// Half precision follows the guest's NaN policy in the interpreter too, the
/// way the JIT's float callout does, rather than the host CPU's own rule.
///
/// Both policies are checked so the test fails on either host: an x86 host
/// propagates the first operand, an Arm host the signaling one, and only one
/// of them is the guest's. Negation and absolute value only touch the sign
/// bit, so a signaling NaN stays signaling (as `FNEG`/`FABS` leave it).
#[test]
fn half_precision_nan_propagation_follows_the_policy_in_the_interpreter() {
    let mut state = MachineState::new();
    let (a, b, out) = (slot(0x100, 2), slot(0x110, 2), slot(0x120, 2));
    let qnan = 0x7e01u16; // quiet, payload 1
    let snan = 0xfd02u16; // signaling, negative, payload 0x102
    write_u16_slot(&mut state, &a, qnan);
    write_u16_slot(&mut state, &b, snan);
    for opcode in [
        PcodeOpcode::FloatAdd,
        PcodeOpcode::FloatSub,
        PcodeOpcode::FloatMult,
        PcodeOpcode::FloatDiv,
    ] {
        run_with_policy(
            &mut state,
            NanPolicy::FirstOperand,
            opcode,
            &[a.clone(), b.clone()],
            &out,
        );
        assert_eq!(
            read_u16_slot(&mut state, &out),
            qnan,
            "FirstOperand {opcode:?}"
        );
        run_with_policy(
            &mut state,
            NanPolicy::SignalingFirst,
            opcode,
            &[a.clone(), b.clone()],
            &out,
        );
        assert_eq!(
            read_u16_slot(&mut state, &out),
            snan | 0x0200,
            "SignalingFirst {opcode:?}"
        );
    }

    let snan_pos = 0x7d01u16;
    write_u16_slot(&mut state, &a, snan_pos);
    for (opcode, want) in [
        (PcodeOpcode::FloatNeg, 0xfd01u16),
        (PcodeOpcode::FloatAbs, 0x7d01u16),
    ] {
        run_with_policy(
            &mut state,
            NanPolicy::SignalingFirst,
            opcode,
            std::slice::from_ref(&a),
            &out,
        );
        assert_eq!(read_u16_slot(&mut state, &out), want, "{opcode:?}");
    }
}
