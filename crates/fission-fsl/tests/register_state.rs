use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use fission_fsl::{
    compile_source, emit_instruction, execute_decoded, ExecutionStatus, FirOp, FslcPackage,
    MachineState, OutputLayer,
};

const SOURCE: &str = include_str!("../specs/amdgcn-gfx900-sadd-u32.fsl");
const PROFILE: &str = "amdgcn.gfx900.sadd_u32";
const CARRY_SOURCE: &str = include_str!("../specs/amdgcn-gfx900-saddc-u32.fsl");
const CARRY_PROFILE: &str = "amdgcn.gfx900.saddc_u32";

fn package() -> FslcPackage {
    let package = compile_source(SOURCE).unwrap();
    assert_eq!(package.version, 3);
    FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap()
}

fn bytes(src0: u8, src1: u8, dst: u8) -> [u8; 4] {
    [src0, src1, dst, 0x80]
}

#[test]
fn rejects_invalid_state_types_and_version_downgrade() {
    let mut package_value = package();
    package_value.version = 2;
    assert!(package_value.encode_binary().is_err());
    let mut raw = compile_source(SOURCE).unwrap().encode_binary().unwrap();
    raw[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert!(FslcPackage::decode_binary(&raw).is_err());
    for malformed in [
        SOURCE.replace("%carry: u1", "%carry: u32"),
        SOURCE.replace("%rhs: u32", "%rhs: i32"),
        SOURCE.replace("register.read source0", "register.read missing"),
        SOURCE.replace("flag.write 0, %carry", "flag.write 0, %sum"),
    ] {
        assert!(compile_source(&malformed).is_err());
    }
    let mut instruction = package().instructions.remove(0);
    instruction.ops[0] = FirOp::RegisterRead {
        output: fission_fsl::ValueId(0),
        field: 99,
    };
    assert!(instruction.validate().is_err());
}

#[test]
fn refusal_preserves_all_state_and_cli_runs_decoded_fir() {
    let package = package();
    let decoded = package
        .decode_bytes(PROFILE, &bytes(0, 1, 2))
        .unwrap()
        .unwrap();
    for mut state in [
        MachineState {
            registers: vec![1, 2],
            flags: vec![0],
        },
        MachineState {
            registers: vec![1, 2, 3],
            flags: vec![],
        },
        MachineState {
            registers: vec![1, 2, 3],
            flags: vec![2],
        },
    ] {
        let before = state.clone();
        assert_eq!(
            execute_decoded(&package, &decoded, &mut state).unwrap(),
            ExecutionStatus::InvalidState
        );
        assert_eq!(state, before);
    }
    let mut tampered = decoded;
    tampered.fields[0].1 = 95;
    let mut state = MachineState {
        registers: vec![1, 2, 3],
        flags: vec![0],
    };
    let before = state.clone();
    assert!(execute_decoded(&package, &tampered, &mut state).is_err());
    assert_eq!(state, before);
    for raw in [
        bytes(96, 1, 2),
        bytes(0, 127, 2),
        bytes(255, 1, 2),
        bytes(0, 1, 96),
    ] {
        assert!(package.decode_bytes(PROFILE, &raw).unwrap().is_none());
    }
    let directory = temporary();
    let path = directory.join("sadd.fslc");
    fs::write(&path, package.encode_binary().unwrap()).unwrap();
    let output = checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
        "execute-state",
        path.to_str().unwrap(),
        PROFILE,
        "00010280",
        "0xffffffff,1,123",
        "0",
    ]));
    assert!(output.contains("status=Success registers=[4294967295, 1, 0] flags=[1]"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn carry_input_rejects_bad_types_versions_and_preserves_state() {
    let mut package = compile_source(CARRY_SOURCE).unwrap();
    assert_eq!(package.version, 4);
    for version in [2, 3] {
        package.version = version;
        assert!(package.encode_binary().is_err());
        let mut raw = compile_source(CARRY_SOURCE)
            .unwrap()
            .encode_binary()
            .unwrap();
        raw[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(FslcPackage::decode_binary(&raw).is_err());
    }
    for malformed in [
        CARRY_SOURCE.replace("%carry_in: u1", "%carry_in: u32"),
        CARRY_SOURCE.replace("%carry: u1", "%carry: u32"),
        CARRY_SOURCE.replace("%rhs, %carry_in", "%rhs, %lhs"),
    ] {
        assert!(compile_source(&malformed).is_err());
    }
    let package = compile_source(CARRY_SOURCE).unwrap();
    let decoded = package
        .decode_bytes(CARRY_PROFILE, &[0, 1, 2, 0x82])
        .unwrap()
        .unwrap();
    for flags in [vec![], vec![2]] {
        let mut state = MachineState {
            registers: vec![1, 2, 3],
            flags,
        };
        let before = state.clone();
        assert_eq!(
            execute_decoded(&package, &decoded, &mut state).unwrap(),
            ExecutionStatus::InvalidState
        );
        assert_eq!(state, before);
    }
    // Read-only flag effects must also be checked before any register mutation.
    let read_only = CARRY_SOURCE
        .replace("flag.write 0, %carry;", "")
        .replace("flag.read 0", "flag.read 7");
    let package = compile_source(&read_only).unwrap();
    let decoded = package
        .decode_bytes(CARRY_PROFILE, &[0, 1, 2, 0x82])
        .unwrap()
        .unwrap();
    let mut state = MachineState {
        registers: vec![1, 2, 3],
        flags: vec![0],
    };
    let before = state.clone();
    assert_eq!(
        execute_decoded(&package, &decoded, &mut state).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(state, before);
    let directory = temporary();
    let path = directory.join("saddc.fslc");
    fs::write(
        &path,
        compile_source(CARRY_SOURCE)
            .unwrap()
            .encode_binary()
            .unwrap(),
    )
    .unwrap();
    let output = checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
        "execute-state",
        path.to_str().unwrap(),
        CARRY_PROFILE,
        "00010282",
        "0xffffffff,0,123",
        "1",
    ]));
    assert!(output.contains("status=Success registers=[4294967295, 0, 0] flags=[1]"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn scalar_add_then_carry_input_matches_u64_addition() {
    let low = package();
    let high = compile_source(CARRY_SOURCE).unwrap();
    let lo_decoded = low.decode_bytes(PROFILE, &bytes(0, 2, 4)).unwrap().unwrap();
    let hi_decoded = high
        .decode_bytes(CARRY_PROFILE, &[1, 3, 5, 0x82])
        .unwrap()
        .unwrap();
    let boundary = [
        0,
        1,
        u32::MAX as u64,
        1u64 << 32,
        u64::MAX,
        0x7fff_ffff_ffff_ffff,
    ];
    let mut count = 0;
    let mut seed = 0x42a9_5678_1234_9012u64;
    let mut pairs = Vec::new();
    for left in boundary {
        for right in boundary {
            pairs.push((left, right));
        }
    }
    for _ in 0..1024 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let left = seed;
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        pairs.push((left, seed));
    }
    for (left, right) in pairs {
        let mut state = MachineState {
            registers: vec![
                left & 0xffff_ffff,
                left >> 32,
                right & 0xffff_ffff,
                right >> 32,
                99,
                99,
                77,
            ],
            flags: vec![1, 1],
        };
        assert_eq!(
            execute_decoded(&low, &lo_decoded, &mut state).unwrap(),
            ExecutionStatus::Success
        );
        assert_eq!(
            execute_decoded(&high, &hi_decoded, &mut state).unwrap(),
            ExecutionStatus::Success
        );
        let full = u128::from(left) + u128::from(right);
        assert_eq!(state.registers[4] | state.registers[5] << 32, full as u64);
        assert_eq!(state.flags, vec![(full >> 64) as u64, 1]);
        assert_eq!(state.registers[6], 77);
        count += 1;
    }
    println!("scalar add/carry chain cases={count}");
}

fn temporary() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("fission-register-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn checked(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run(executable: &Path, input: &Path) -> String {
    checked(Command::new(executable).stdin(Stdio::from(fs::File::open(input).unwrap())))
}

fn observation(status: u32, state: &MachineState) -> String {
    let mut text = status.to_string();
    for value in state.registers.iter().chain(&state.flags) {
        text.push_str(&format!(" {value}"));
    }
    text.push('\n');
    text
}

#[test]
fn decoded_scalar_add_matches_independent_oracle_and_c_rust_recompilation() {
    verify_projection(SOURCE, PROFILE, 0x80, 32, false);
}

#[test]
fn carry_input_matches_oracle_and_recompilation_across_widths() {
    for bits in [1, 8, 16, 32, 64] {
        let source = CARRY_SOURCE.replace("u32", &format!("u{bits}"));
        let profile = CARRY_PROFILE.replace("u32", &format!("u{bits}"));
        verify_projection(&source, &profile, 0x82, bits, true);
    }
}

fn verify_projection(source: &str, profile: &str, high: u8, bits: u16, carry_input: bool) {
    let directory = temporary();
    let compiled = compile_source(source).unwrap();
    assert_eq!(compiled.version, if carry_input { 4 } else { 3 });
    let package = FslcPackage::decode_binary(&compiled.encode_binary().unwrap()).unwrap();
    let instruction = &package.instructions[0];
    let original = instruction.clone();
    let mut c = emit_instruction(instruction, OutputLayer::C, "fsl_scalar").unwrap();
    let mut rust = emit_instruction(instruction, OutputLayer::Rust, "fsl_scalar").unwrap();
    assert_eq!(instruction, &original);
    c.push_str("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) { uint64_t fields[3], registers[96], flags[2]; size_t nr, nf; while (scanf(\"%\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %zu %zu\", &fields[0], &fields[1], &fields[2], &nr, &nf) == 5) { for (size_t i=0;i<96;i++) if(scanf(\"%\" SCNu64, &registers[i]) != 1) return 4; for(size_t i=0;i<2;i++) if(scanf(\"%\" SCNu64, &flags[i]) != 1) return 4; uint32_t status=fsl_scalar(registers,nr,flags,nf,fields,3); printf(\"%u\",status); for(size_t i=0;i<96;i++) printf(\" %\" PRIu64,registers[i]); for(size_t i=0;i<2;i++) printf(\" %\" PRIu64,flags[i]); puts(\"\"); } return 0; }\n");
    rust.push_str("\nfn main() { use std::io::Read; let mut text=String::new(); std::io::stdin().read_to_string(&mut text).unwrap(); let values=text.split_whitespace().map(|v|v.parse::<u64>().unwrap()).collect::<Vec<_>>(); for row in values.chunks_exact(103) { let mut registers=row[5..101].to_vec(); let mut flags=row[101..103].to_vec(); let status=fsl_scalar(&mut registers[..row[3] as usize],&mut flags[..row[4] as usize],&row[..3]); print!(\"{status}\"); for value in registers.iter().chain(&flags) { print!(\" {value}\"); } println!(); } }\n");
    fs::write(directory.join("out.c"), c).unwrap();
    fs::write(directory.join("out.rs"), rust).unwrap();
    let mut input = fs::File::create(directory.join("input.txt")).unwrap();
    let mut expected = String::new();
    let mut count = 0;
    let modulus = 1u128 << bits;
    let mask = (modulus - 1) as u64;
    let pairs = [
        (0, 0),
        (mask, 1),
        (mask >> 1, 1),
        (1u64 << (bits - 1), 1u64 << (bits - 1)),
        (mask, mask),
        (u64::MAX, u64::MAX),
        (123456789, 987654321),
        (0x1_0000_0000, 0x1_0000_0001),
    ];
    for source0 in [0u8, 1, 2, 95] {
        for source1 in [0u8, 1, 2, 95] {
            for destination in [0u8, 1, 2, 95] {
                let decoded = package
                    .decode_bytes(profile, &[source0, source1, destination, high])
                    .unwrap()
                    .unwrap();
                for (left, right) in pairs {
                    for initial_carry in [0, 1] {
                        let mut state = MachineState {
                            registers: (0..96).map(|i| 0x1234_5678_0000_0000 + i).collect(),
                            flags: vec![initial_carry, 1],
                        };
                        state.registers[usize::from(source0)] = left;
                        state.registers[usize::from(source1)] = right;
                        let fields = [
                            u64::from(source0),
                            u64::from(source1),
                            u64::from(destination),
                        ];
                        write!(input, "{} {} {} 96 2", fields[0], fields[1], fields[2]).unwrap();
                        for v in state.registers.iter().chain(&state.flags) {
                            write!(input, " {v}").unwrap();
                        }
                        writeln!(input).unwrap();
                        let mut oracle = state.clone();
                        // Independent full-state oracle: widened integer addition and division,
                        // not the FIR evaluator or carry emission formula.
                        let full = u128::from(state.registers[usize::from(source0)]) % modulus
                            + u128::from(state.registers[usize::from(source1)]) % modulus
                            + if carry_input {
                                u128::from(initial_carry)
                            } else {
                                0
                            };
                        oracle.registers[usize::from(destination)] = (full % modulus) as u64;
                        oracle.flags[0] = (full / modulus) as u64;
                        assert_eq!(
                            execute_decoded(&package, &decoded, &mut state).unwrap(),
                            ExecutionStatus::Success
                        );
                        assert_eq!(state, oracle);
                        expected.push_str(&observation(0, &oracle));
                        count += 1;
                    }
                }
            }
        }
    }
    // Emitter ABI preconditions must fail before *any* architectural mutation.
    for (fields, nr, nf, flag) in [
        ([0, 1, 2], 2, 2, 0),
        ([0, 1, 2], 96, 0, 0),
        ([0, 1, 2], 96, 2, 2),
        ([96, 1, 2], 96, 2, 0),
        ([128, 1, 2], 96, 2, 0),
        ([0, 1, 96], 96, 2, 0),
    ] {
        let state = MachineState {
            registers: vec![17; 96],
            flags: vec![flag, 1],
        };
        write!(input, "{} {} {} {nr} {nf}", fields[0], fields[1], fields[2]).unwrap();
        for v in state.registers.iter().chain(&state.flags) {
            write!(input, " {v}").unwrap();
        }
        writeln!(input).unwrap();
        expected.push_str(&observation(3, &state));
        count += 1;
    }
    drop(input);
    for level in ["0", "2"] {
        let c_executable = directory.join(format!("c-{level}"));
        checked(
            Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    &format!("-O{level}"),
                ])
                .arg(directory.join("out.c"))
                .arg("-o")
                .arg(&c_executable),
        );
        assert_eq!(run(&c_executable, &directory.join("input.txt")), expected);
        let rust_executable = directory.join(format!("rust-{level}"));
        checked(
            Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .args([
                    "--edition=2021",
                    "-Dwarnings",
                    "-C",
                    &format!("opt-level={level}"),
                ])
                .arg(directory.join("out.rs"))
                .arg("-o")
                .arg(&rust_executable),
        );
        assert_eq!(
            run(&rust_executable, &directory.join("input.txt")),
            expected
        );
    }
    println!(
        "profile={profile}; bits={bits}; state cases={count}; C/Rust O0/O2 comparisons={}",
        count * 4
    );
    fs::remove_dir_all(directory).unwrap();
}
