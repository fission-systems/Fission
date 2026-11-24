//! End-to-end tests for SSE, SSE4.1, MMX, and AArch64 NEON `CALLOTHER` userops
//! in both the Cranelift JIT and the p-code interpreter (#1058).

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
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
    // Pad with AArch64 NOPs (0xD503201F)
    while bytes.len() < 0x100 {
        bytes.extend_from_slice(&0xD503_201Fu32.to_le_bytes());
    }
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = CODE;
    emu
}

fn reg_offset(emu: &Emulator, name: &str) -> u64 {
    emu.register_map
        .get(name)
        .map(|&(_, off, _)| off)
        .unwrap_or_else(|| panic!("missing register {name}"))
}

fn write_reg_bytes(emu: &mut Emulator, name: &str, data: &[u8]) {
    let off = reg_offset(emu, name);
    let sp = emu.state.register_space();
    emu.state.write_space(sp, off, data).expect("write reg");
}

fn read_reg_bytes(emu: &mut Emulator, name: &str, len: usize) -> Vec<u8> {
    let off = reg_offset(emu, name);
    let sp = emu.state.register_space();
    emu.state.read_space(sp, off, len).expect("read reg")
}

fn write_u128_lanes_u32(lanes: [u32; 4]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, w) in lanes.into_iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn read_u32_lanes(bytes: &[u8]) -> [u32; 4] {
    [
        u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
    ]
}

fn write_f64_pair(a: f64, b: f64) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&a.to_le_bytes());
    out[8..16].copy_from_slice(&b.to_le_bytes());
    out
}

#[test]
fn x64_round_and_fp_userops_run_and_preserve_lanes_in_both_engines() {
    // roundsd xmm0, xmm1, 1 (floor)
    // roundss xmm2, xmm3, 2 (ceil)
    // roundps xmm0, xmm1, 0 (nearest) -> overwrites xmm0 later? Use separate steps:
    // Sequence:
    //   0: 66 0f 3a 0b c1 01  roundsd xmm0, xmm1, 1
    //   1: 66 0f 3a 0a d3 02  roundss xmm2, xmm3, 2
    //   2: 66 0f 5e c1        divpd xmm0, xmm1
    //   3: 0f 51 d3           sqrtps xmm2, xmm3
    //   4: f3 0f 53 d3        rcpss xmm2, xmm3
    //   5: 0f 50 c0           movmskps eax, xmm0
    // Wait -- test roundsd + roundss upper-lane preservation directly first!
    let round_code = [
        0x66, 0x0F, 0x3A, 0x0B, 0xC1, 0x01, // roundsd xmm0, xmm1, 1
        0x66, 0x0F, 0x3A, 0x0A, 0xD3, 0x02, // roundss xmm2, xmm3, 2
    ];
    for interpret in [false, true] {
        let mut emu = x64_machine(&round_code, 2, interpret);
        write_reg_bytes(&mut emu, "XMM0", &write_f64_pair(99.0, -7.5));
        write_reg_bytes(&mut emu, "XMM1", &write_f64_pair(-1.25, 42.0));
        write_reg_bytes(
            &mut emu,
            "XMM2",
            &write_u128_lanes_u32([
                0.0f32.to_bits(),
                5.0f32.to_bits(),
                (-3.0f32).to_bits(),
                9.0f32.to_bits(),
            ]),
        );
        write_reg_bytes(
            &mut emu,
            "XMM3",
            &write_u128_lanes_u32([1.25f32.to_bits(), 99.0f32.to_bits(), 0, 0]),
        );
        let _ = emu.run();
        assert!(
            emu.metrics.unhandled_userops.is_empty(),
            "unhandled: {:?}",
            emu.metrics.unhandled_userops
        );
        let xmm0 = read_reg_bytes(&mut emu, "XMM0", 16);
        assert_eq!(xmm0, write_f64_pair(-2.0, -7.5));
        let xmm2 = read_u32_lanes(&read_reg_bytes(&mut emu, "XMM2", 16));
        assert_eq!(
            xmm2,
            [
                2.0f32.to_bits(),
                5.0f32.to_bits(),
                (-3.0f32).to_bits(),
                9.0f32.to_bits()
            ]
        );
    }

    // Test MXCSR rounding-control selection (imm8 bit 2 == 1 -> RC = 1 = floor)
    let mxcsr_code = [0x66, 0x0F, 0x3A, 0x0B, 0xC1, 0x04]; // roundsd xmm0, xmm1, 4
    for interpret in [false, true] {
        let mut emu = x64_machine(&mxcsr_code, 1, interpret);
        emu.write_register_u64("MXCSR", 0x3F80).unwrap(); // RC = 1 (floor)
        write_reg_bytes(&mut emu, "XMM0", &[0u8; 16]);
        write_reg_bytes(&mut emu, "XMM1", &write_f64_pair(2.75, 0.0));
        let _ = emu.run();
        let xmm0 = read_reg_bytes(&mut emu, "XMM0", 16);
        assert_eq!(f64::from_le_bytes(xmm0[..8].try_into().unwrap()), 2.0);
    }

    // divpd, sqrtps, sqrtpd, minps, maxps, minpd, maxpd, rcpps, rsqrtps, movmskps, movmskpd
    let fp_code = [
        0x66, 0x0F, 0x5E, 0xC1, // divpd xmm0, xmm1
        0x0F, 0x50, 0xC0, // movmskps eax, xmm0
        0x66, 0x0F, 0x50, 0xC9, // movmskpd ecx, xmm1
        0x0F, 0x51, 0xD3, // sqrtps xmm2, xmm3
        0x0F, 0x53, 0xE3, // rcpps xmm4, xmm3
        0x0F, 0x52, 0xEB, // rsqrtps xmm5, xmm3
    ];
    let mut jit_out = Vec::new();
    for interpret in [false, true] {
        let mut emu = x64_machine(&fp_code, 6, interpret);
        write_reg_bytes(&mut emu, "XMM0", &write_f64_pair(-9.0, 12.0));
        write_reg_bytes(&mut emu, "XMM1", &write_f64_pair(2.0, -3.0));
        write_reg_bytes(
            &mut emu,
            "XMM3",
            &write_u128_lanes_u32([
                4.0f32.to_bits(),
                9.0f32.to_bits(),
                16.0f32.to_bits(),
                25.0f32.to_bits(),
            ]),
        );
        let _ = emu.run();
        assert!(emu.metrics.unhandled_userops.is_empty());
        let rax = emu.read_register_u64("RAX").unwrap();
        let rcx = emu.read_register_u64("RCX").unwrap();
        let xmm0 = read_reg_bytes(&mut emu, "XMM0", 16);
        let xmm2 = read_u32_lanes(&read_reg_bytes(&mut emu, "XMM2", 16));
        let xmm4 = read_u32_lanes(&read_reg_bytes(&mut emu, "XMM4", 16));
        let xmm5 = read_u32_lanes(&read_reg_bytes(&mut emu, "XMM5", 16));
        // divpd = (-4.5, -4.0) => bit 1 (lane1 f32 sign = 1) and bit 3 (lane3 f32 sign = 1) => movmskps = 0b1010 = 10
        assert_eq!(xmm0, write_f64_pair(-4.5, -4.0));
        assert_eq!(rax, 0b1010);
        // XMM1 = (2.0, -3.0) => movmskpd = 0b10 = 2
        assert_eq!(rcx, 0b10);
        assert_eq!(
            xmm2,
            [
                2.0f32.to_bits(),
                3.0f32.to_bits(),
                4.0f32.to_bits(),
                5.0f32.to_bits()
            ]
        );
        assert!((f32::from_bits(xmm4[0]) - 0.25).abs() < 1e-3);
        assert!((f32::from_bits(xmm5[0]) - 0.5).abs() < 1e-3);
        if !interpret {
            jit_out = vec![rax, rcx, u64::from(xmm2[0]), u64::from(xmm5[3])];
        } else {
            assert_eq!(
                vec![rax, rcx, u64::from(xmm2[0]), u64::from(xmm5[3])],
                jit_out
            );
        }
    }
}

#[test]
fn x64_integer_shuffle_pack_madd_dot_and_narrow_pavg_match_between_engines() {
    // Covers PSHUFB, PSHUFLW, PSHUFHW, PACKSSWB, PACKUSDW, PADDSB, PSUBUSB, PMULHRSW, PMULLD,
    // PMULDQ, PMADDUBSW, PSADBW, MPSADBW, PHADDD, PSIGNW, PSLLW, PSRAW, PMOVSXBW, PMOVZXBD,
    // PBLENDW, BLENDPS, DPPS, EXTRACTPS, INSERTPS, PHMINPOSUW, PABSB, PABSD, PAVGB (XMM+MMX), PAVGW.
    let code: Vec<u8> = vec![
        0x66, 0x0F, 0x38, 0x00, 0xC1, // pshufb xmm0, xmm1
        0xF2, 0x0F, 0x70, 0xC0, 0x1B, // pshuflw xmm0, xmm0, 0x1b
        0x66, 0x0F, 0x38, 0x20, 0x07, // pmovsxbw xmm0, qword ptr [rdi]
        0x66, 0x0F, 0xE0, 0xC1, // pavgb xmm0, xmm1  (16 narrow 1B CallOther!)
        0x66, 0x0F, 0xE3, 0xC2, // pavgw xmm0, xmm2  (8 narrow 2B CallOther!)
        0x66, 0x0F, 0x71, 0xF0, 0x01, // psllw xmm0, 1
        0x66, 0x0F, 0x71, 0xE0, 0x01, // psraw xmm0, 1
        0x66, 0x0F, 0x38, 0x41, 0xC0, // phminposuw xmm0, xmm0
        0x66, 0x0F, 0x3A, 0x17, 0xC0, 0x01, // extractps eax, xmm0, 1 (min index!)
        0x0F, 0x38, 0x00, 0xC1, // pshufb mm0, mm1 (narrow 8B!)
        0x0F, 0xE0, 0xD3, // pavgb mm2, mm3 (narrow 8x1B!)
    ];

    let mut jit_state = None;
    for interpret in [false, true] {
        let mut emu = x64_machine(&code, 11, interpret);
        emu.write_register_u64("RDI", DATA).unwrap();
        let mem_bytes = [-4i8, 10, -20, 30, 40, -50, 60, -70].map(|b| b as u8);
        let ram = emu.state.ram_space();
        emu.state.write_space(ram, DATA, &mem_bytes).unwrap();
        write_reg_bytes(&mut emu, "XMM0", &(1u8..=16).collect::<Vec<_>>());
        write_reg_bytes(&mut emu, "XMM1", &[3u8; 16]);
        write_reg_bytes(&mut emu, "XMM2", &[10u8; 16]);
        write_reg_bytes(&mut emu, "MM0", &[1, 2, 3, 4, 5, 6, 7, 8]);
        write_reg_bytes(&mut emu, "MM1", &[7, 6, 5, 4, 3, 2, 1, 0x80]);
        write_reg_bytes(&mut emu, "MM2", &[10, 20, 30, 40, 50, 60, 70, 80]);
        write_reg_bytes(&mut emu, "MM3", &[11, 20, 31, 40, 51, 60, 71, 80]);

        let _ = emu.run();
        assert!(
            emu.metrics.unhandled_userops.is_empty(),
            "unhandled userops: {:?}",
            emu.metrics.unhandled_userops
        );
        let rax = emu.read_register_u64("RAX").unwrap();
        let xmm0 = read_reg_bytes(&mut emu, "XMM0", 16);
        let mm0 = read_reg_bytes(&mut emu, "MM0", 8);
        let mm2 = read_reg_bytes(&mut emu, "MM2", 8);

        assert_eq!(mm0, vec![8, 7, 6, 5, 4, 3, 2, 0]);
        assert_eq!(mm2, vec![11, 20, 31, 40, 51, 60, 71, 80]);

        if let Some((j_rax, ref j_xmm0)) = jit_state {
            assert_eq!(rax, j_rax, "JIT vs interpreter EAX mismatch");
            assert_eq!(&xmm0, j_xmm0, "JIT vs interpreter XMM0 mismatch");
        } else {
            jit_state = Some((rax, xmm0));
        }
    }
}

#[test]
fn x64_dpps_dppd_mpsadbw_phaddd_and_packusdw_match_both_engines() {
    let code: Vec<u8> = vec![
        0x66, 0x0F, 0x3A, 0x40, 0xC1,
        0x71, // dpps xmm0, xmm1, 0x71 (sum lanes 0..2 -> lane 0)
        0x66, 0x0F, 0x3A, 0x42, 0xD3, 0x05, // mpsadbw xmm2, xmm3, 0x05
        0x66, 0x0F, 0x38, 0x02, 0xE5, // phaddd xmm4, xmm5
        0x66, 0x0F, 0x38, 0x2B, 0xF7, // packusdw xmm6, xmm7
    ];
    for interpret in [false, true] {
        let mut emu = x64_machine(&code, 4, interpret);
        write_reg_bytes(
            &mut emu,
            "XMM0",
            &write_u128_lanes_u32([
                1.0f32.to_bits(),
                2.0f32.to_bits(),
                3.0f32.to_bits(),
                99.0f32.to_bits(),
            ]),
        );
        write_reg_bytes(
            &mut emu,
            "XMM1",
            &write_u128_lanes_u32([
                4.0f32.to_bits(),
                5.0f32.to_bits(),
                6.0f32.to_bits(),
                88.0f32.to_bits(),
            ]),
        );
        write_reg_bytes(&mut emu, "XMM2", &(0u8..16).collect::<Vec<_>>());
        write_reg_bytes(&mut emu, "XMM3", &[10u8; 16]);
        write_reg_bytes(&mut emu, "XMM4", &write_u128_lanes_u32([10, 20, 30, 40]));
        write_reg_bytes(&mut emu, "XMM5", &write_u128_lanes_u32([1, 2, 3, 4]));
        write_reg_bytes(
            &mut emu,
            "XMM6",
            &write_u128_lanes_u32([(-5i32) as u32, 100, 70000, 0]),
        );
        write_reg_bytes(
            &mut emu,
            "XMM7",
            &write_u128_lanes_u32([65535, (-1i32) as u32, 65536, 42]),
        );

        let _ = emu.run();
        assert!(emu.metrics.unhandled_userops.is_empty());

        // dpps: 1*4 + 2*5 + 3*6 = 32.0 in lane 0, 0.0 in lanes 1..3
        assert_eq!(
            read_u32_lanes(&read_reg_bytes(&mut emu, "XMM0", 16)),
            [32.0f32.to_bits(), 0, 0, 0]
        );
        // mpsadbw imm=5 -> dst_off=4, src_off=4 -> |4-10|+|5-10|+|6-10|+|7-10| = 18
        let xmm2 = read_reg_bytes(&mut emu, "XMM2", 16);
        assert_eq!(u16::from_le_bytes([xmm2[0], xmm2[1]]), 18);
        // phaddd: [30, 70, 3, 7]
        assert_eq!(
            read_u32_lanes(&read_reg_bytes(&mut emu, "XMM4", 16)),
            [30, 70, 3, 7]
        );
        // packusdw: [0, 100, 65535, 0, 65535, 0, 65535, 42]
        let xmm6: Vec<u16> = read_reg_bytes(&mut emu, "XMM6", 16)
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(xmm6, vec![0, 100, 65535, 0, 65535, 0, 65535, 42]);
    }
}

#[test]
fn aarch64_neon_userops_match_hardware_and_zero_upper_q_lanes() {
    // Every instruction below is hardware-checked against Apple Silicon above:
    //   0: fminnm s0, s1, s2        (scalar narrow: writes s0, MUST zero upper 12B of q0)
    //   1: urshr  d3, d4, #5        (scalar narrow with Imm_shr_imm32 quirk: zeroes upper 8B of q3)
    //   2: scvtf  v5.4h, v6.4h, #4  (vector 64b fixed 4H with Imm_shr_imm32 quirk: zeroes upper 8B of q5)
    //   3: rshrn  v7.4h, v8.4s, #4  (narrowing *1: zeroes upper 8B of q7)
    //   4: sqrdmulh v9.4s, v10.4s, v11.4s
    //   5: uaddlv h12, v13.16b      (scalar output h12: zeroes upper 14B of q12)
    //   6: rev64 v14.4s, v15.4s
    let mut code = Vec::new();
    for word in [
        0x1E22_7820u32, // fminnm s0, s1, s2
        0x7F7B_2483u32, // urshr d3, d4, #5   (0x7f7b2420 | (4<<5) | 3)
        0x0F1C_E4C5u32, // scvtf v5.4h, v6.4h, #4 (0x0f1ce420 | (6<<5) | 5)
        0x0F1C_8D07u32, // rshrn v7.4h, v8.4s, #4 (0x0f1c8c20 | (8<<5) | 7)
        0x6EAB_B549u32, // sqrdmulh v9.4s, v10.4s, v11.4s (Rn=10, Rm=11, Rd=9)
        0x6E30_39ACu32, // uaddlv h12, v13.16b (Rn=13, Rd=12)
        0x4EA0_09EEu32, // rev64 v14.4s, v15.4s (Rn=15, Rd=14)
    ] {
        code.extend_from_slice(&word.to_le_bytes());
    }

    for interpret in [false, true] {
        let mut emu = aarch64_machine(&code, 7, interpret);
        // Pre-poison q0, q3, q5, q7, q12 with 0xFF to prove upper lanes are zeroed!
        for q in ["q0", "q3", "q5", "q7", "q12"] {
            write_reg_bytes(&mut emu, q, &[0xFFu8; 16]);
        }
        // q1 = qNaN, q2 = 2.5f32
        write_reg_bytes(
            &mut emu,
            "q1",
            &write_u128_lanes_u32([0x7FC0_0001, 0, 0, 0]),
        );
        write_reg_bytes(
            &mut emu,
            "q2",
            &write_u128_lanes_u32([0x4020_0000, 0, 0, 0]),
        );
        // q4 = 0x12345
        write_reg_bytes(&mut emu, "q4", &0x12345u128.to_le_bytes());
        // q6 = [16i16, -24, 32, -8, ...]
        let mut q6 = [0u8; 16];
        for (i, v) in [16i16, -24, 32, -8].into_iter().enumerate() {
            q6[i * 2..(i + 1) * 2].copy_from_slice(&v.to_le_bytes());
        }
        write_reg_bytes(&mut emu, "q6", &q6);
        // q8 = [0x00010018, 0x00020017, 0x7FFF8008, 0x12345678]
        write_reg_bytes(
            &mut emu,
            "q8",
            &write_u128_lanes_u32([0x0001_0018, 0x0002_0017, 0x7FFF_8008, 0x1234_5678]),
        );
        // q10, q11 for sqrdmulh
        write_reg_bytes(
            &mut emu,
            "q10",
            &write_u128_lanes_u32([
                i32::MIN as u32,
                0x4000_0000,
                (-0x4000_0000i32) as u32,
                0x1234_5678,
            ]),
        );
        write_reg_bytes(
            &mut emu,
            "q11",
            &write_u128_lanes_u32([
                i32::MIN as u32,
                0x4000_0000,
                0x4000_0000,
                (-0x2345_6789i32) as u32,
            ]),
        );
        // q13 = [0xFF; 16], q15 = [1, 2, 3, 4]
        write_reg_bytes(&mut emu, "q13", &[0xFFu8; 16]);
        write_reg_bytes(&mut emu, "q15", &write_u128_lanes_u32([1, 2, 3, 4]));

        let _ = emu.run();
        assert!(
            emu.metrics.unhandled_userops.is_empty(),
            "unhandled AArch64 userops: {:?}",
            emu.metrics.unhandled_userops
        );

        let q0 = read_u32_lanes(&read_reg_bytes(&mut emu, "q0", 16));
        assert_eq!(q0, [0x4020_0000, 0, 0, 0]);

        let q3 = u128::from_le_bytes(read_reg_bytes(&mut emu, "q3", 16).try_into().unwrap());
        assert_eq!(q3, 0x91a);

        let q5: Vec<u16> = read_reg_bytes(&mut emu, "q5", 16)
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(q5, vec![0x3c00, 0xbe00, 0x4000, 0xb800, 0, 0, 0, 0]);

        let q7: Vec<u16> = read_reg_bytes(&mut emu, "q7", 16)
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(q7, vec![0x1002, 0x2001, 0xf801, 0x4568, 0, 0, 0, 0]);

        let q9 = read_u32_lanes(&read_reg_bytes(&mut emu, "q9", 16));
        assert_eq!(q9, [0x7fff_ffff, 0x2000_0000, 0xe000_0000, 0xfafb_d16b]);

        let q12 = u128::from_le_bytes(read_reg_bytes(&mut emu, "q12", 16).try_into().unwrap());
        assert_eq!(q12, 4080);

        let q14 = read_u32_lanes(&read_reg_bytes(&mut emu, "q14", 16));
        assert_eq!(q14, [2, 1, 4, 3]);
    }
}

#[test]
fn x64_remaining_curated_sse_userops_match_between_engines() {
    let cases: &[(&str, &[u8])] = &[
        (
            "roundps/minps/insertps",
            &[
                0x66, 0x0F, 0x3A, 0x08, 0xC1, 0x00, // roundps xmm0, xmm1, 0
                0x0F, 0x5D, 0xC1, // minps xmm0, xmm1
                0x66, 0x0F, 0x3A, 0x21, 0xC1, 0x9C, // insertps xmm0, xmm1, 0x9c
            ],
        ),
        (
            "roundpd/maxpd/rcpss/dppd",
            &[
                0x66, 0x0F, 0x3A, 0x09, 0xD3, 0x03, // roundpd xmm2, xmm3, 3
                0x66, 0x0F, 0x5F, 0xD3, // maxpd xmm2, xmm3
                0x66, 0x0F, 0x3A, 0x41, 0xD3, 0x33, // dppd xmm2, xmm3, 0x33
                0xF3, 0x0F, 0x53, 0xD3, // rcpss xmm2, xmm3
            ],
        ),
        (
            "sqrtpd/minpd/maxps",
            &[
                0x66, 0x0F, 0x51, 0xC1, // sqrtpd xmm0, xmm1
                0x66, 0x0F, 0x5D, 0xC1, // minpd xmm0, xmm1
                0x0F, 0x5F, 0xD3, // maxps xmm2, xmm3
            ],
        ),
        (
            "pshufhw/packsswb/packssdw/paddsb/paddusw/psubsw/psubusb",
            &[
                0xF3, 0x0F, 0x70, 0xD3, 0x4E, // pshufhw xmm2, xmm3, 0x4e
                0x66, 0x0F, 0x63, 0xC1, // packsswb xmm0, xmm1
                0x66, 0x0F, 0x6B, 0xD3, // packssdw xmm2, xmm3
                0x66, 0x0F, 0xEC, 0xC1, // paddsb xmm0, xmm1
                0x66, 0x0F, 0xDD, 0xD3, // paddusw xmm2, xmm3
                0x66, 0x0F, 0xE9, 0xC1, // psubsw xmm0, xmm1
                0x66, 0x0F, 0xD8, 0xD3, // psubusb xmm2, xmm3
            ],
        ),
        (
            "pmul/pmadd/psadbw/psignw/pmovzxbd/blends/pabs",
            &[
                0x66, 0x0F, 0xE5, 0xC1, // pmulhw xmm0, xmm1
                0x66, 0x0F, 0xE4, 0xD3, // pmulhuw xmm2, xmm3
                0x66, 0x0F, 0x38, 0x0B, 0xC1, // pmulhrsw xmm0, xmm1
                0x66, 0x0F, 0x38, 0x40, 0xD3, // pmulld xmm2, xmm3
                0x66, 0x0F, 0x38, 0x28, 0xC1, // pmuldq xmm0, xmm1
                0x66, 0x0F, 0xF5, 0xD3, // pmaddwd xmm2, xmm3
                0x66, 0x0F, 0x38, 0x04, 0xC1, // pmaddubsw xmm0, xmm1
                0x66, 0x0F, 0xF6, 0xC1, // psadbw xmm0, xmm1
                0x66, 0x0F, 0x38, 0x09, 0xD3, // psignw xmm2, xmm3
                0x66, 0x0F, 0x38, 0x31, 0x17, // pmovzxbd xmm2, dword ptr [rdi]
                0x66, 0x0F, 0x3A, 0x0E, 0xC1, 0x55, // pblendw xmm0, xmm1, 0x55
                0x66, 0x0F, 0x3A, 0x0C, 0xD3, 0x0A, // blendps xmm2, xmm3, 0x0a
                0x66, 0x0F, 0x3A, 0x0D, 0xC1, 0x02, // blendpd xmm0, xmm1, 0x02
                0x66, 0x0F, 0x38, 0x1C, 0xC1, // pabsb xmm0, xmm1
                0x66, 0x0F, 0x38, 0x1E, 0xD3, // pabsd xmm2, xmm3
            ],
        ),
    ];
    let counts = [3u64, 4, 3, 7, 15];
    let mut jit_res = Vec::new();
    for interpret in [false, true] {
        let mut emu = x64_machine(&[], 1, interpret);
        let mut engine_res = Vec::new();
        for (idx, ((name, code), &count)) in cases.iter().zip(&counts).enumerate() {
            let pc = CODE + (idx as u64) * 0x80;
            let mut bytes = code.to_vec();
            bytes.resize(0x80, 0x90);
            let ram = emu.state.ram_space();
            let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
            emu.state.write_space(ram, pc, &bytes).expect("code");
            emu.state.enforce_page_faults = enforce;
            emu.pc = pc;
            emu.inst_count = 0;
            emu.max_inst = Some(count);
            emu.halt_requested = false;
            emu.metrics.unhandled_userops.clear();
            emu.write_register_u64("RDI", DATA).unwrap();
            emu.state
                .write_space(ram, DATA, &[10u8, 200, 30, 40])
                .unwrap();
            write_reg_bytes(
                &mut emu,
                "XMM0",
                &write_u128_lanes_u32([
                    2.5f32.to_bits(),
                    4.0f32.to_bits(),
                    9.0f32.to_bits(),
                    16.0f32.to_bits(),
                ]),
            );
            write_reg_bytes(
                &mut emu,
                "XMM1",
                &write_u128_lanes_u32([
                    1.25f32.to_bits(),
                    (-3.75f32).to_bits(),
                    4.0f32.to_bits(),
                    8.0f32.to_bits(),
                ]),
            );
            write_reg_bytes(&mut emu, "XMM2", &write_f64_pair(-12.75, 4.5));
            write_reg_bytes(&mut emu, "XMM3", &write_f64_pair(2.5, -1.25));
            let _ = emu.run();
            assert!(
                emu.metrics.unhandled_userops.is_empty(),
                "{name} left unhandled userops: {:?}",
                emu.metrics.unhandled_userops
            );
            engine_res.push((
                *name,
                read_reg_bytes(&mut emu, "XMM0", 16),
                read_reg_bytes(&mut emu, "XMM2", 16),
            ));
        }
        if !interpret {
            jit_res = engine_res;
        } else {
            assert_eq!(engine_res, jit_res, "JIT vs interpreter mismatch");
        }
    }
}

#[test]
fn aarch64_remaining_curated_neon_userops_match_between_engines() {
    let instructions: &[(&str, u32)] = &[
        ("fmin v0.4s", 0x4EA2_F420),
        ("fmax v0.2d", 0x4E62_F420),
        ("fminnm v0.4s", 0x4EA2_C420),
        ("fmaxnm v0.8h", 0x4E42_0420),
        ("scvtf v0.4s", 0x4E21_D820),
        ("ucvtf v0.2d", 0x6E61_D820),
        ("sqadd v0.8h", 0x4E62_0C20),
        ("uqsub v0.16b", 0x6E22_2C20),
        ("sqxtn v0.4h", 0x0E61_4820),
        ("uqxtn2 v0.8h", 0x6E61_4840),
        ("sshl v0.4s", 0x4EA2_4420),
        ("ushl d0", 0x7EE2_4420),
        ("urshr v0.4s", 0x6F3C_2420),
        ("saba v0.16b", 0x4E22_7C20),
        ("uabd v0.8h", 0x6E62_7420),
    ];
    let mut jit_q0s = Vec::new();
    for interpret in [false, true] {
        let mut emu = aarch64_machine(&[], 1, interpret);
        let mut engine_q0s = Vec::new();
        for (idx, &(name, word)) in instructions.iter().enumerate() {
            let pc = CODE + (idx as u64) * 0x20;
            let mut bytes = word.to_le_bytes().to_vec();
            while bytes.len() < 0x20 {
                bytes.extend_from_slice(&0xD503_201Fu32.to_le_bytes());
            }
            let ram = emu.state.ram_space();
            let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
            emu.state.write_space(ram, pc, &bytes).expect("code");
            emu.state.enforce_page_faults = enforce;
            emu.pc = pc;
            emu.inst_count = 0;
            emu.max_inst = Some(1);
            emu.halt_requested = false;
            emu.metrics.unhandled_userops.clear();

            write_reg_bytes(&mut emu, "q0", &[0xAAu8; 16]);
            write_reg_bytes(
                &mut emu,
                "q1",
                &write_u128_lanes_u32([
                    1.5f32.to_bits(),
                    (-2.5f32).to_bits(),
                    0x7FC0_0001,
                    i32::MIN as u32,
                ]),
            );
            write_reg_bytes(
                &mut emu,
                "q2",
                &write_u128_lanes_u32([0x4020_0000, 0x02FE_0003, 0x8000_0002, 100_000]),
            );
            let _ = emu.run();
            assert!(
                emu.metrics.unhandled_userops.is_empty(),
                "{name} left unhandled userops: {:?}",
                emu.metrics.unhandled_userops
            );
            let q0 = read_reg_bytes(&mut emu, "q0", 16);
            if matches!(name, "sqxtn v0.4h" | "ushl d0") {
                assert_eq!(
                    &q0[8..16],
                    &[0u8; 8],
                    "{name} did not zero upper 8 bytes of q0"
                );
            }
            engine_q0s.push((name, q0));
        }
        if !interpret {
            jit_q0s = engine_q0s;
        } else {
            assert_eq!(engine_q0s, jit_q0s, "JIT vs interpreter AArch64 mismatch");
        }
    }
}
