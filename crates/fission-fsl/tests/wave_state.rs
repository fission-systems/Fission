use fission_fsl::{
    compile_source, emit_instruction, execute_decoded, execute_wave, ExecutionStatus, FirOp,
    FslcPackage, MachineState, OutputLayer, ValueId, WaveContract, WaveState,
};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = include_str!("../specs/amdgcn-gfx900-vadd-u32-wave64.fsl");
const PROFILE: &str = "amdgcn.gfx900.vadd_u32.wave64";

fn package() -> FslcPackage {
    let package = compile_source(SOURCE).unwrap();
    assert_eq!(package.version, 5);
    FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap()
}
fn bytes(src0: u16, src1: u8, dst: u8) -> [u8; 4] {
    (0x68000000u32 | u32::from(src0) | (u32::from(src1) << 9) | (u32::from(dst) << 17))
        .to_le_bytes()
}
fn state(exec: u64, seed: u64) -> WaveState {
    WaveState {
        lanes: 64,
        exec,
        scalar: MachineState {
            registers: (0..96).map(|i| seed.wrapping_mul(i + 1)).collect(),
            flags: vec![1, 0],
        },
        lane_registers: (0..256)
            .map(|i| seed.wrapping_mul(i + 1).wrapping_add((i % 64) << 32))
            .collect(),
    }
}
fn oracle(before: &WaveState, src0: usize, src1: usize, dst: usize, scalar: bool) -> WaveState {
    let mut out = before.clone();
    for lane in 0..64 {
        if before.exec & (1u64 << lane) != 0 {
            let lhs = if scalar {
                before.scalar.registers[src0]
            } else {
                before.lane_registers[src0 * 64 + lane]
            };
            let rhs = before.lane_registers[src1 * 64 + lane];
            out.lane_registers[dst * 64 + lane] = ((u128::from(lhs) % (1u128 << 32)
                + u128::from(rhs) % (1u128 << 32))
                % (1u128 << 32)) as u64;
        }
    }
    out
}
fn observation(status: u32, state: &WaveState) -> String {
    let mut out = format!("{status} {} {}", state.lanes, state.exec);
    for v in state
        .scalar
        .registers
        .iter()
        .chain(&state.scalar.flags)
        .chain(&state.lane_registers)
    {
        out.push_str(&format!(" {v}"));
    }
    out.push('\n');
    out
}
fn checked(command: &mut Command) -> String {
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn lane_domains_versions_and_invalid_observations_fail_closed() {
    let mut p = package();
    for version in [1, 2, 3, 4] {
        p.version = version;
        assert!(p.encode_binary().is_err());
        let mut binary = package().encode_binary().unwrap();
        binary[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(FslcPackage::decode_binary(&binary).is_err());
    }
    for bad in [
        SOURCE.replace("%exec: u64", "%exec: u32"),
        SOURCE.replace("lane.mask.read 64", "lane.mask.read 65"),
        SOURCE.replace("lane.mask.read 64", "lane.mask.read 0"),
        SOURCE.replace(
            "lane.register.write destination, %sum, %exec",
            "register.write destination, %sum",
        ),
        SOURCE.replace("u32.add.wrap %lhs, %rhs", "u32.add.wrap %lhs, %exec"),
        SOURCE.replace("source1, 0, %exec", "source1, 0, %lhs"),
    ] {
        assert!(compile_source(&bad).is_err());
    }
    let p = package();
    let decoded = p.decode_bytes(PROFILE, &bytes(256, 1, 2)).unwrap().unwrap();
    for mut s in [
        WaveState {
            lanes: 32,
            ..state(0, 1)
        },
        WaveState {
            lane_registers: vec![1; 64 * 2],
            ..state(0, 1)
        },
        WaveState {
            lane_registers: vec![1; 255],
            ..state(0, 1)
        },
        WaveState {
            scalar: MachineState {
                registers: vec![],
                flags: vec![2],
            },
            ..state(0, 1)
        },
    ] {
        let before = s.clone();
        assert_eq!(
            execute_wave(&p, &decoded, &mut s).unwrap(),
            ExecutionStatus::InvalidState
        );
        assert_eq!(s, before);
    }
    let mut forged = decoded.clone();
    forged.fields[0].1 = 511;
    let mut s = state(u64::MAX, 17);
    let before = s.clone();
    assert!(execute_wave(&p, &forged, &mut s).is_err());
    assert_eq!(s, before);
    assert!(execute_decoded(&p, &decoded, &mut s.scalar).is_err());
    for src0 in [96, 127, 128, 249, 250, 255] {
        assert!(p
            .decode_bytes(PROFILE, &bytes(src0, 1, 2))
            .unwrap()
            .is_none());
    }
    let mut i = p.instructions[0].clone();
    i.ops[1] = FirOp::LaneRead {
        output: ValueId(1),
        field: 99,
        bias: 256,
        mask: ValueId(0),
    };
    assert!(i.validate().is_err());
}

#[test]
fn scalar_effects_run_once_at_zero_exec_and_generic_mask_bounds_are_checked() {
    let source = SOURCE.replace(
        "%lhs: u32 = register.read source0;",
        "%lhs: u32 = register.read source0;\n register.write source0, %lhs;",
    );
    let p = compile_source(&source).unwrap();
    let d = p.decode_bytes(PROFILE, &bytes(0, 1, 2)).unwrap().unwrap();
    let mut s = state(0, u64::MAX);
    let mut expected = s.clone();
    expected.scalar.registers[0] = u64::from(u32::MAX);
    assert_eq!(
        execute_wave(&p, &d, &mut s).unwrap(),
        ExecutionStatus::Success
    );
    assert_eq!(s, expected);
    let small = compile_source(&SOURCE.replace("lane.mask.read 64", "lane.mask.read 4")).unwrap();
    let d = small
        .decode_bytes(PROFILE, &bytes(256, 1, 2))
        .unwrap()
        .unwrap();
    let mut s = WaveState {
        lanes: 4,
        exec: 16,
        lane_registers: vec![99; 16],
        ..state(0, 1)
    };
    let before = s.clone();
    assert_eq!(
        execute_wave(&small, &d, &mut s).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(s, before);
    // Highest VGPR and SGPR admitted by the profile, plus highest lane.
    for src0 in [95, 511] {
        let p = package();
        let d = p
            .decode_bytes(PROFILE, &bytes(src0, 255, 255))
            .unwrap()
            .unwrap();
        let mut s = WaveState {
            lane_registers: vec![u64::MAX; 256 * 64],
            ..state(1 << 63, u64::MAX)
        };
        let expected = oracle(&s, if src0 == 95 { 95 } else { 255 }, 255, 255, src0 == 95);
        assert_eq!(
            execute_wave(&p, &d, &mut s).unwrap(),
            ExecutionStatus::Success
        );
        assert_eq!(s, expected);
    }
}

#[test]
fn wave64_matches_independent_oracle_and_c_rust_recompilation() {
    verify(false);
    verify(true);
}

#[test]
fn wave_cli_executes_the_compiled_package() {
    let directory = std::env::temp_dir().join(format!("fsl-wave-cli-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("wave.fslc");
    fs::write(&path, package().encode_binary().unwrap()).unwrap();
    let s = state(1u64 << 63, u64::MAX);
    let csv = |words: &[u64]| {
        words
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let out = checked(
        Command::new(env!("CARGO_BIN_EXE_fslc")).args([
            "execute-wave",
            path.to_str().unwrap(),
            PROFILE,
            &bytes(256, 1, 2)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            "64",
            "0x8000000000000000",
            &csv(&s.scalar.registers),
            &csv(&s.scalar.flags),
            &csv(&s.lane_registers),
        ]),
    );
    let expected = oracle(&s, 0, 1, 2, false);
    assert_eq!(out, format!("status=Success lanes=64 exec=0x8000000000000000 registers={:?} flags={:?} lane_registers={:?}\n", expected.scalar.registers, expected.scalar.flags, expected.lane_registers));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn generic_mask_and_scalar_effect_contracts_recompile() {
    // Four lanes are a synthetic FIR contract, not a GFX900 execution profile.
    let source = SOURCE
        .replace("lane.mask.read 64", "lane.mask.read 4")
        .replace(
            "%lhs: u32 = register.read source0;",
            "%lhs: u32 = register.read source0;\n register.write source0, %lhs;",
        );
    let p = compile_source(&source).unwrap();
    let instruction = &p.instructions[1];
    let directory = std::env::temp_dir().join(format!("fsl-wave-generic-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut c = emit_instruction(instruction, OutputLayer::C, "fsl_wave").unwrap();
    c.push_str("\nint main(void) { uint64_t masks[4]={16,0,9,0}; for(size_t k=0;k<4;k++) { uint64_t r[1]={UINT64_C(0x1ffffffff)}, f[1]={1}, v[12], fields[3]={0,1,2}; for(size_t i=0;i<12;i++) v[i]=17; uint32_t status=fsl_wave(r,1,f,1,v,k==3?8:12,4,masks[k],fields,3); if(status != (k==0||k==3?3:0)) return 4; if(r[0] != (status?UINT64_C(0x1ffffffff):UINT64_C(0xffffffff)) || f[0]!=1) return 5; for(size_t i=0;i<12;i++) if(v[i] != (k==2&&(i==8||i==11)?16:17)) return 6; } return 0; }\n");
    let mut rust = emit_instruction(instruction, OutputLayer::Rust, "fsl_wave").unwrap();
    rust.push_str("\nfn main() { for (k,exec) in [16,0,9,0].into_iter().enumerate() { let mut r=[0x1ffffffffu64]; let mut f=[1]; let mut v=[17;12]; let status=fsl_wave(&mut r,&mut f,&mut v[..if k==3 {8} else {12}],4,exec,&[0,1,2]); assert_eq!(status,if k==0||k==3 {3} else {0}); assert_eq!(r[0],if status==3 {0x1ffffffff} else {0xffffffff}); assert_eq!(f,[1]); for (i,value) in v.into_iter().enumerate() { assert_eq!(value,if k==2&&(i==8||i==11) {16} else {17}); } } }\n");
    fs::write(directory.join("out.c"), c).unwrap();
    fs::write(directory.join("out.rs"), rust).unwrap();
    for opt in ["0", "2"] {
        let ce = directory.join(format!("c{opt}"));
        let re = directory.join(format!("r{opt}"));
        checked(
            Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
                .arg(format!("-O{opt}"))
                .arg(directory.join("out.c"))
                .arg("-o")
                .arg(&ce),
        );
        checked(
            Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
                .args(["--edition=2021", "-Dwarnings", "-C"])
                .arg(format!("opt-level={opt}"))
                .arg(directory.join("out.rs"))
                .arg("-o")
                .arg(&re),
        );
        checked(&mut Command::new(ce));
        checked(&mut Command::new(re));
    }
    println!("synthetic four-lane/scalar contract states=4; C/Rust O0/O2 comparisons=16");
    fs::remove_dir_all(directory).unwrap();
}

fn verify(scalar: bool) {
    let p = package();
    let instruction = &p.instructions[usize::from(scalar)];
    let contract = WaveContract::for_instruction(instruction).unwrap();
    assert_eq!(contract.lanes, 64);
    let directory = std::env::temp_dir().join(format!(
        "fsl-wave-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let mut c = emit_instruction(instruction, OutputLayer::C, "fsl_wave").unwrap();
    c.push_str("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) { uint64_t fields[3], registers[96], flags[2], vr[256], ex; unsigned n; size_t fc,nr,nf,nv; while(scanf(\"%u %\" SCNu64 \" %zu %zu %zu %zu %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64, &n,&ex,&fc,&nr,&nf,&nv,&fields[0],&fields[1],&fields[2])==9) { for(size_t i=0;i<96;i++) if(scanf(\"%\" SCNu64,&registers[i])!=1) return 4; for(size_t i=0;i<2;i++) if(scanf(\"%\" SCNu64,&flags[i])!=1) return 4; for(size_t i=0;i<256;i++) if(scanf(\"%\" SCNu64,&vr[i])!=1) return 4; uint32_t status=fsl_wave(registers,nr,flags,nf,vr,nv,(uint16_t)n,ex,fields,fc); printf(\"%u %u %\" PRIu64,status,n,ex); for(size_t i=0;i<96;i++) printf(\" %\" PRIu64,registers[i]); for(size_t i=0;i<2;i++) printf(\" %\" PRIu64,flags[i]); for(size_t i=0;i<256;i++) printf(\" %\" PRIu64,vr[i]); puts(\"\"); } return 0; }\n");
    let mut rust = emit_instruction(instruction, OutputLayer::Rust, "fsl_wave").unwrap();
    rust.push_str("\nfn main() { use std::io::Read; let mut text=String::new(); std::io::stdin().read_to_string(&mut text).unwrap(); let words=text.split_whitespace().map(|v|v.parse::<u64>().unwrap()).collect::<Vec<_>>(); for row in words.chunks_exact(363) { let mut registers=row[9..105].to_vec(); let mut flags=row[105..107].to_vec(); let mut vr=row[107..363].to_vec(); let status=fsl_wave(&mut registers[..row[3] as usize],&mut flags[..row[4] as usize],&mut vr[..row[5] as usize],row[0] as u16,row[1],&row[6..6+row[2] as usize]); print!(\"{status} {} {}\",row[0],row[1]); for v in registers.iter().chain(&flags).chain(&vr) { print!(\" {v}\"); } println!(); } }\n");
    fs::write(directory.join("out.c"), c).unwrap();
    fs::write(directory.join("out.rs"), rust).unwrap();
    let mut input = fs::File::create(directory.join("input.txt")).unwrap();
    let mut expected = String::new();
    let mut count = 0;
    let mut push = |fields: [u64; 3], mut s: WaveState, lengths: [usize; 4], valid: bool| {
        write!(
            input,
            "{} {} {} {} {} {} {} {} {}",
            s.lanes,
            s.exec,
            lengths[0],
            lengths[1],
            lengths[2],
            lengths[3],
            fields[0],
            fields[1],
            fields[2]
        )
        .unwrap();
        for v in s
            .scalar
            .registers
            .iter()
            .chain(&s.scalar.flags)
            .chain(&s.lane_registers)
        {
            write!(input, " {v}").unwrap();
        }
        writeln!(input).unwrap();
        if valid {
            let before = s.clone();
            let raw = bytes(fields[0] as u16, fields[1] as u8, fields[2] as u8);
            let d = p.decode_bytes(PROFILE, &raw).unwrap().unwrap();
            assert_eq!(p.reencode(&d, &[]).unwrap(), raw);
            let wanted = oracle(
                &s,
                if scalar {
                    fields[0] as usize
                } else {
                    (fields[0] - 256) as usize
                },
                fields[1] as usize,
                fields[2] as usize,
                scalar,
            );
            assert_eq!(
                execute_wave(&p, &d, &mut s).unwrap(),
                ExecutionStatus::Success
            );
            assert_eq!(s, wanted);
            assert_eq!(s.exec, before.exec);
            assert_eq!(s.scalar, before.scalar);
            expected.push_str(&observation(0, &wanted));
        } else {
            expected.push_str(&observation(3, &s));
        }
        count += 1;
    };
    let masks = [
        0,
        u64::MAX,
        1,
        1u64 << 63,
        0xaaaaaaaaaaaaaaaa,
        0x5555555555555555,
        0x8000000000000001,
        0x00ff00ff00ff00ff,
    ];
    for a in 0..3 {
        for b in 0..3 {
            for d in 0..3 {
                for exec in masks {
                    for seed in [0, 1, u64::MAX, 0x80000000ffffffff] {
                        push(
                            [a + if scalar { 0 } else { 256 }, b, d],
                            state(exec, seed),
                            [3, 96, 2, 256],
                            true,
                        );
                    }
                }
            }
        }
    }
    let mut seed = 0x123456789abcdef0u64;
    for _ in 0..128 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        push(
            [
                seed % 3 + if scalar { 0 } else { 256 },
                (seed >> 9) % 3,
                (seed >> 17) % 3,
            ],
            state(seed, seed.rotate_left(13)),
            [3, 96, 2, 256],
            true,
        );
    }
    let base = [if scalar { 0 } else { 256 }, 1, 2];
    for (fields, lengths, bad_lanes, bad_flag) in [
        (base, [2, 96, 2, 256], 64, 1),
        (base, [3, 96, 2, 128], 64, 1),
        (base, [3, 96, 2, 255], 64, 1),
        (base, [3, 96, 2, 256], 32, 1),
        (base, [3, 96, 2, 256], 64, 2),
        ([base[0], 4, 2], [3, 96, 2, 256], 64, 1),
        ([base[0], 1, 4], [3, 96, 2, 256], 64, 1),
        (
            [if scalar { 96 } else { 255 }, 1, 2],
            [3, 96, 2, 256],
            64,
            1,
        ),
        (
            [if scalar { 128 } else { 512 }, 1, 2],
            [3, 96, 2, 256],
            64,
            1,
        ),
    ] {
        let mut s = state(0, 17);
        s.lanes = bad_lanes;
        s.scalar.flags[0] = bad_flag;
        push(fields, s, lengths, false);
    }
    if scalar {
        push(base, state(0, 17), [3, 0, 2, 256], false);
    }
    drop(input);
    for opt in ["0", "2"] {
        let c_exe = directory.join(format!("c{opt}"));
        let r_exe = directory.join(format!("r{opt}"));
        checked(
            Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
                .arg(format!("-O{opt}"))
                .arg(directory.join("out.c"))
                .arg("-o")
                .arg(&c_exe),
        );
        checked(
            Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
                .args(["--edition=2021", "-Dwarnings", "-C"])
                .arg(format!("opt-level={opt}"))
                .arg(directory.join("out.rs"))
                .arg("-o")
                .arg(&r_exe),
        );
        for exe in [c_exe, r_exe] {
            assert_eq!(
                checked(Command::new(exe).stdin(Stdio::from(
                    fs::File::open(directory.join("input.txt")).unwrap()
                ))),
                expected
            );
        }
    }
    println!(
        "wave64 scalar_source={scalar}; states={count}; C/Rust O0/O2 comparisons={}",
        count * 4
    );
    fs::remove_dir_all(directory).unwrap();
}
