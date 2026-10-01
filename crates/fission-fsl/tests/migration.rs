use fission_fsl::abi::compile_abi_source;
use fission_fsl::{
    compile_source, emit_instruction, execute_decoded, ExecutionStatus, MachineState, OutputLayer,
};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const ABI: &str = include_str!("../specs/ebpf.fslabi");
const BPF_ABI: &str = include_str!("../specs/bpf.fslabi");
const ADD: &str = include_str!("../specs/ebpf-add64-register.fsl");

#[test]
fn migrated_cspec_has_typed_ordered_metadata_and_rejects_information_loss() {
    let abi = compile_abi_source(ABI).unwrap();
    assert_eq!(abi.data["pointer_size"], 8);
    assert_eq!(abi.size_alignments[&8], 8);
    assert_eq!(abi.stack_register, "R10");
    assert_eq!(abi.global_spaces, ["ram", "syscall"]);
    let convention = &abi.conventions[0];
    assert_eq!(
        convention
            .inputs
            .iter()
            .map(|r| r.register.as_str())
            .collect::<Vec<_>>(),
        ["R1", "R2", "R3", "R4", "R5"]
    );
    assert_eq!(convention.outputs[0].register, "R0");
    assert_eq!(
        convention.preserved_registers,
        ["R6", "R7", "R8", "R9", "R10"]
    );
    assert_eq!(convention.preserved_memory[0].offset, 8);
    assert_eq!(convention.preserved_memory[0].size_bytes, 8);
    assert_eq!(
        compile_abi_source(&ABI.replace("extrapop 0", "extrapop unknown"))
            .unwrap()
            .conventions[0]
            .extrapop,
        None
    );
    assert!(compile_abi_source(BPF_ABI).unwrap().conventions[0].output_killed_by_call);
    for malformed in [
        ABI.replace("pointer_size 8", "pointer_size 0"),
        ABI.replace("data long_size", "data unknown_size"),
        ABI.replace("alignment 8 8;", "alignment 8 8; alignment 8 4;"),
        ABI.replace("input_register \"R1\" 1 8", "input_register \"R1\" 9 8"),
        ABI.replace(
            "default_convention \"__fastcall\"",
            "default_convention \"missing\"",
        ),
        ABI.replace("stackshift 0;", "stackshift 0; opaque_rule \"unmodeled\";"),
    ] {
        assert!(compile_abi_source(&malformed).is_err());
    }
}

fn checked(command: &mut Command) {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn migrated_sleigh_leaf_executes_and_recompiles_without_sla_dependency() {
    let package = compile_source(ADD).unwrap();
    let instruction = &package.instructions[0];
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("fsl-ebpf-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let mut c = emit_instruction(instruction, OutputLayer::C, "fsl_add").unwrap();
    let mut rust = emit_instruction(instruction, OutputLayer::Rust, "fsl_add").unwrap();
    c.push_str("\nint main(void) {\n");
    rust.push_str("\nfn main() {\n");
    let mut count = 0;
    for destination in 0..11u8 {
        for source in 0..11u8 {
            for (left, right) in [
                (0, 0),
                (u64::MAX, 1),
                (u64::MAX, u64::MAX),
                (123456789, 987654321),
            ] {
                // Nonzero unused fields test that translation retains the whole instruction.
                let raw = [
                    0x0f,
                    destination | source << 4,
                    0x34,
                    0x12,
                    0x78,
                    0x56,
                    0x34,
                    0x12,
                ];
                let decoded = package
                    .decode_bytes("ebpf.le.add64.register", &raw)
                    .unwrap()
                    .unwrap();
                assert_eq!(package.reencode(&decoded, &[]).unwrap(), raw);
                let mut state = MachineState {
                    registers: (0..11).map(|r| r + 77).collect(),
                    flags: vec![1],
                };
                state.registers[destination as usize] = left;
                state.registers[source as usize] = right;
                let before = state.clone();
                let mut oracle = state.clone();
                oracle.registers[destination as usize] =
                    ((u128::from(state.registers[destination as usize])
                        + u128::from(state.registers[source as usize]))
                        % (1u128 << 64)) as u64;
                assert_eq!(
                    execute_decoded(&package, &decoded, &mut state).unwrap(),
                    ExecutionStatus::Success
                );
                assert_eq!(state, oracle);
                c.push_str("    { uint64_t r[11] = {");
                rust.push_str("    { let mut r = [");
                for (i, &v) in before.registers.iter().enumerate() {
                    if i != 0 {
                        c.push(',');
                        rust.push(',');
                    }
                    c.push_str(&format!("UINT64_C({v})"));
                    rust.push_str(&format!("{v}u64"));
                }
                c.push_str(&format!("}}; uint64_t f[1]={{1}}, fields[2]={{{destination},{source}}}; if(fsl_add(r,11,f,1,fields,2)!=0 || f[0]!=1) return 1;\n"));
                rust.push_str(&format!("]; let mut f=[1u64]; assert_eq!(fsl_add(&mut r,&mut f,&[{destination},{source}]),0); assert_eq!(f,[1]);\n"));
                for (i, &v) in oracle.registers.iter().enumerate() {
                    c.push_str(&format!("        if(r[{i}] != UINT64_C({v})) return 2;\n"));
                    rust.push_str(&format!("        assert_eq!(r[{i}],{v}u64);\n"));
                }
                c.push_str("    }\n");
                rust.push_str("    }\n");
                count += 1;
            }
        }
    }
    c.push_str("return 0; }\n");
    rust.push_str("}\n");
    fs::write(directory.join("out.c"), c).unwrap();
    fs::write(directory.join("out.rs"), rust).unwrap();
    for level in ["0", "2"] {
        let c_exe = directory.join(format!("c-{level}"));
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
                .arg(&c_exe),
        );
        checked(&mut Command::new(c_exe));
        let rust_exe = directory.join(format!("rust-{level}"));
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
                .arg(&rust_exe),
        );
        checked(&mut Command::new(rust_exe));
    }
    for invalid in [0x0e, 0x0f, 0x0b] {
        assert!(package
            .decode_bytes("ebpf.le.add64.register", &[0x0f, invalid, 0, 0, 0, 0, 0, 0])
            .unwrap()
            .is_none());
    }
    assert!(package
        .decode_bytes("ebpf.le.add64.register", &[0x07, 0x12, 0, 0, 0, 0, 0, 0])
        .unwrap()
        .is_none());
    println!(
        "migrated eBPF leaf states={count}; C/Rust O0/O2 comparisons={}",
        count * 4
    );
    fs::remove_dir_all(directory).unwrap();
}
