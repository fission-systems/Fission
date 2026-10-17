//! Forward-only guest control between sequence instances. The fixture language is
//! synthetic and not an ISA; success here is not binary lifting, whole-function
//! equivalence, or GPU evidence.
use fission_fsl::{
    compile_source, emit_instruction,
    sequence::{FirSequence, SequenceStateContract},
    ExecutionStatus, FslcPackage, MachineState, OutputLayer,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const SOURCE: &str = include_str!("../specs/guest-branch-forward.fsl");
const BASE: u64 = 0x1000;
const REGS: usize = 4;
const FLAGS: usize = 1;

fn package() -> FslcPackage {
    compile_source(SOURCE).unwrap()
}
fn addi(rd: u32, imm: u32) -> u32 {
    0x0100_0000 | imm << 8 | rd
}
fn br(offset: u32) -> u32 {
    0x0200_0000 | offset
}
fn brz(source: u32, offset: u32) -> u32 {
    0x0300_0000 | offset << 8 | source
}
fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn compose(words: &[u32]) -> FirSequence {
    FirSequence::compose(
        &package(),
        "fixture.guest.branch",
        BASE,
        &bytes(words),
        SequenceStateContract {
            register_count: REGS,
            flag_count: FLAGS,
        },
    )
    .unwrap()
}

/// Independent interpreter written from the fixture's prose contract only; it
/// shares no code with the FIR executor or emitters.
fn oracle(words: &[u32], registers: &[u64]) -> (u32, Vec<u64>) {
    let mut r = registers.to_vec();
    let end = BASE + 4 * words.len() as u64;
    let mut pc = BASE;
    while pc < end {
        let word = words[((pc - BASE) / 4) as usize];
        let mut next = pc + 4;
        match word >> 24 {
            1 => {
                let rd = (word & 0xf) as usize;
                let imm = u64::from((word >> 8) & 0xff);
                r[rd] = ((r[rd] & 0xffff_ffff) + imm) & 0xffff_ffff;
            }
            2 | 3 => {
                let taken = word >> 24 == 2 || r[(word & 0xf) as usize] & 0xffff_ffff == 0;
                if taken {
                    let delta = if word >> 24 == 2 {
                        u64::from(word & 0xff)
                    } else {
                        u64::from((word >> 8) & 0xff)
                    };
                    next = pc + 4 + delta;
                    if next > end || !(next - BASE).is_multiple_of(4) {
                        return (4, registers.to_vec());
                    }
                }
            }
            _ => unreachable!(),
        }
        pc = next;
    }
    (0, r)
}

fn programs() -> Vec<(&'static str, Vec<u32>)> {
    vec![
        (
            "diamond",
            vec![addi(0, 1), brz(1, 4), addi(2, 7), addi(3, 9)],
        ),
        ("branch-to-end", vec![brz(0, 8), addi(1, 1), addi(2, 2)]),
        (
            "unconditional-chain",
            vec![br(4), addi(1, 5), br(0), addi(0, 3)],
        ),
        (
            "alias-and-join",
            vec![addi(0, 1), brz(0, 4), addi(0, 200), brz(0, 0), addi(1, 1)],
        ),
    ]
}
fn bad_programs() -> Vec<(&'static str, Vec<u32>)> {
    vec![
        ("misaligned-taken", vec![addi(0, 1), br(1), addi(1, 1)]),
        ("past-end", vec![addi(0, 1), br(200), addi(1, 1)]),
        ("conditional-past-end", vec![brz(0, 9), addi(1, 1)]),
    ]
}
fn states() -> Vec<Vec<u64>> {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut out = vec![vec![0; REGS], vec![0, 1, 0, 1], vec![1, 0, 1, 0]];
    for _ in 0..40 {
        let mut row = vec![0u64; REGS];
        for value in &mut row {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            // Force many exact zeros and 32-bit wraps so both branch directions run.
            *value = match seed % 4 {
                0 => 0,
                1 => 0xffff_ff00 | (seed >> 8) & 0xff,
                2 => seed & 0xffff_ffff_0000_0000,
                _ => seed,
            };
        }
        out.push(row);
    }
    out
}

#[test]
fn reference_matches_independent_oracle_for_taken_untaken_join_and_alias() {
    let mut count = 0;
    for (name, words) in programs() {
        let plan = compose(&words);
        assert!(plan.has_guest_control(), "{name}");
        for registers in states() {
            let mut state = MachineState {
                registers: registers.clone(),
                flags: vec![1],
            };
            let status = plan.execute(&mut state).unwrap();
            let (expected_status, expected) = oracle(&words, &registers);
            assert_eq!(status as u32, expected_status, "{name} {registers:?}");
            assert_eq!(state.registers, expected, "{name} {registers:?}");
            assert_eq!(state.flags, [1]);
            count += 1;
        }
    }
    println!("guest control oracle: {count} states");
}

#[test]
fn bad_targets_fail_closed_and_preserve_state() {
    for (name, words) in bad_programs() {
        let plan = compose(&words);
        for registers in states() {
            let mut state = MachineState {
                registers: registers.clone(),
                flags: vec![0],
            };
            let status = plan.execute(&mut state).unwrap();
            let (expected_status, _) = oracle(&words, &registers);
            // Conditional targets are only bad when the branch is actually taken.
            assert_eq!(status as u32, expected_status, "{name} {registers:?}");
            if status == ExecutionStatus::BadBranchTarget {
                assert_eq!(state.registers, registers, "{name} committed a prefix");
            }
        }
    }
    let plan = compose(&[br(1), addi(0, 1)]);
    let mut state = MachineState {
        registers: vec![5; REGS],
        flags: vec![0],
    };
    assert_eq!(
        plan.execute(&mut state).unwrap(),
        ExecutionStatus::BadBranchTarget
    );
    assert_eq!(state.registers, vec![5; REGS]);
}

#[test]
fn envelope_v2_round_trips_and_versions_are_enforced() {
    let plan = compose(&programs()[0].1);
    let encoded = plan.encode_binary().unwrap();
    assert_eq!(&encoded[..8], b"FSLSEQ\0\x02");
    let decoded = FirSequence::decode_binary(&encoded).unwrap();
    assert_eq!(decoded, plan);
    assert!(decoded.has_guest_control());
    let mut as_v1 = encoded.clone();
    as_v1[7] = 1;
    assert!(FirSequence::decode_binary(&as_v1).is_err());

    // A sequence without guest PC bodies keeps the v1 magic and bytes' meaning.
    let plain = compose(&[addi(0, 1), addi(1, 2)]);
    assert!(!plain.has_guest_control());
    let plain_bytes = plain.encode_binary().unwrap();
    assert_eq!(&plain_bytes[..8], b"FSLSEQ\0\x01");
    let mut as_v2 = plain_bytes;
    as_v2[7] = 2;
    assert!(FirSequence::decode_binary(&as_v2).is_err());
}

#[test]
fn standalone_execution_and_emission_refuse_guest_pc_bodies() {
    let package = package();
    assert_eq!(package.version, 8);
    let plan = compose(&[br(0)]);
    let decoded = plan.instances()[0].decoded.clone();
    let mut state = MachineState {
        registers: vec![0; REGS],
        flags: vec![0],
    };
    assert!(fission_fsl::execute_decoded(&package, &decoded, &mut state).is_err());
    let branch = &package.instructions[decoded.instruction_index];
    assert!(emit_instruction(branch, OutputLayer::C, "fsl_standalone").is_err());
    assert!(emit_instruction(branch, OutputLayer::Rust, "fsl_standalone").is_err());
    // Diagnostic FIR is still available for review.
    assert!(emit_instruction(branch, OutputLayer::Fir, "ignored")
        .unwrap()
        .contains("GuestNextPcWrite"));
    // Older package versions cannot carry these ops.
    let mut older = package.clone();
    older.version = 7;
    assert!(older.validate().is_err());
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "fsl-guest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
fn c_and_rust_recompilation_match_reference_and_oracle_at_o0_and_o2() {
    let mut total = 0;
    for (name, words) in programs().into_iter().chain(bad_programs()) {
        let plan = FirSequence::decode_binary(&compose(&words).encode_binary().unwrap()).unwrap();
        let directory = Temp::new();
        let mut c = plan.emit(OutputLayer::C, "fsl_sequence").unwrap();
        let mut rust = plan.emit(OutputLayer::Rust, "fsl_sequence").unwrap();
        c.push_str("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) {uint64_t r[4],f[1];while(1){for(int i=0;i<4;++i) if(scanf(\"%\" SCNu64,&r[i])!=1) return 0; f[0]=1; uint32_t s=fsl_sequence(r,4,f,1); printf(\"%u\",s); for(int i=0;i<4;++i) printf(\" %\" PRIu64,r[i]); printf(\" %\" PRIu64 \"\\n\",f[0]);}}\n");
        rust.push_str("\nfn main(){use std::io::Read;let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();let n=input.split_whitespace().map(|v|v.parse::<u64>().unwrap()).collect::<Vec<_>>();for row in n.chunks_exact(4){let mut r=[row[0],row[1],row[2],row[3]];let mut f=[1u64];let s=fsl_sequence(&mut r,&mut f);println!(\"{} {} {} {} {} {}\",s,r[0],r[1],r[2],r[3],f[0]);}}\n");
        fs::write(directory.0.join("out.c"), c).unwrap();
        fs::write(directory.0.join("out.rs"), rust).unwrap();
        let mut input = fs::File::create(directory.0.join("input")).unwrap();
        let mut expected = String::new();
        let rows = states();
        for registers in &rows {
            writeln!(
                input,
                "{}",
                registers
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .unwrap();
            let mut state = MachineState {
                registers: registers.clone(),
                flags: vec![1],
            };
            let status = plan.execute(&mut state).unwrap() as u32;
            let (oracle_status, oracle_registers) = oracle(&words, registers);
            assert_eq!(status, oracle_status, "{name}");
            if status == 0 {
                assert_eq!(state.registers, oracle_registers, "{name}");
            }
            expected.push_str(&format!(
                "{status} {} {}\n",
                state
                    .registers
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                state.flags[0]
            ));
        }
        drop(input);
        for level in ["0", "2"] {
            let exe = directory.0.join(format!("c{level}"));
            checked(
                Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
                    .args([
                        "-std=c11",
                        "-Wall",
                        "-Wextra",
                        "-Werror",
                        &format!("-O{level}"),
                    ])
                    .arg(directory.0.join("out.c"))
                    .arg("-o")
                    .arg(&exe),
            );
            let run = |exe: &PathBuf| {
                checked(Command::new(exe).stdin(Stdio::from(
                    fs::File::open(directory.0.join("input")).unwrap(),
                )))
            };
            assert_eq!(run(&exe), expected, "C O{level} {name}");
            let exe = directory.0.join(format!("rust{level}"));
            checked(
                Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                    .args([
                        "--edition=2021",
                        "-Dwarnings",
                        "-C",
                        &format!("opt-level={level}"),
                    ])
                    .arg(directory.0.join("out.rs"))
                    .arg("-o")
                    .arg(&exe),
            );
            assert_eq!(run(&exe), expected, "Rust O{level} {name}");
        }
        total += rows.len() * 4;
    }
    println!("guest control source gate: {total} C/Rust O0/O2 comparisons");
}
