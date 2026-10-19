use fission_fsl::{
    compile_source, emit_instruction, execute_instruction, ExecutionStatus, FirOp, FirTerminator,
    FslcPackage, JitDecoder, OutputLayer, StackContract, ValueId,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const DIAMOND: &str = include_str!("../specs/stack-branch-join.fsl");

fn profile(body: &str) -> FslcPackage {
    compile_source(&format!("language proof {{ byte_order little; address_unit byte; instruction example {{ opcode 0x70; mnemonic \"fixture\"; evidence \"self-authored\" \"synthetic\" \"v1\" \"Control contract fixture\"; semantics {{ {body} }} }} }}")).unwrap()
}

#[test]
fn v6_retains_blocks_and_legacy_versions_keep_their_bytes() {
    let package = compile_source(DIAMOND).unwrap();
    assert_eq!(package.version, 6);
    let binary = package.encode_binary().unwrap();
    let loaded = FslcPackage::decode_binary(&binary).unwrap();
    assert_eq!(package, loaded);
    assert_eq!(loaded.encode_binary().unwrap(), binary);
    assert_eq!(loaded.instructions[0].blocks.len(), 4);
    assert!(
        emit_instruction(&loaded.instructions[0], OutputLayer::Fir, "ignored")
            .unwrap()
            .contains("CondBranch")
    );
    for end in 0..binary.len() {
        assert!(FslcPackage::decode_binary(&binary[..end]).is_err());
    }
    let mut trailing = binary.clone();
    trailing.push(0);
    assert!(FslcPackage::decode_binary(&trailing).is_err());
    for version in 1u16..6 {
        let mut bad = binary.clone();
        bad[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(FslcPackage::decode_binary(&bad).is_err());
        let mut downgraded = loaded.clone();
        downgraded.version = version;
        assert!(downgraded.encode_binary().is_err());
    }
    let mut legacy = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    for version in 1..=5 {
        legacy.version = version;
        let bytes = legacy.encode_binary().unwrap();
        let restored = FslcPackage::decode_binary(&bytes).unwrap();
        assert_eq!(bytes, restored.encode_binary().unwrap());
        assert!(restored.instructions[0].blocks.is_empty());
        let view = fission_fsl::control::blocks(&restored.instructions[0]);
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].terminator, FirTerminator::Return);
    }
}

#[test]
fn malformed_scopes_edges_types_ranges_and_terminators_refuse() {
    let original = compile_source(DIAMOND).unwrap();
    let reject = |package: FslcPackage| {
        assert!(package.validate().is_err());
        assert!(package.encode_binary().is_err());
        let mut stack = vec![8];
        let saved = stack.clone();
        assert!(execute_instruction(&package.instructions[0], &mut stack, 2).is_err());
        assert_eq!(stack, saved);
        assert!(emit_instruction(&package.instructions[0], OutputLayer::C, "fsl_bad").is_err());
    };
    let mut bad = original.clone();
    bad.instructions[0].blocks[1].start = 0;
    reject(bad);
    let mut bad = original.clone();
    bad.instructions[0].blocks[3].end += 1;
    reject(bad);
    let mut bad = original.clone();
    bad.instructions[0].blocks[1].parameters = vec![ValueId(0)];
    reject(bad);
    let mut bad = original.clone();
    bad.instructions[0].values[3].ty.bits = 16;
    reject(bad);
    let mut bad = original.clone();
    if let FirTerminator::CondBranch { condition, .. } =
        &mut bad.instructions[0].blocks[0].terminator
    {
        *condition = ValueId(0);
    }
    reject(bad);
    let mut bad = original.clone();
    if let FirTerminator::CondBranch { on_true, .. } = &mut bad.instructions[0].blocks[0].terminator
    {
        on_true.target = 100;
    }
    reject(bad);
    let mut bad = original.clone();
    if let FirTerminator::CondBranch { on_true, .. } = &mut bad.instructions[0].blocks[0].terminator
    {
        on_true.arguments.clear();
    }
    reject(bad);
    let mut bad = original.clone();
    bad.instructions[0].ops[5] = FirOp::VmStackPush { value: ValueId(0) };
    reject(bad);
    for (before, after) in [
        ("return;", ""),
        ("int.const 10", "int.const 0x100000000"),
        ("int.ult", "int.slt"),
        ("%small: u1", "%small: i1"),
        ("branch join(%low_sum);", "branch join(%high_sum);"),
        ("stack.push %joined;", "stack.push %input;"),
        ("high(%input)", "missing(%input)"),
        ("%low_input: u32", "%input: u32"),
    ] {
        assert!(
            compile_source(&DIAMOND.replace(before, after)).is_err(),
            "{before} -> {after}"
        );
    }
    let unreachable = DIAMOND.replace(
        "branch.if %small, low(%input), high(%input);",
        "branch low(%input);",
    );
    assert!(compile_source(&unreachable).is_err());
}

#[test]
fn cyclic_and_unbalanced_cfg_are_preserved_but_execution_refuses() {
    let cycle = profile("block entry() { %seed: u32 = int.const 0; branch again(%seed); } block again(%loop_value: u32) { branch again(%loop_value); }");
    cycle.validate().unwrap();
    assert_eq!(
        FslcPackage::decode_binary(&cycle.encode_binary().unwrap()).unwrap(),
        cycle
    );
    let unbalanced = compile_source(&DIAMOND.replace(
        "branch join(%low_sum);",
        "stack.push %low_sum; branch join(%low_sum);",
    ))
    .unwrap();
    for package in [cycle, unbalanced] {
        let instruction = &package.instructions[0];
        assert!(emit_instruction(instruction, OutputLayer::Fir, "ignored").is_ok());
        assert!(StackContract::for_instruction(instruction).is_err());
        for layer in [
            OutputLayer::C,
            OutputLayer::Rust,
            OutputLayer::CudaCpp,
            OutputLayer::Ptx,
        ] {
            assert!(emit_instruction(instruction, layer, "fsl_case").is_err());
        }
        assert!(JitDecoder::compile(&package).is_err());
        let mut stack = vec![9];
        assert!(execute_instruction(instruction, &mut stack, 2).is_err());
        assert_eq!(stack, [9]);
    }
}

#[test]
fn native_cli_runs_packaged_branches_and_refuses_unsupported_backends() {
    let directory = TempDir::new();
    let path = directory.0.join("branch.fslc");
    let package = compile_source(DIAMOND).unwrap();
    fs::write(&path, package.encode_binary().unwrap()).unwrap();
    for (value, expected) in [("9", "10"), ("10", "12"), ("4294967295", "1")] {
        let observed = checked(Command::new(env!("CARGO_BIN_EXE_fslc")).args([
            "execute",
            path.to_str().unwrap(),
            "70",
            "2",
            "77",
            value,
        ]));
        assert_eq!(
            observed.trim(),
            format!("status=Success stack=[77, {expected}]")
        );
    }
    let decoded = package
        .decode_bytes("stack.branch.join", &[0x70])
        .unwrap()
        .unwrap();
    let mut state = fission_fsl::MachineState {
        registers: vec![9],
        flags: vec![0],
    };
    let saved = state.clone();
    assert!(fission_fsl::execute_decoded(&package, &decoded, &mut state).is_err());
    assert_eq!(state, saved);
    assert!(JitDecoder::compile(&package).is_err());
    assert!(fission_fsl::emit_aot_object(&package).is_err());
    for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
        assert!(emit_instruction(&package.instructions[0], layer, "fsl_case").is_err());
    }
}

#[test]
fn compare_and_branch_evaluate_independently_and_failure_has_no_effects() {
    let package = compile_source(DIAMOND).unwrap();
    let instruction = &package.instructions[0];
    for x in 0..=65535 {
        let mut stack = vec![77, x];
        assert_eq!(
            execute_instruction(instruction, &mut stack, 2).unwrap(),
            ExecutionStatus::Success
        );
        assert_eq!(stack, [77, if x < 10 { x + 1 } else { x + 2 }]);
    }
    for (stack, capacity, expected) in [
        (vec![], 1, ExecutionStatus::StackUnderflow),
        (vec![1], 0, ExecutionStatus::InvalidState),
    ] {
        let saved = stack.clone();
        let mut stack = stack;
        assert_eq!(
            execute_instruction(instruction, &mut stack, capacity).unwrap(),
            expected
        );
        assert_eq!(stack, saved);
    }
    // All syntactic paths participate in preflight even when the condition is
    // constant. A transient push/pop needs capacity although final delta is 0.
    let peak = profile("block entry() { %condition: u1 = int.const 0; %value: u32 = int.const 7; branch.if %condition, temporary(%value), join(); } block temporary(%temp_value: u32) { stack.push %temp_value; %unused: u32 = stack.pop; branch join(); } block join() { return; }");
    assert_eq!(
        StackContract::for_instruction(&peak.instructions[0])
            .unwrap()
            .extra_capacity,
        1
    );
    let mut stack = vec![9];
    assert_eq!(
        execute_instruction(&peak.instructions[0], &mut stack, 1).unwrap(),
        ExecutionStatus::CapacityExceeded
    );
    assert_eq!(stack, [9]);
    assert_eq!(
        execute_instruction(&peak.instructions[0], &mut stack, 2).unwrap(),
        ExecutionStatus::Success
    );
    assert_eq!(stack, [9]);
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fsl-control-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
    let output = command.output().expect("compiler / executable required");
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

#[test]
fn block_arguments_and_comparisons_recompile_at_o0_o2_with_independent_oracles() {
    let directory = TempDir::new();
    type ProfileCase = (
        fission_fsl::CompiledInstruction,
        u16,
        &'static str,
        u64,
        u64,
    );
    let mut profiles: Vec<ProfileCase> = Vec::new();
    for bits in [1, 8, 32, 64] {
        for predicate in ["eq", "ult", "slt"] {
            let sign = if predicate == "slt" { "i" } else { "u" };
            let ty = format!("{sign}{bits}");
            let limit = if predicate == "slt" {
                0
            } else if bits == 1 {
                1
            } else {
                10
            };
            let high_add = if bits == 1 { 0 } else { 2 };
            let source = DIAMOND
                .replace("u32", &ty)
                .replace("int.ult", &format!("int.{predicate}"))
                .replace("int.const 10", &format!("int.const {limit}"))
                .replace("int.const 2", &format!("int.const {high_add}"));
            let package = compile_source(&source).unwrap();
            let package = FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap();
            profiles.push((
                package.instructions[0].clone(),
                bits,
                predicate,
                limit,
                high_add,
            ));
        }
    }
    // Argument reordering and values joined from two distinct predecessors.
    let permutation = profile("block entry() { %first: u64 = stack.pop; %second: u64 = stack.pop; branch reorder(%second, %first); } block reorder(%a: u64, %b: u64) { stack.push %a; stack.push %b; return; }");
    profiles.push((permutation.instructions[0].clone(), 64, "permutation", 0, 0));
    let constant = profile("%constant: i64 = int.const 0xffffffffffffffff; stack.push %constant;");
    profiles.push((constant.instructions[0].clone(), 64, "constant", 0, 0));
    let empty = profile("block entry() { return; }");
    profiles.push((empty.instructions[0].clone(), 64, "empty", 0, 0));
    let peak = profile("block entry() { %condition: u1 = int.const 0; %value: u32 = int.const 7; branch.if %condition, temporary(%value), join(); } block temporary(%temp_value: u32) { stack.push %temp_value; %unused: u32 = stack.pop; branch join(); } block join() { return; }");
    profiles.push((peak.instructions[0].clone(), 32, "peak", 0, 0));
    let mut c = String::new();
    let mut rust = String::new();
    for (index, (instruction, ..)) in profiles.iter().enumerate() {
        c.push_str(
            &emit_instruction(instruction, OutputLayer::C, &format!("fsl_case_{index}")).unwrap(),
        );
        rust.push_str(
            &emit_instruction(instruction, OutputLayer::Rust, &format!("fsl_case_{index}"))
                .unwrap(),
        );
    }
    let symbols = (0..profiles.len())
        .map(|i| format!("fsl_case_{i}"))
        .collect::<Vec<_>>()
        .join(",");
    c.push_str(&format!("\n#include <stdio.h>\n#include <inttypes.h>\nint main(void) {{\nuint32_t (*functions[])(uint64_t *, size_t *, size_t) = {{{symbols}}};\nunsigned index; size_t capacity, depth; uint64_t stack[4];\nwhile (scanf(\"%u %zu %zu %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64 \" %\" SCNu64, &index, &capacity, &depth, &stack[0], &stack[1], &stack[2], &stack[3]) == 7) {{\nuint32_t status = functions[index](stack, &depth, capacity);\nprintf(\"%u %zu\", status, depth); for (size_t i=0;i<(status == 0 ? depth : 4);++i) printf(\" %\" PRIu64, stack[i]); puts(\"\"); }} return 0; }}\n"));
    rust.push_str(&format!("\nfn main() {{ use std::io::Read; let functions: &[fn(&mut [u64], &mut usize)->u32] = &[{symbols}]; let mut input = String::new(); std::io::stdin().read_to_string(&mut input).unwrap(); let numbers = input.split_whitespace().map(|v| v.parse::<u64>().unwrap()).collect::<Vec<_>>(); for row in numbers.chunks_exact(7) {{ let mut stack = [row[3],row[4],row[5],row[6]]; let mut depth = row[2] as usize; let status = functions[row[0] as usize](&mut stack[..row[1] as usize], &mut depth); print!(\"{{}} {{}}\",status,depth); for value in &stack[..if status == 0 {{ depth }} else {{ 4 }}] {{ print!(\" {{}}\",value); }} println!(); }} }}\n"));
    fs::write(directory.0.join("out.c"), c).unwrap();
    fs::write(directory.0.join("out.rs"), rust).unwrap();
    let input_path = directory.0.join("cases.txt");
    let mut input = fs::File::create(&input_path).unwrap();
    let mut expected = String::new();
    let mut seed = 0x7edb_4638_a618_1359u64;
    for (index, (instruction, bits, predicate, limit, high_add)) in profiles.iter().enumerate() {
        let mask = if *bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let contract = StackContract::for_instruction(instruction).unwrap();
        for case in 0..128 {
            let depth = if case < 100 { 2 } else { case % 5 };
            let capacity = if case < 100 { 4 } else { case / 5 % 5 };
            let mut slots = [0u64; 4];
            for value in &mut slots {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *value = seed;
            }
            if case < 8 {
                slots[1] = [
                    0,
                    1,
                    *limit,
                    limit.wrapping_sub(1),
                    mask,
                    mask >> 1,
                    (mask >> 1) + 1,
                    10,
                ][case];
            }
            let mut oracle = slots[..depth].to_vec();
            let status = if depth > capacity {
                3
            } else if depth < contract.required_input {
                1
            } else if contract.extra_capacity > capacity - depth {
                2
            } else {
                0
            };
            if status == 0 {
                match *predicate {
                    "constant" => oracle.push(u64::MAX),
                    "empty" | "peak" | "permutation" => {}
                    _ => {
                        let raw = oracle.pop().unwrap() & mask;
                        let signed = if raw & (1u64 << (bits - 1)) != 0 {
                            i128::from(raw) - (1i128 << bits)
                        } else {
                            i128::from(raw)
                        };
                        let condition = match *predicate {
                            "eq" => raw == *limit,
                            "ult" => raw < *limit,
                            "slt" => signed < 0,
                            _ => unreachable!(),
                        };
                        oracle.push(
                            ((u128::from(raw) + u128::from(if condition { 1 } else { *high_add }))
                                & u128::from(mask)) as u64,
                        );
                    }
                }
            }
            let mut reference = slots[..depth].to_vec();
            assert_eq!(
                execute_instruction(instruction, &mut reference, capacity).unwrap() as u32,
                status
            );
            assert_eq!(reference, oracle);
            writeln!(
                input,
                "{index} {capacity} {depth} {} {} {} {}",
                slots[0], slots[1], slots[2], slots[3]
            )
            .unwrap();
            use std::fmt::Write as _;
            write!(expected, "{status} {}", oracle.len()).unwrap();
            for &value in if status == 0 {
                oracle.as_slice()
            } else {
                &slots
            } {
                write!(expected, " {value}").unwrap();
            }
            expected.push('\n');
        }
    }
    drop(input);
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
        assert_eq!(run(&c_exe, &input_path), expected, "C -O{level}");
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
        assert_eq!(run(&rust_exe, &input_path), expected, "Rust -O{level}");
    }
    println!("control gate: 16 profiles, 2048 independent/reference states, 8192 C/Rust O0/O2 comparisons");
}
