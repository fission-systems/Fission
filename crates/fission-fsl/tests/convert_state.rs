use fission_fsl::{
    compile_source, emit_instruction, execute_decoded, execute_instruction, ExecutionStatus, FirOp,
    FslcPackage, JitDecoder, MachineState, OutputLayer,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
const STATE: &str = include_str!("../specs/register-branch-convert.fsl");
fn profile(body: &str) -> FslcPackage {
    compile_source(&format!("language convert {{ byte_order little; address_unit byte; instruction fixture {{ opcode 0x71; mnemonic \"fixture\"; evidence \"self-authored\" \"synthetic\" \"v1\" \"Conversion contract fixture\"; semantics {{ {body} }} }} }}")).unwrap()
}
fn mask(bits: u16) -> u64 {
    ((1u128 << bits) - 1) as u64
}
fn oracle(raw: u64, source: u16, target: u16, op: &str) -> u64 {
    let raw = raw & mask(source);
    let number = if op == "sext" && raw & (1u64 << (source - 1)) != 0 {
        i128::from(raw) - (1i128 << source)
    } else {
        i128::from(raw)
    };
    (number as u128 & u128::from(mask(target))) as u64
}
fn conversion(source: u16, target: u16, sign: &str, op: &str) -> FslcPackage {
    profile(&format!("%input: {sign}{source} = stack.pop; %converted: {sign}{target} = int.{op} %input; stack.push %converted;"))
}
#[test]
fn v7_round_trip_and_strict_conversion_validation() {
    let package = compile_source(STATE).unwrap();
    assert_eq!(package.version, 7);
    let bytes = package.encode_binary().unwrap();
    assert_eq!(FslcPackage::decode_binary(&bytes).unwrap(), package);
    for end in 0..bytes.len() {
        assert!(FslcPackage::decode_binary(&bytes[..end]).is_err());
    }
    for version in 1..7 {
        let mut bad = package.clone();
        bad.version = version;
        assert!(bad.encode_binary().is_err());
        let mut bad = bytes.clone();
        bad[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(FslcPackage::decode_binary(&bad).is_err());
    }
    // Previous structured encoding remains v6 and byte stable.
    let v6 = compile_source(include_str!("../specs/stack-branch-join.fsl")).unwrap();
    assert_eq!(v6.version, 6);
    assert_eq!(
        v6.encode_binary().unwrap(),
        FslcPackage::decode_binary(&v6.encode_binary().unwrap())
            .unwrap()
            .encode_binary()
            .unwrap()
    );
    for (from, to, op) in [
        ("u8", "u8", "zext"),
        ("u16", "u8", "zext"),
        ("i8", "i16", "zext"),
        ("u8", "u16", "sext"),
        ("i8", "u16", "sext"),
        ("u8", "u16", "trunc"),
        ("i16", "u8", "trunc"),
        ("u64", "u65", "zext"),
    ] {
        let source = format!("language bad {{ byte_order little; address_unit byte; instruction bad {{ opcode 1; mnemonic \"bad\"; evidence \"self\" \"synthetic\" \"v1\" \"bad\"; semantics {{ %x: {from} = stack.pop; %y: {to} = int.{op} %x; stack.push %y; }} }} }}");
        assert!(compile_source(&source).is_err(), "{from} {to} {op}");
    }
    let mut bad = conversion(8, 32, "u", "zext");
    if let FirOp::IntConvert { input, .. } = &mut bad.instructions[0].ops[1] {
        input.0 = 65535;
    }
    assert!(bad.validate().is_err());
    let package = conversion(8, 32, "u", "zext");
    assert!(JitDecoder::compile(&package).is_err());
    assert!(fission_fsl::emit_aot_object(&package).is_err());
    for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
        assert!(emit_instruction(&package.instructions[0], layer, "fsl_bad").is_err());
    }
    let mut empty = vec![];
    assert_eq!(
        execute_instruction(&package.instructions[0], &mut empty, 1).unwrap(),
        ExecutionStatus::StackUnderflow
    );
    assert!(empty.is_empty());
}
fn decode(package: &FslcPackage, fields: [u64; 3]) -> fission_fsl::DecodedInstruction {
    let word = 0xa000 | fields[0] | (fields[1] << 4) | (fields[2] << 8);
    package
        .decode_bytes(&package.language, &(word as u32).to_le_bytes())
        .unwrap()
        .unwrap()
}
fn state_oracle(mut registers: Vec<u64>, fields: [u64; 3]) -> MachineState {
    let raw = registers[fields[0] as usize] % 256;
    let signed = if raw >= 128 {
        i128::from(raw) - 256
    } else {
        i128::from(raw)
    };
    registers[fields[2] as usize] = raw;
    registers[fields[1] as usize] = (signed as u128 & 0xffff_ffff) as u64;
    MachineState {
        registers,
        flags: vec![u64::from(signed < 0), 1, u64::from(signed < 0)],
    }
}
#[test]
fn register_state_cfg_exhaustive_aliasing_and_failure_preservation() {
    let package = compile_source(STATE).unwrap();
    for raw in 0..256 {
        for source in 0..3 {
            for destination in 0..3 {
                for auxiliary in 0..3 {
                    let fields = [source, destination, auxiliary];
                    let mut state = MachineState {
                        registers: vec![0x8000_0000_0000_0081, 33, 129],
                        flags: vec![1, 0, 1],
                    };
                    state.registers[source as usize] = !255u64 | raw;
                    let expected = state_oracle(state.registers.clone(), fields);
                    assert_eq!(
                        execute_decoded(&package, &decode(&package, fields), &mut state).unwrap(),
                        ExecutionStatus::Success
                    );
                    assert_eq!(state, expected);
                }
            }
        }
    }
    for (registers, flags, fields) in failure_cases() {
        let mut state = MachineState { registers, flags };
        let saved = state.clone();
        assert_eq!(
            execute_decoded(&package, &decode(&package, fields), &mut state).unwrap(),
            ExecutionStatus::InvalidState
        );
        assert_eq!(state, saved);
    }
    // Invalid selector in an untaken branch still refuses before entry writes.
    let untaken = compile_source(&STATE.replace(
        "%negative: u1 = int.slt %wide, %zero;",
        "%negative: u1 = int.const 0;",
    ))
    .unwrap();
    let mut state = MachineState {
        registers: vec![1, 2, 3],
        flags: vec![0, 1, 0],
    };
    let saved = state.clone();
    assert_eq!(
        execute_decoded(&untaken, &decode(&untaken, [0, 15, 1]), &mut state).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(state, saved);
    // Loops are retained in FIR but refused before any state mutation.
    let cycle = profile("block entry() { %on: u1 = flag.read 0; branch again(%on); } block again(%value: u1) { flag.write 0, %value; branch again(%value); }");
    let decoded = cycle.decode_bytes("convert", &[0x71]).unwrap().unwrap();
    assert!(execute_decoded(&cycle, &decoded, &mut state).is_err());
    assert_eq!(state, saved);
    assert!(emit_instruction(&cycle.instructions[0], OutputLayer::C, "fsl_cycle").is_err());
    assert!(emit_instruction(&cycle.instructions[0], OutputLayer::Fir, "ignored").is_ok());
    // Decoded observation tampering also refuses before effects.
    let mut bad = decode(&package, [0, 1, 2]);
    bad.fields[0].1 = 1;
    assert!(execute_decoded(&package, &bad, &mut state).is_err());
    assert_eq!(state, saved);
    println!("register CFG oracle: 6912 exhaustive alias states; invalid state and cyclic CFG preserve storage");
}
fn failure_cases() -> Vec<(Vec<u64>, Vec<u64>, [u64; 3])> {
    vec![
        (vec![9, 10, 11], vec![0, 0, 0], [15, 1, 2]),
        (vec![9, 10, 11], vec![0, 0, 0], [0, 15, 2]),
        (vec![9, 10, 11], vec![0, 0, 0], [0, 1, 15]),
        (vec![], vec![0, 0, 0], [0, 1, 2]),
        (vec![9, 10, 11], vec![], [0, 1, 2]),
        (vec![9, 10, 11], vec![0, 0], [0, 1, 2]),
        (vec![9, 10, 11], vec![0, 2, 0], [0, 1, 2]),
        (vec![9, 10, 11], vec![0, 0, u64::MAX], [0, 1, 2]),
    ]
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "fsl-convert-{}-{}",
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
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn compile_run(directory: &Temp, input: &std::path::Path, expected: &str) {
    for level in ["0", "2"] {
        let c_exe = directory.0.join(format!("c-{level}"));
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
                .arg(&c_exe),
        );
        assert_eq!(
            checked(Command::new(&c_exe).stdin(Stdio::from(fs::File::open(input).unwrap()))),
            expected,
            "C O{level}"
        );
        let rust_exe = directory.0.join(format!("rust-{level}"));
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
                .arg(&rust_exe),
        );
        assert_eq!(
            checked(Command::new(&rust_exe).stdin(Stdio::from(fs::File::open(input).unwrap()))),
            expected,
            "Rust O{level}"
        );
    }
}
#[test]
fn conversions_recompile_at_o0_o2_with_independent_widened_oracles() {
    let directory = Temp::new();
    let mut profiles = vec![];
    for (from, to) in [(1, 8), (7, 9), (8, 16), (16, 32), (32, 64), (63, 64)] {
        for (source, target, sign, op) in [
            (from, to, "u", "zext"),
            (from, to, "i", "sext"),
            (to, from, "u", "trunc"),
            (to, from, "i", "trunc"),
        ] {
            let package = conversion(source, target, sign, op);
            let restored = FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap();
            profiles.push((restored.instructions[0].clone(), source, target, op));
        }
    }
    let mut c = String::new();
    let mut rust = String::new();
    for (i, (instruction, ..)) in profiles.iter().enumerate() {
        c.push_str(
            &emit_instruction(instruction, OutputLayer::C, &format!("fsl_case_{i}")).unwrap(),
        );
        rust.push_str(
            &emit_instruction(instruction, OutputLayer::Rust, &format!("fsl_case_{i}")).unwrap(),
        );
    }
    let symbols = (0..profiles.len())
        .map(|i| format!("fsl_case_{i}"))
        .collect::<Vec<_>>()
        .join(",");
    c.push_str(&format!("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) {{ uint32_t (*functions[])(uint64_t*,size_t*,size_t)={{{symbols}}}; unsigned index; uint64_t x; while(scanf(\"%u %\" SCNu64,&index,&x)==2) {{ uint64_t stack[1]={{x}}; size_t depth=1; uint32_t status=functions[index](stack,&depth,1); printf(\"%u %zu %\" PRIu64 \"\\n\",status,depth,stack[0]); }} return 0; }}"));
    rust.push_str(&format!("\nfn main() {{ use std::io::Read; let functions: &[fn(&mut[u64],&mut usize)->u32]=&[{symbols}]; let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap(); let numbers=input.split_whitespace().map(|s|s.parse::<u64>().unwrap()).collect::<Vec<_>>(); for row in numbers.chunks_exact(2) {{ let mut stack=[row[1]]; let mut depth=1; let status=functions[row[0] as usize](&mut stack,&mut depth); println!(\"{{}} {{}} {{}}\",status,depth,stack[0]); }} }}"));
    fs::write(directory.0.join("out.c"), c).unwrap();
    fs::write(directory.0.join("out.rs"), rust).unwrap();
    let path = directory.0.join("input");
    let mut input = fs::File::create(&path).unwrap();
    let mut expected = String::new();
    let mut seed = 0x197362abcd389876u64;
    for (i, (instruction, source, target, op)) in profiles.iter().enumerate() {
        for case in 0..128 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let raw = match case {
                0 => 0,
                1 => 1,
                2 => mask(*source),
                3 => mask(*source) >> 1,
                4 => 1u64 << (source - 1),
                5 => u64::MAX,
                _ => seed,
            };
            let result = oracle(raw, *source, *target, op);
            let mut stack = vec![raw];
            assert_eq!(
                execute_instruction(instruction, &mut stack, 1).unwrap(),
                ExecutionStatus::Success
            );
            assert_eq!(stack, [result]);
            writeln!(input, "{i} {raw}").unwrap();
            expected.push_str(&format!("0 1 {result}\n"));
        }
    }
    drop(input);
    compile_run(&directory, &path, &expected);
    println!("conversion gate: 24 profiles, 3072 independent/reference states, 12288 C/Rust O0/O2 comparisons");
}
#[test]
fn register_cfg_recompiles_and_cli_runs_same_packaged_fir() {
    let directory = Temp::new();
    let package = compile_source(STATE).unwrap();
    let instruction = &package.instructions[0];
    let mut c = emit_instruction(instruction, OutputLayer::C, "fsl_case").unwrap();
    let mut rust = emit_instruction(instruction, OutputLayer::Rust, "fsl_case").unwrap();
    c.push_str("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) { size_t nr,nf; uint64_t r[3],f[3],fields[3]; while(scanf(\"%zu %zu %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64,&nr,&nf,&r[0],&r[1],&r[2],&f[0],&f[1],&f[2],&fields[0],&fields[1],&fields[2])==11) { uint32_t status=fsl_case(r,nr,f,nf,fields,3); printf(\"%u\",status); for(size_t i=0;i<3;++i) printf(\" %\" PRIu64,r[i]); for(size_t i=0;i<3;++i) printf(\" %\" PRIu64,f[i]); puts(\"\"); } return 0; }");
    rust.push_str("\nfn main() { use std::io::Read; let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap(); let numbers=input.split_whitespace().map(|s|s.parse::<u64>().unwrap()).collect::<Vec<_>>(); for row in numbers.chunks_exact(11) { let mut r=[row[2],row[3],row[4]]; let mut f=[row[5],row[6],row[7]]; let fields=[row[8],row[9],row[10]]; let status=fsl_case(&mut r[..row[0] as usize],&mut f[..row[1] as usize],&fields); print!(\"{}\",status); for value in r.iter().chain(f.iter()) { print!(\" {}\",value); } println!(); } }");
    fs::write(directory.0.join("out.c"), c).unwrap();
    fs::write(directory.0.join("out.rs"), rust).unwrap();
    let path = directory.0.join("input");
    let mut input = fs::File::create(&path).unwrap();
    let mut expected = String::new();
    let mut cases = vec![];
    for raw in 0..256 {
        for src in 0..3 {
            for dst in 0..3 {
                for aux in 0..3 {
                    let mut r = vec![u64::MAX, 77, 128];
                    r[src as usize] = raw | !255u64;
                    cases.push((r, vec![1, 0, 1], [src, dst, aux]));
                }
            }
        }
    }
    cases.extend(failure_cases());
    for (registers, flags, fields) in &cases {
        let mut state = MachineState {
            registers: registers.clone(),
            flags: flags.clone(),
        };
        let valid = fields.iter().all(|&v| v < (registers.len() as u64))
            && flags.len() >= 3
            && flags.iter().all(|&v| v <= 1);
        let status = if valid { 0 } else { 3 };
        let oracle = if valid {
            state_oracle(registers.clone(), *fields)
        } else {
            state.clone()
        };
        assert_eq!(
            execute_decoded(&package, &decode(&package, *fields), &mut state).unwrap() as u32,
            status
        );
        assert_eq!(state, oracle);
        let mut r = [0xdead; 3];
        let mut f = [0xbeef; 3];
        r[..registers.len()].copy_from_slice(registers);
        f[..flags.len()].copy_from_slice(flags);
        writeln!(
            input,
            "{} {} {} {} {} {} {} {} {} {} {}",
            registers.len(),
            flags.len(),
            r[0],
            r[1],
            r[2],
            f[0],
            f[1],
            f[2],
            fields[0],
            fields[1],
            fields[2]
        )
        .unwrap();
        r[..oracle.registers.len()].copy_from_slice(&oracle.registers);
        f[..oracle.flags.len()].copy_from_slice(&oracle.flags);
        expected.push_str(&format!(
            "{status} {} {} {} {} {} {}\n",
            r[0], r[1], r[2], f[0], f[1], f[2]
        ));
    }
    drop(input);
    compile_run(&directory, &path, &expected);
    let binary = directory.0.join("state.fslc");
    fs::write(&binary, package.encode_binary().unwrap()).unwrap();
    for (value, expected) in [("127", "127"), ("128", "4294967168"), ("255", "4294967295")] {
        let output = checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
            "execute-state",
            binary.to_str().unwrap(),
            "register.branch.convert",
            "10a20000",
            &format!("{value},77,88"),
            "0,0,0",
        ]));
        let negative = u64::from(value != "127");
        assert_eq!(output.trim(),format!("status=Success registers=[{value}, {expected}, {value}] flags=[{negative}, 1, {negative}]"));
    }
    println!("register CFG recompilation gate: {} independent/reference states, {} C/Rust O0/O2 comparisons",cases.len(),cases.len()*4);
}
