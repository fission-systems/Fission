use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use fission_fsl::{
    compile_source, emit_instruction, execute_instruction, ExecutionStatus, FirOp, FslcPackage,
    JitDecoder, OutputLayer, ValueId,
};

fn profile(ty: &str, body: &str) -> fission_fsl::CompiledInstruction {
    let source = format!("language proof {{ byte_order big; address_unit byte; instruction sample {{ opcode 0x60; mnemonic \"sample\"; evidence \"fixture\" \"synthetic\" \"v1\" \"contract fixture\"; semantics {{ {} }} }} }}", body.replace("TYPE", ty));
    compile_source(&source).unwrap().instructions.remove(0)
}

const ADD: &str = "%rhs: TYPE = stack.pop; %lhs: TYPE = stack.pop; %sum: TYPE = TYPE.add.wrap %lhs, %rhs; stack.push %sum;";

#[test]
fn rejects_malformed_ssa_and_types_at_all_boundaries() {
    let mut package = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    let valid_bytes = package.encode_binary().unwrap();
    let mut malformed_bytes = valid_bytes.clone();
    // The final 10 bytes are wrapping-add (7) and push (3). Replace the
    // add's left input with its own not-yet-defined output id 2.
    let left = malformed_bytes.len() - 7;
    malformed_bytes[left..left + 2].copy_from_slice(&2u16.to_le_bytes());
    assert!(FslcPackage::decode_binary(&malformed_bytes).is_err());

    package.instructions[0].ops[2] = FirOp::IntAddWrap {
        output: ValueId(2),
        left: ValueId(2),
        right: ValueId(0),
    };
    assert!(package.encode_binary().is_err());
    assert!(JitDecoder::compile(&package).is_err());
    assert!(emit_instruction(&package.instructions[0], OutputLayer::C, "fsl_execute").is_err());

    let mut instruction = profile("i32", ADD);
    instruction.values[0].ty.bits = 16;
    assert!(instruction.validate().is_err());
    instruction = profile("i32", ADD);
    instruction.ops[1] = FirOp::VmStackPop { output: ValueId(0) };
    assert!(instruction.validate().is_err());
    assert_eq!(
        FslcPackage::decode_binary(&valid_bytes)
            .unwrap()
            .encode_binary()
            .unwrap(),
        valid_bytes
    );
}

#[test]
fn evaluator_matches_jvm_integer_wrap_and_preserves_failure_state() {
    let instruction = profile("i32", ADD);
    for (left, right, expected) in [
        (0x7fff_ffff, 1, 0x8000_0000),
        (0xffff_ffff, 1, 0),
        (0x8000_0000, 0xffff_ffff, 0x7fff_ffff),
    ] {
        let mut stack = vec![123, left, right];
        assert_eq!(
            execute_instruction(&instruction, &mut stack, 3).unwrap(),
            ExecutionStatus::Success
        );
        assert_eq!(stack, vec![123, expected]);
    }
    let mut stack = vec![9];
    assert_eq!(
        execute_instruction(&instruction, &mut stack, 1).unwrap(),
        ExecutionStatus::StackUnderflow
    );
    assert_eq!(stack, vec![9]);
    let duplicate = profile("u32", "%x: TYPE = stack.pop; stack.push %x; stack.push %x;");
    assert_eq!(
        execute_instruction(&duplicate, &mut stack, 1).unwrap(),
        ExecutionStatus::CapacityExceeded
    );
    assert_eq!(stack, vec![9]);
    assert_eq!(
        execute_instruction(&duplicate, &mut stack, 0).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(stack, vec![9]);
}

#[test]
fn wider_fir_is_preserved_and_executable_outputs_reject_it() {
    let instruction = profile("u128", ADD);
    assert!(emit_instruction(&instruction, OutputLayer::Fir, "ignored")
        .unwrap()
        .contains("u128"));
    for layer in [OutputLayer::C, OutputLayer::Rust] {
        assert!(emit_instruction(&instruction, layer, "fsl_execute").is_err());
    }
    let mut stack = vec![1, 2];
    assert!(execute_instruction(&instruction, &mut stack, 2).is_err());
    assert_eq!(stack, vec![1, 2]);
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fission-fsl-recompile-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn checked(command: &mut Command) -> String {
    let output = command
        .output()
        .expect("compiler or executable must be available for the recompilation gate");
    assert!(
        output.status.success(),
        "command {command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run(executable: &Path, input: &Path) -> String {
    checked(Command::new(executable).stdin(Stdio::from(fs::File::open(input).unwrap())))
}

#[test]
fn c_and_rust_recompile_same_fir_and_match_state_at_two_optimization_levels() {
    let dir = TempDir::new();
    let mut instructions = Vec::new();
    for bits in [1, 8, 16, 32, 64] {
        for sign in ["i", "u"] {
            instructions.push(profile(&format!("{sign}{bits}"), ADD));
        }
    }
    // Include the source -> portable package -> opcode decode -> output path,
    // rather than validating only compiler-produced in-memory bodies.
    let jvm = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    let jvm = FslcPackage::decode_binary(&jvm.encode_binary().unwrap()).unwrap();
    instructions[6] = jvm.instruction_for_opcode(0x60).unwrap().clone();
    instructions.push(profile(
        "u32",
        "%x: TYPE = stack.pop; stack.push %x; stack.push %x;",
    ));
    instructions.push(profile(
        "u32",
        "%x: TYPE = stack.pop; stack.push %x; stack.push %x; %y: TYPE = stack.pop;",
    ));
    let originals = instructions.clone();
    let mut c = String::new();
    let mut rust = String::new();
    for (index, instruction) in instructions.iter().enumerate() {
        let symbol = format!("fsl_case_{index}");
        c.push_str(&emit_instruction(instruction, OutputLayer::C, &symbol).unwrap());
        rust.push_str(&emit_instruction(instruction, OutputLayer::Rust, &symbol).unwrap());
    }
    assert_eq!(
        originals, instructions,
        "output projections must not mutate FIR"
    );
    let symbols = (0..instructions.len())
        .map(|i| format!("fsl_case_{i}"))
        .collect::<Vec<_>>()
        .join(",");
    c.push_str(&format!("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) {{\n uint32_t (*functions[])(uint64_t *, size_t *, size_t) = {{{symbols}}};\n unsigned index; size_t capacity, depth; uint64_t stack[4];\n while (scanf(\"%u %zu %zu %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64, &index, &capacity, &depth, &stack[0], &stack[1], &stack[2], &stack[3]) == 7) {{\n  uint32_t status = functions[index](stack, &depth, capacity);\n  printf(\"%u %zu\", status, depth);\n  for (size_t i = 0; i < depth; ++i) printf(\" %\" PRIu64, stack[i]);\n  puts(\"\");\n }}\n return 0;\n}}\n"));
    rust.push_str(&format!("\nfn main() {{\n use std::io::Read;\n let functions: &[fn(&mut [u64], &mut usize) -> u32] = &[{symbols}];\n let mut input = String::new(); std::io::stdin().read_to_string(&mut input).unwrap();\n let numbers = input.split_whitespace().map(|v| v.parse::<u64>().unwrap()).collect::<Vec<_>>();\n for row in numbers.chunks_exact(7) {{\n  let mut stack = [row[3], row[4], row[5], row[6]]; let mut depth = row[2] as usize;\n  let status = functions[row[0] as usize](&mut stack[..row[1] as usize], &mut depth);\n  print!(\"{{}} {{}}\", status, depth);\n  for value in &stack[..depth] {{ print!(\" {{}}\", value); }} println!();\n }}\n}}\n"));
    fs::write(dir.0.join("out.c"), c).unwrap();
    fs::write(dir.0.join("out.rs"), rust).unwrap();
    let mut input = fs::File::create(dir.0.join("cases.txt")).unwrap();
    let mut expected = String::new();
    let mut seed = 0x91e1_0da5_5678_1234u64;
    for (index, instruction) in instructions.iter().enumerate() {
        for case in 0..160 {
            let depth = case % 5;
            let capacity = (case / 5) % 5;
            let mut values = [0u64; 4];
            for value in &mut values {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *value = match case % 8 {
                    0 => 0,
                    1 => u64::MAX,
                    2 => 0x7fff_ffff,
                    3 => 0x8000_0000,
                    4 => 1,
                    _ => seed,
                };
            }
            writeln!(
                input,
                "{index} {capacity} {depth} {} {} {} {}",
                values[0], values[1], values[2], values[3]
            )
            .unwrap();
            let mut stack = values[..depth].to_vec();
            let status = execute_instruction(instruction, &mut stack, capacity).unwrap();
            if index < 10 && status == ExecutionStatus::Success {
                let bits = instruction.values[0].ty.bits;
                let mask = (1u128 << bits) - 1;
                let oracle =
                    ((u128::from(values[depth - 2]) + u128::from(values[depth - 1])) & mask) as u64;
                assert_eq!(
                    *stack.last().unwrap(),
                    oracle,
                    "independent wide-integer modulo oracle"
                );
            }
            use std::fmt::Write as _;
            write!(expected, "{} {}", status as u32, stack.len()).unwrap();
            for value in stack {
                write!(expected, " {value}").unwrap();
            }
            expected.push('\n');
        }
    }
    drop(input);
    for level in ["0", "2"] {
        let c_exe = dir.0.join(format!("c-{level}"));
        checked(
            Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    &format!("-O{level}"),
                ])
                .arg(dir.0.join("out.c"))
                .arg("-o")
                .arg(&c_exe),
        );
        assert_eq!(
            run(&c_exe, &dir.0.join("cases.txt")),
            expected,
            "C -O{level}"
        );
        let rust_exe = dir.0.join(format!("rust-{level}"));
        checked(
            Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                .args([
                    "--edition=2021",
                    "-Dwarnings",
                    "-C",
                    &format!("opt-level={level}"),
                ])
                .arg(dir.0.join("out.rs"))
                .arg("-o")
                .arg(&rust_exe),
        );
        assert_eq!(
            run(&rust_exe, &dir.0.join("cases.txt")),
            expected,
            "Rust opt-level={level}"
        );
    }
}
