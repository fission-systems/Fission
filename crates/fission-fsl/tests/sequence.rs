use fission_fsl::{
    compile_source,
    sequence::{FirSequence, SequenceStateContract},
    ExecutionStatus, FirOp, FslcPackage, MachineState, OutputLayer,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
const SCALAR: &str = include_str!("../specs/amdgcn-gfx900-scalar-sequence.fsl");
const CFG: &str = include_str!("../specs/register-branch-convert.fsl");
fn scalar() -> FslcPackage {
    compile_source(SCALAR).unwrap()
}
fn compose(package: &FslcPackage, bytes: &[u8], regs: usize, flags: usize) -> FirSequence {
    FirSequence::compose(
        package,
        &package.language,
        0x1000,
        bytes,
        SequenceStateContract {
            register_count: regs,
            flag_count: flags,
        },
    )
    .unwrap()
}
fn chain_bytes() -> Vec<u8> {
    vec![0, 2, 4, 0x80, 1, 3, 5, 0x82]
}
#[test]
fn sequence_serialization_derives_origins_and_refuses_mismatched_inputs() {
    let package = scalar();
    let plan = compose(&package, &chain_bytes(), 7, 2);
    assert_eq!(plan.instances().len(), 2);
    assert_eq!(plan.instances()[0].origin.address, 0x1000);
    assert_eq!(plan.instances()[1].origin.address, 0x1004);
    assert_eq!(plan.instances()[1].origin.input_offset, 4);
    assert_eq!(plan.instances()[1].origin.byte_length, 4);
    assert_eq!(plan.instances()[1].decoded.raw, [1, 3, 5, 0x82]);
    assert_eq!(plan.instances()[1].decoded.instruction_index, 1);
    let mut big_package = package.clone();
    big_package.byte_order = fission_fsl::ByteOrder::Big;
    let big_bytes: Vec<u8> = chain_bytes()
        .chunks_exact(4)
        .flat_map(|word| word.iter().rev().copied())
        .collect();
    let big_plan = compose(&big_package, &big_bytes, 7, 2);
    assert_eq!(
        big_plan.instances()[1].decoded.fields,
        plan.instances()[1].decoded.fields
    );
    assert_eq!(big_plan.instances()[1].decoded.raw, [0x82, 5, 3, 1]);
    let mut little_state = MachineState {
        registers: vec![u32::MAX as u64, 0, 1, 0, 99, 99, 77],
        flags: vec![1, 1],
    };
    let mut big_state = little_state.clone();
    plan.execute(&mut little_state).unwrap();
    big_plan.execute(&mut big_state).unwrap();
    assert_eq!(big_state, little_state);
    assert_eq!(
        FirSequence::decode_binary(&big_plan.encode_binary().unwrap()).unwrap(),
        big_plan
    );
    let encoded = plan.encode_binary().unwrap();
    let restored = FirSequence::decode_binary(&encoded).unwrap();
    assert_eq!(plan, restored);
    assert_eq!(restored.encode_binary().unwrap(), encoded);
    for end in 0..encoded.len() {
        assert!(FirSequence::decode_binary(&encoded[..end]).is_err());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(FirSequence::decode_binary(&trailing).is_err());
    for index in [0, 7, 24, 28, 32, 64, 96, encoded.len() - 1] {
        let mut bad = encoded.clone();
        bad[index] ^= 0xff;
        assert!(FirSequence::decode_binary(&bad).is_err(), "index={index}");
    }
    let mut overflow = encoded.clone();
    overflow[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(FirSequence::decode_binary(&overflow).is_err());
    for (bytes, regs, flags, base, profile) in [
        (vec![], 7, 2, 0x1000, package.language.as_str()),
        (vec![0, 2, 4], 7, 2, 0x1000, package.language.as_str()),
        (
            vec![0, 2, 4, 0x80, 1, 3, 5, 0xff],
            7,
            2,
            0x1000,
            package.language.as_str(),
        ),
        (
            vec![0, 2, 4, 0x80, 1, 3, 95, 0x82],
            7,
            2,
            0x1000,
            package.language.as_str(),
        ),
        (chain_bytes(), 7, 0, 0x1000, package.language.as_str()),
        (chain_bytes(), 4097, 2, 0x1000, package.language.as_str()),
        (chain_bytes(), 7, 2, u64::MAX, package.language.as_str()),
        (chain_bytes(), 7, 2, 0x1000, "foreign"),
    ] {
        assert!(FirSequence::compose(
            &package,
            profile,
            base,
            &bytes,
            SequenceStateContract {
                register_count: regs,
                flag_count: flags
            }
        )
        .is_err());
    }
    let many = [0, 2, 4, 0x80].repeat(4096);
    assert_eq!(compose(&package, &many, 7, 2).instances().len(), 4096);
    assert!(FirSequence::compose(
        &package,
        &package.language,
        0,
        &[0, 2, 4, 0x80].repeat(4097),
        SequenceStateContract {
            register_count: 7,
            flag_count: 2
        }
    )
    .is_err());
    let mut unsupported = package.clone();
    unsupported.instructions[1].ops = vec![FirOp::Unsupported];
    unsupported.instructions[1].values.clear();
    unsupported.instructions[1].blocks.clear();
    unsupported.validate().unwrap();
    assert!(FirSequence::compose(
        &unsupported,
        &unsupported.language,
        0,
        &chain_bytes(),
        SequenceStateContract {
            register_count: 7,
            flag_count: 2
        }
    )
    .is_err());
    // Selected cyclic bodies and lane/stack bodies are refused at composition.
    let stack = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    assert!(FirSequence::compose(
        &stack,
        &stack.language,
        0,
        &[0x60],
        SequenceStateContract {
            register_count: 7,
            flag_count: 2
        }
    )
    .is_err());
    let cycle_source = CFG.replace("return;", "branch join(%joined);");
    let cycle = compile_source(&cycle_source).unwrap();
    assert!(FirSequence::compose(
        &cycle,
        &cycle.language,
        0,
        &[0x10, 0xa2, 0, 0],
        SequenceStateContract {
            register_count: 3,
            flag_count: 3
        }
    )
    .is_err());
    let mut different = package.clone();
    different.instructions[0].evidence[0]
        .claim
        .push_str(" edited");
    let changed = compose(&different, &chain_bytes(), 7, 2);
    assert_ne!(plan.package_sha256(), changed.package_sha256());
    assert_eq!(plan.input_sha256(), changed.input_sha256());
    let text = plan.emit(OutputLayer::Fir, "ignored").unwrap();
    assert!(text.contains("address=0x1004 offset=4 length=4 raw=01030582 body=1"));
    assert!(text.contains("evidence="));
    for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
        assert!(plan.emit(layer, "fsl_chain").is_err());
    }
    assert!(plan.emit(OutputLayer::C, "bad_symbol").is_err());
}
#[test]
fn state_is_threaded_through_two_actual_scalar_bodies_with_independent_u128_oracle() {
    let plan = compose(&scalar(), &chain_bytes(), 7, 2);
    let boundaries = [0, 1, u32::MAX as u64, 1u64 << 32, u64::MAX, u64::MAX >> 1];
    let mut inputs = vec![];
    for &a in &boundaries {
        for &b in &boundaries {
            inputs.push((a, b));
        }
    }
    let mut seed = 0x78cffe66bd029123u64;
    for _ in 0..1024 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let a = seed;
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        inputs.push((a, seed));
    }
    for (a, b) in &inputs {
        let mut state = MachineState {
            registers: vec![a & 0xffffffff, a >> 32, b & 0xffffffff, b >> 32, 99, 99, 77],
            flags: vec![1, 1],
        };
        assert_eq!(plan.execute(&mut state).unwrap(), ExecutionStatus::Success);
        let sum = u128::from(*a) + u128::from(*b);
        assert_eq!(state.registers[4] | (state.registers[5] << 32), sum as u64);
        assert_eq!(state.flags, [(sum >> 64) as u64, 1]);
        assert_eq!(state.registers[6], 77);
    }
    for (r, f) in [
        (vec![9; 6], vec![0, 1]),
        (vec![9; 8], vec![0, 1]),
        (vec![9; 7], vec![0]),
        (vec![9; 7], vec![0, 1, 0]),
        (vec![9; 7], vec![0, 2]),
    ] {
        let mut state = MachineState {
            registers: r,
            flags: f,
        };
        let saved = state.clone();
        assert_eq!(
            plan.execute(&mut state).unwrap(),
            ExecutionStatus::InvalidState
        );
        assert_eq!(state, saved);
    }
    println!(
        "sequence arithmetic oracle: {} two-instruction states; exact bank/failure preservation",
        inputs.len()
    );
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "fsl-sequence-{}-{}",
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
fn compile_run(directory: &Temp, expected: &str) {
    for level in ["0", "2"] {
        let c = directory.0.join(format!("c-{level}"));
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
                .arg(&c),
        );
        assert_eq!(
            checked(Command::new(&c).stdin(Stdio::from(
                fs::File::open(directory.0.join("input")).unwrap()
            ))),
            expected,
            "C O{level}"
        );
        let rust = directory.0.join(format!("rust-{level}"));
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
                .arg(&rust),
        );
        assert_eq!(
            checked(Command::new(&rust).stdin(Stdio::from(
                fs::File::open(directory.0.join("input")).unwrap()
            ))),
            expected,
            "Rust O{level}"
        );
    }
}
fn add_row(
    input: &mut fs::File,
    expected: &mut String,
    plan: &FirSequence,
    r: &[u64],
    f: &[u64],
    expected_state: MachineState,
    status: u32,
) {
    let mut state = MachineState {
        registers: r.to_vec(),
        flags: f.to_vec(),
    };
    assert_eq!(plan.execute(&mut state).unwrap() as u32, status);
    assert_eq!(state, expected_state);
    let mut registers = [0xdead; 7];
    let mut flags = [0xbeef; 3];
    registers[..r.len()].copy_from_slice(r);
    flags[..f.len()].copy_from_slice(f);
    write!(input, "{} {}", r.len(), f.len()).unwrap();
    for value in registers.iter().chain(flags.iter()) {
        write!(input, " {value}").unwrap();
    }
    writeln!(input).unwrap();
    registers[..expected_state.registers.len()].copy_from_slice(&expected_state.registers);
    flags[..expected_state.flags.len()].copy_from_slice(&expected_state.flags);
    expected.push_str(&status.to_string());
    for value in registers.iter().chain(flags.iter()) {
        expected.push_str(&format!(" {value}"));
    }
    expected.push('\n');
}
fn validate_source(
    plan: &FirSequence,
    cases: Vec<(Vec<u64>, Vec<u64>, MachineState, u32)>,
    label: &str,
) {
    let directory = Temp::new();
    let mut c = plan.emit(OutputLayer::C, "fsl_sequence").unwrap();
    let mut rust = plan.emit(OutputLayer::Rust, "fsl_sequence").unwrap();
    c.push_str("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) {size_t nr,nf;uint64_t r[7],f[3];while(scanf(\"%zu %zu\",&nr,&nf)==2) {for(size_t i=0;i<7;++i) if(scanf(\"%\" SCNu64,&r[i])!=1)return 4;for(size_t i=0;i<3;++i) if(scanf(\"%\" SCNu64,&f[i])!=1)return 4;uint32_t status=fsl_sequence(r,nr,f,nf);printf(\"%u\",status);for(size_t i=0;i<7;++i)printf(\" %\" PRIu64,r[i]);for(size_t i=0;i<3;++i)printf(\" %\" PRIu64,f[i]);puts(\"\");}return 0;}\n");
    rust.push_str("\nfn main(){use std::io::Read;let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();let numbers=input.split_whitespace().map(|v|v.parse::<u64>().unwrap()).collect::<Vec<_>>();for row in numbers.chunks_exact(12){let mut r=[row[2],row[3],row[4],row[5],row[6],row[7],row[8]];let mut f=[row[9],row[10],row[11]];let status=fsl_sequence(&mut r[..row[0] as usize],&mut f[..row[1] as usize]);print!(\"{}\",status);for value in r.iter().chain(f.iter()){print!(\" {}\",value);}println!();}}\n");
    fs::write(directory.0.join("out.c"), c).unwrap();
    fs::write(directory.0.join("out.rs"), rust).unwrap();
    let mut input = fs::File::create(directory.0.join("input")).unwrap();
    let mut expected = String::new();
    for (r, f, oracle, status) in &cases {
        add_row(
            &mut input,
            &mut expected,
            plan,
            r,
            f,
            oracle.clone(),
            *status,
        );
    }
    drop(input);
    compile_run(&directory, &expected);
    println!(
        "sequence source gate {label}: {} reference/oracle cases; {} C/Rust O0/O2 comparisons",
        cases.len(),
        cases.len() * 4
    );
}
#[test]
fn scalar_sequence_c_rust_recompilation_preserves_carry_and_aliases() {
    let mut total = 0;
    for bits in [1, 8, 16, 32, 64] {
        let package = compile_source(&SCALAR.replace("u32", &format!("u{bits}"))).unwrap();
        for aliases in [false, true] {
            let bytes = if aliases {
                vec![0, 2, 0, 0x80, 1, 3, 1, 0x82]
            } else {
                chain_bytes()
            };
            let plan = compose(&package, &bytes, 7, 2);
            let plan = FirSequence::decode_binary(&plan.encode_binary().unwrap()).unwrap();
            let modulus = 1u128 << bits;
            let mut seed = 0xd454b0a18389e81fu64;
            let mut cases = vec![];
            for case in 0..128 {
                let mut r = vec![0; 7];
                for value in &mut r {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    *value = seed;
                }
                if case < 8 {
                    let mask = (modulus - 1) as u64;
                    let v = [
                        0,
                        1,
                        mask,
                        mask >> 1,
                        1u64 << (bits - 1),
                        u64::MAX,
                        0xffff_ffff,
                        0x1_0000_0000,
                    ][case];
                    r[0] = v;
                    r[2] = if case.is_multiple_of(2) { v } else { 1 };
                    r[1] = v;
                    r[3] = v;
                }
                let f = vec![(case % 2) as u64, 1];
                let mut oracle = MachineState {
                    registers: r.clone(),
                    flags: f.clone(),
                };
                let low = (u128::from(r[0]) % modulus) + (u128::from(r[2]) % modulus);
                let high =
                    (u128::from(r[1]) % modulus) + (u128::from(r[3]) % modulus) + (low / modulus);
                oracle.registers[if aliases { 0 } else { 4 }] = (low % modulus) as u64;
                oracle.registers[if aliases { 1 } else { 5 }] = (high % modulus) as u64;
                oracle.flags[0] = (high / modulus) as u64;
                cases.push((r, f, oracle, 0));
            }
            for (nr, nf, badflag) in [(6, 2, 0), (7, 1, 0), (7, 3, 0), (7, 2, 2)] {
                let r = vec![17; nr];
                let mut f = vec![1; nf];
                f[0] = badflag;
                cases.push((
                    r.clone(),
                    f.clone(),
                    MachineState {
                        registers: r,
                        flags: f,
                    },
                    3,
                ));
            }
            total += cases.len();
            validate_source(
                &plan,
                cases,
                &format!("scalar-width={bits} aliases={aliases}"),
            );
        }
    }
    println!(
        "sequence scalar total: {total} states, {} C/Rust comparisons",
        total * 4
    );
}
#[test]
fn structured_bodies_share_state_and_keep_origins_in_sequence_cli() {
    let package = compile_source(CFG).unwrap();
    let bytes = [0x10, 0xa2, 0, 0, 0x21, 0xa0, 0, 0, 0x02, 0xa1, 0, 0];
    let plan = compose(&package, &bytes, 3, 3);
    let mut cases = vec![];
    for value in 0..256 {
        let r = vec![value | 0xabcd_ef00, 77, 88];
        let f = vec![1, 0, 1];
        let mut oracle = MachineState {
            registers: r.clone(),
            flags: f.clone(),
        };
        for (src, dst, aux) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
            let raw = oracle.registers[src] % 256;
            let signed = if raw < 128 {
                raw as i128
            } else {
                raw as i128 - 256
            };
            oracle.registers[aux] = raw;
            oracle.registers[dst] = (signed as u128 & 0xffffffff) as u64;
            oracle.flags = vec![u64::from(signed < 0), 1, u64::from(signed < 0)];
        }
        cases.push((r, f, oracle, 0));
    }
    for (r, f) in [
        (vec![5; 2], vec![0; 3]),
        (vec![5; 3], vec![0; 2]),
        (vec![5; 3], vec![0, 2, 0]),
    ] {
        cases.push((
            r.clone(),
            f.clone(),
            MachineState {
                registers: r,
                flags: f,
            },
            3,
        ));
    }
    validate_source(&plan, cases, "three structured bodies");
    let directory = Temp::new();
    let p = directory.0.join("plan.fslseq");
    fs::write(&p, plan.encode_binary().unwrap()).unwrap();
    let output = checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
        "execute-sequence",
        p.to_str().unwrap(),
        "128,77,88",
        "0,0,0",
    ]));
    assert_eq!(
        output.trim(),
        "status=Success registers=[4294967168, 128, 4294967168] flags=[1, 1, 1]"
    );
    let text = checked(
        Command::new(env!("CARGO_BIN_EXE_fslc")).args(["inspect-sequence", p.to_str().unwrap()]),
    );
    assert!(text.contains("step=2 address=0x1008 offset=8 length=4"));
    let package_path = directory.0.join("profile.fslc");
    let assembled = directory.0.join("assembled.fslseq");
    fs::write(&package_path, package.encode_binary().unwrap()).unwrap();
    checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
        "compose-sequence",
        package_path.to_str().unwrap(),
        &package.language,
        "0x1000",
        "3",
        "3",
        "10a2000021a0000002a10000",
        assembled.to_str().unwrap(),
    ]));
    assert_eq!(fs::read(&assembled).unwrap(), fs::read(&p).unwrap());
}
#[test]
fn empty_register_or_flag_banks_are_explicit_source_contracts() {
    for (body, raw, nr, nf, expected) in [
        (
            "%v: u1 = flag.read 0; flag.write 0, %v;",
            vec![0x71],
            0,
            1,
            MachineState {
                registers: vec![],
                flags: vec![1],
            },
        ),
        (
            "%v: u8 = register.read destination; register.write destination, %v;",
            vec![0, 0xa0, 0, 0],
            1,
            0,
            MachineState {
                registers: vec![255],
                flags: vec![],
            },
        ),
    ] {
        let encoding = if nr == 0 {
            "opcode 0x71;"
        } else {
            "encoding 32 mask 0xfffffff0 value 0xa000 {field destination offset 0 bits 4;}"
        };
        let source=format!("language zero {{byte_order little; address_unit byte; instruction body {{{encoding} mnemonic \"fixture\"; evidence \"self\" \"synthetic\" \"v1\" \"Zero bank\"; semantics {{{body}}} }} }}");
        let package = compile_source(&source).unwrap();
        let plan = compose(&package, &raw, nr, nf);
        let r = if nr == 0 { vec![] } else { vec![u64::MAX] };
        let f = if nf == 0 { vec![] } else { vec![1] };
        validate_source(&plan, vec![(r, f, expected, 0)], "explicit zero bank");
    }
}
