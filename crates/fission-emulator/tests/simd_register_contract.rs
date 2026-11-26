//! End-to-end SIMD register-view, upper-bit, alias, and shape-contract tests (#1242).

use std::collections::BTreeMap;
use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::arch::vector::{
    UpperBitsPolicy, VectorContractError, VectorIsa, VectorRegisterBank, answer_vector_userop,
    apply_vector_write, describe_vector_op,
};
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode, Varnode};
use fission_sleigh::runtime::RuntimeSleighFrontend;

const CODE: u64 = 0x5000_0000;
const DATA: u64 = 0x5001_0000;

fn x64_machine(code: &[u8], count: u64, interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
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

    emu.state.page_map.map_region(CODE, 0x1000, prot::RX, true);
    emu.state.page_map.map_region(DATA, 0x1000, prot::RW, true);
    let ram = emu.state.ram_space();
    let mut bytes = code.to_vec();
    bytes.resize(0x100, 0x90);
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = CODE;
    emu
}

fn aarch64_machine(code: &[u8], count: u64, interpret: bool) -> Emulator {
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
        .with_max_inst(Some(count));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");

    emu.state.page_map.map_region(CODE, 0x1000, prot::RX, true);
    emu.state.page_map.map_region(DATA, 0x1000, prot::RW, true);
    let ram = emu.state.ram_space();
    let mut bytes = code.to_vec();
    while bytes.len() < 0x100 {
        bytes.extend_from_slice(&0xD503_201Fu32.to_le_bytes());
    }
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = CODE;
    emu
}

fn lookup_reg(emu: &Emulator, name: &str) -> Option<(u64, u64, u32)> {
    emu.register_map.get(name).copied().or_else(|| {
        emu.register_map
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, &v)| v)
    })
}

fn write_reg(emu: &mut Emulator, name: &str, data: &[u8]) {
    let (sp, off, size) = lookup_reg(emu, name).unwrap_or_else(|| panic!("missing {name}"));
    assert!(data.len() <= size as usize);
    emu.state.write_space(sp, off, data).expect("write reg");
}

fn read_reg(emu: &mut Emulator, name: &str, len: usize) -> Vec<u8> {
    let (sp, off, size) = lookup_reg(emu, name).unwrap_or_else(|| panic!("missing {name}"));
    assert!(len <= size as usize);
    emu.state.read_space(sp, off, len).expect("read reg")
}

fn find_userop_id(map: &BTreeMap<u32, String>, name: &str) -> u32 {
    *map.iter()
        .find(|(_, v)| v.as_str() == name)
        .unwrap_or_else(|| panic!("missing userop {name}"))
        .0
}

#[test]
fn aarch64_narrow_and_aliased_neon_views_zero_upper_q_bits_in_both_engines() {
    // 0: ext v0.8b, v0.8b, v0.8b, #3     -> 0x2E00_1800  (64-bit Q=0 self-alias, imm4=3: [14:11]=3)
    // 1: umaxp v1.4h, v1.4h, v2.4h       -> 0x2E62_A421  (64-bit pairwise self-alias Rd=Rn=v1, Rm=v2)
    // 2: rev32 v3.8b, v3.8b              -> 0x2E20_0863  (64-bit rev32 self-alias Rd=Rn=v3)
    // 3: fminnm s4, s4, s5               -> 0x1E25_7884  (32-bit scalar self-alias Rd=Rn=s4, Rm=s5)
    // 4: uaddlv h6, v7.8b                -> 0x2E30_38E6  (16-bit scalar h6 output)
    // 5: sqxtn v8.4h, v9.4s              -> 0x0E61_4928  (64-bit narrowing zeroes upper 64b of q8)
    // 6: sqxtn2 v8.8h, v10.4s            -> 0x4E61_4948  (128-bit part-2 preserves q8[63:0])
    let words: [u32; 7] = [
        0x2E00_1800,
        0x2E62_A421,
        0x2E20_0863,
        0x1E25_7884,
        0x2E30_38E6,
        0x0E61_4928,
        0x4E61_4948,
    ];
    let mut code = Vec::new();
    for w in words {
        code.extend_from_slice(&w.to_le_bytes());
    }

    let mut q_inputs = [[0u8; 16]; 11];
    // q0: low 8 bytes = [1..=8], high 8 bytes poisoned nonzero!
    q_inputs[0] = [
        1, 2, 3, 4, 5, 6, 7, 8, 0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE,
    ];
    // q1, q2: 4H lanes + poisoned upper 64 bits
    q_inputs[1][0..8].copy_from_slice(
        &[10u16, 40, 30, 20]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    q_inputs[1][8..16].copy_from_slice(&[0xFF; 8]);
    q_inputs[2][0..8].copy_from_slice(
        &[55u16, 5, 99, 100]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    q_inputs[2][8..16].copy_from_slice(&[0xEE; 8]);
    // q3: [1,2,3,4,5,6,7,8] + poisoned upper
    q_inputs[3] = [
        1, 2, 3, 4, 5, 6, 7, 8, 0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44,
    ];
    // q4/q5: s4=qNaN, s5=3.5f32 -> fminnm picks 3.5f32 and zeroes upper 12B of q4
    q_inputs[4] = [0xAA; 16];
    q_inputs[4][0..4].copy_from_slice(&0x7FC0_0001u32.to_le_bytes());
    q_inputs[5][0..4].copy_from_slice(&3.5f32.to_le_bytes());
    // q6/q7: uaddlv h6, v7.8b ([10, 20, 30, 40, 50, 60, 70, 80] => sum=360)
    q_inputs[6] = [0x99; 16];
    q_inputs[7][0..8].copy_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
    // q8 (pre-poisoned), q9 (part-1 4x i32), q10 (part-2 4x i32)
    q_inputs[8] = [0x77; 16];
    for (i, val) in [-50000i32, 40000, -100, 100].into_iter().enumerate() {
        q_inputs[9][i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
    }
    for (i, val) in [1i32, -2, 300000, -300000].into_iter().enumerate() {
        q_inputs[10][i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
    }

    let mut jit_results = Vec::new();
    for interpret in [false, true] {
        let mut emu = aarch64_machine(&code, words.len() as u64, interpret);
        for (idx, bytes) in q_inputs.iter().enumerate() {
            write_reg(&mut emu, &format!("q{idx}"), bytes);
        }
        emu.run().expect("run aarch64 neon contract sequence");

        let q0 = read_reg(&mut emu, "q0", 16);
        assert_eq!(q0, vec![4, 5, 6, 7, 8, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0]);

        let q1 = read_reg(&mut emu, "q1", 16);
        let mut exp_q1 = [0u8; 16];
        exp_q1[0..8].copy_from_slice(
            &[40u16, 30, 55, 100]
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        assert_eq!(q1, exp_q1);

        let q3 = read_reg(&mut emu, "q3", 16);
        assert_eq!(q3, vec![4, 3, 2, 1, 8, 7, 6, 5, 0, 0, 0, 0, 0, 0, 0, 0]);

        let q4 = read_reg(&mut emu, "q4", 16);
        let mut exp_q4 = [0u8; 16];
        exp_q4[0..4].copy_from_slice(&3.5f32.to_le_bytes());
        assert_eq!(q4, exp_q4);

        let q6 = read_reg(&mut emu, "q6", 16);
        let mut exp_q6 = [0u8; 16];
        exp_q6[0..2].copy_from_slice(&360u16.to_le_bytes());
        assert_eq!(q6, exp_q6);

        let q8 = read_reg(&mut emu, "q8", 16);
        let exp_q8: Vec<u8> = [i16::MIN, i16::MAX, -100, 100, 1, -2, i16::MAX, i16::MIN]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        assert_eq!(q8, exp_q8);

        let snapshot = vec![q0, q1, q3, q4, q6, q8];
        if !interpret {
            jit_results = snapshot;
        } else {
            assert_eq!(snapshot, jit_results, "JIT vs interpreter mismatch");
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn aarch64_native_host_matches_full_128bit_neon_views() {
    // 1. ext v0.8b, v0.8b, v0.8b, #3 (must zero upper 64 bits despite poisoned v0[127:64])
    // 2. sqxtn v0.4h, v1.4s -> sqxtn2 v0.8h, v2.4s (part1+part2 sequence)
    let in_ext: [u8; 16] = [
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA,
        0xBE,
    ];
    let mut native_ext_out = [0u8; 16];
    unsafe {
        core::arch::asm!(
            "ldr q0, [{inp}]",
            "ext v0.8b, v0.8b, v0.8b, #3",
            "str q0, [{outp}]",
            inp = in(reg) in_ext.as_ptr(),
            outp = in(reg) native_ext_out.as_mut_ptr(),
            out("v0") _,
            options(nostack),
        );
    }

    let ext_code = 0x2E00_1800u32.to_le_bytes();
    for interpret in [false, true] {
        let mut emu = aarch64_machine(&ext_code, 1, interpret);
        write_reg(&mut emu, "q0", &in_ext);
        emu.run().expect("run ext");
        let got = read_reg(&mut emu, "q0", 16);
        assert_eq!(
            got.as_slice(),
            &native_ext_out,
            "native AArch64 host disagrees on full 128b q0 (interpret={interpret})"
        );
    }

    // Native part1 + part2 narrowing with pre-poisoned q0
    let mut in_v0 = [0xFFu8; 16];
    in_v0[0..8].copy_from_slice(&[0x55; 8]);
    let mut in_v1 = [0u8; 16];
    let mut in_v2 = [0u8; 16];
    for (i, v) in [-40_000i32, 40_000, -17, 19].into_iter().enumerate() {
        in_v1[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in [12i32, -34, 50_000, -50_000].into_iter().enumerate() {
        in_v2[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
    }
    let mut native_xtn_out = [0u8; 16];
    unsafe {
        core::arch::asm!(
            "ldr q0, [{v0}]",
            "ldr q1, [{v1}]",
            "ldr q2, [{v2}]",
            "sqxtn v0.4h, v1.4s",
            "sqxtn2 v0.8h, v2.4s",
            "str q0, [{outp}]",
            v0 = in(reg) in_v0.as_ptr(),
            v1 = in(reg) in_v1.as_ptr(),
            v2 = in(reg) in_v2.as_ptr(),
            outp = in(reg) native_xtn_out.as_mut_ptr(),
            out("v0") _,
            out("v1") _,
            out("v2") _,
            options(nostack),
        );
    }
    let mut xtn_code = Vec::new();
    // sqxtn v0.4h, v1.4s = 0x0E61_4820; sqxtn2 v0.8h, v2.4s = 0x4E61_4840
    xtn_code.extend_from_slice(&0x0E61_4820u32.to_le_bytes());
    xtn_code.extend_from_slice(&0x4E61_4840u32.to_le_bytes());
    for interpret in [false, true] {
        let mut emu = aarch64_machine(&xtn_code, 2, interpret);
        write_reg(&mut emu, "q0", &in_v0);
        write_reg(&mut emu, "q1", &in_v1);
        write_reg(&mut emu, "q2", &in_v2);
        emu.run().expect("run sqxtn+sqxtn2");
        let got = read_reg(&mut emu, "q0", 16);
        assert_eq!(
            got.as_slice(),
            &native_xtn_out,
            "native AArch64 host disagrees on sqxtn/sqxtn2 (interpret={interpret})"
        );
    }
}

#[test]
fn x86_64_legacy_sse_preserves_enclosing_ymm_and_scalar_upper_lanes() {
    // 0: 66 0f 3a 0b c0 01    roundsd xmm0, xmm0, 1  (floor f64[0], preserve XMM0[127:64] & YMM0[255:128])
    // 1: 66 0f 3a 0a c9 02    roundss xmm1, xmm1, 2  (ceil  f32[0], preserve XMM1[127:32] & YMM1[255:128])
    // 2: 66 0f 38 00 d2       pshufb  xmm2, xmm2     (self-alias byte shuffle)
    // 3: 66 0f 3a 0e dc 55    pblendw xmm3, xmm4, 0x55
    let code = [
        0x66, 0x0F, 0x3A, 0x0B, 0xC0, 0x01, 0x66, 0x0F, 0x3A, 0x0A, 0xC9, 0x02, 0x66, 0x0F, 0x38,
        0x00, 0xD2, 0x66, 0x0F, 0x3A, 0x0E, 0xDC, 0x55,
    ];

    for interpret in [false, true] {
        let mut emu = x64_machine(&code, 4, interpret);
        let has_ymm0 = lookup_reg(&emu, "YMM0").is_some();

        let mut ymm0 = [0xA5u8; 32];
        ymm0[0..8].copy_from_slice(&(-1.25f64).to_le_bytes());
        ymm0[8..16].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());

        let mut ymm1 = [0x5Au8; 32];
        ymm1[0..4].copy_from_slice(&1.25f32.to_le_bytes());
        ymm1[4..16].copy_from_slice(&[0xCC; 12]);

        let mut xmm2 = [0u8; 16];
        for i in 0..16 {
            xmm2[i] = ((15 - i) as u8) & 0x0F;
        }
        let mut xmm3 = [0x11u8; 16];
        let mut xmm4 = [0x22u8; 16];
        xmm3[0..2].copy_from_slice(&0x1111u16.to_le_bytes());
        xmm4[0..2].copy_from_slice(&0x2222u16.to_le_bytes());

        if has_ymm0 {
            write_reg(&mut emu, "YMM0", &ymm0);
            write_reg(&mut emu, "YMM1", &ymm1);
        } else {
            write_reg(&mut emu, "XMM0", &ymm0[..16]);
            write_reg(&mut emu, "XMM1", &ymm1[..16]);
        }
        write_reg(&mut emu, "XMM2", &xmm2);
        write_reg(&mut emu, "XMM3", &xmm3);
        write_reg(&mut emu, "XMM4", &xmm4);

        emu.run().expect("run x86 legacy sse sequence");

        let out_xmm0 = read_reg(&mut emu, "XMM0", 16);
        assert_eq!(&out_xmm0[0..8], &(-2.0f64).to_le_bytes());
        assert_eq!(
            &out_xmm0[8..16],
            &0x1122_3344_5566_7788u64.to_le_bytes(),
            "roundsd corrupted XMM0[127:64]"
        );

        let out_xmm1 = read_reg(&mut emu, "XMM1", 16);
        assert_eq!(&out_xmm1[0..4], &2.0f32.to_le_bytes());
        assert_eq!(
            &out_xmm1[4..16],
            &[0xCC; 12],
            "roundss corrupted XMM1[127:32]"
        );

        let out_xmm2 = read_reg(&mut emu, "XMM2", 16);
        let expected_xmm2: Vec<u8> = (0..16).map(|i| xmm2[(xmm2[i] & 0x0F) as usize]).collect();
        assert_eq!(out_xmm2, expected_xmm2);

        if has_ymm0 {
            let out_ymm0 = read_reg(&mut emu, "YMM0", 32);
            let out_ymm1 = read_reg(&mut emu, "YMM1", 32);
            assert_eq!(
                &out_ymm0[16..32],
                &[0xA5u8; 16],
                "legacy roundsd must preserve YMM0[255:128]"
            );
            assert_eq!(
                &out_ymm1[16..32],
                &[0x5Au8; 16],
                "legacy roundss must preserve YMM1[255:128]"
            );

            // Contrast with VEX-128 ZeroUpperTo { full_reg_bytes: 32 } through apply_vector_write.
            let (sp, off, _) = lookup_reg(&emu, "XMM0").unwrap();
            let xmm0_vn = Varnode {
                space_id: sp,
                offset: off,
                size: 16,
                is_constant: false,
                constant_val: 0,
            };
            let slot = VectorRegisterBank::resolve_enclosing_slot(&emu, &xmm0_vn, VectorIsa::X86)
                .expect("x86 slot");
            assert!(slot.full_bytes >= 32);
            let audit = apply_vector_write(
                &mut emu,
                &xmm0_vn,
                &[0x33u8; 16],
                VectorIsa::X86,
                UpperBitsPolicy::ZeroUpperTo { full_reg_bytes: 32 },
            )
            .expect("vex128 zero-upper");
            assert_eq!(audit.zeroed_upper_bytes, 16);
            assert_eq!(&read_reg(&mut emu, "YMM0", 32)[16..32], &[0u8; 16]);
        }
    }
}

#[test]
fn vector_contract_rejects_unsupported_widths_endianness_and_unknown_policies() {
    // 1. x86 unmodeled widths (32B AVX2 pblendw, 6B odd paddsb, 24B divps) must return false.
    let mut emu = x64_machine(&[], 1, true);
    let reg_space = emu.state.register_space();
    let base_off = lookup_reg(&emu, "XMM0").unwrap().1;

    for (name, size, extra_imm) in [
        ("pblendw", 32u32, Some(0x55i64)),
        ("paddsb", 6u32, None),
        ("divps", 24u32, None),
    ] {
        let uid = find_userop_id(&emu.userop_map, name);
        let dst = Varnode {
            space_id: reg_space,
            offset: base_off,
            size,
            is_constant: false,
            constant_val: 0,
        };
        let src = Varnode {
            space_id: reg_space,
            offset: base_off + 64,
            size,
            is_constant: false,
            constant_val: 0,
        };
        let mut inputs = vec![Varnode::constant(uid as i64, 4), dst.clone(), src];
        if let Some(imm) = extra_imm {
            inputs.push(Varnode::constant(imm, 1));
        }
        let op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::CallOther,
            address: CODE,
            output: Some(dst),
            inputs,
            asm_mnemonic: None,
        };
        assert!(
            !answer_vector_userop(&mut emu, &op),
            "{name} at {size}B must fail cleanly instead of truncating or mis-masking"
        );
    }

    // 2. apply_vector_write refuses mismatched slice length and UnknownUpper policy.
    let xmm0 = Varnode {
        space_id: reg_space,
        offset: base_off,
        size: 16,
        is_constant: false,
        constant_val: 0,
    };
    assert_eq!(
        apply_vector_write(
            &mut emu,
            &xmm0,
            &[0u8; 8],
            VectorIsa::X86,
            UpperBitsPolicy::PreserveUpper
        ),
        Err(VectorContractError::ExactWidthMismatch {
            expected: 16,
            actual: 8
        })
    );
    assert_eq!(
        apply_vector_write(
            &mut emu,
            &xmm0,
            &[0u8; 16],
            VectorIsa::X86,
            UpperBitsPolicy::UnknownUpper
        ),
        Err(VectorContractError::UnknownUpperPolicy)
    );
    assert!(describe_vector_op("NEON_ext", 16).is_some());

    // 3. AArch64 unmodeled 32B width for NEON_umaxp returns false, AND big-endian refuses.
    let mut arm_emu = aarch64_machine(&[], 1, true);
    let arm_reg_space = arm_emu.state.register_space();
    let q0_off = lookup_reg(&arm_emu, "q0").unwrap().1;
    let umaxp_id = find_userop_id(&arm_emu.userop_map, "NEON_umaxp");
    let wide_q0 = Varnode {
        space_id: arm_reg_space,
        offset: q0_off,
        size: 32,
        is_constant: false,
        constant_val: 0,
    };
    let op32 = PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::CallOther,
        address: CODE,
        output: Some(wide_q0.clone()),
        inputs: vec![
            Varnode::constant(umaxp_id as i64, 4),
            wide_q0.clone(),
            wide_q0.clone(),
            Varnode::constant(4, 1),
        ],
        asm_mnemonic: None,
    };
    assert!(
        !answer_vector_userop(&mut arm_emu, &op32),
        "32B unmodeled SVE-width NEON_umaxp must fail"
    );

    // Big-endian endianness rejection test on valid 16B shape:
    arm_emu.arch.endian = fission_emulator::arch::Endianness::Big;
    let q0_16 = Varnode {
        size: 16,
        ..wide_q0
    };
    let op_be = PcodeOp {
        seq_num: 1,
        opcode: PcodeOpcode::CallOther,
        address: CODE,
        output: Some(q0_16.clone()),
        inputs: vec![
            Varnode::constant(umaxp_id as i64, 4),
            q0_16.clone(),
            q0_16,
            Varnode::constant(4, 1),
        ],
        asm_mnemonic: None,
    };
    assert!(
        !answer_vector_userop(&mut arm_emu, &op_be),
        "big-endian emulator must refuse little-endian SIMD handlers"
    );
}
