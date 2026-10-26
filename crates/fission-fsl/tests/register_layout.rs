use fission_fsl::abi::compile_abi_source;
use fission_fsl::registers::{
    compile_layout_source, execute_bound, link_abi, RegisterBinding, RegisterFile,
};
use fission_fsl::{compile_source, ExecutionStatus};
use std::{fs, process::Command};

const BPF: &str = include_str!("../specs/bpf.le.registers.fslregs");
const EBPF: &str = include_str!("../specs/ebpf.le.registers.fslregs");
const EBPF_BE: &str = include_str!("../specs/ebpf.be.registers.fslregs");
const ABI: &str = include_str!("../specs/ebpf.fslabi");
const BPF_ABI: &str = include_str!("../specs/bpf.fslabi");
const ADD: &str = include_str!("../specs/ebpf-add64-register.fsl");

#[test]
fn source_views_share_storage_and_preserve_partial_write_bytes() {
    for (source, big) in [
        (BPF.to_owned(), false),
        (BPF.replace("byte little", "byte big"), true),
    ] {
        let layout = compile_layout_source(&source).unwrap();
        assert_eq!(layout.default_space, "ram");
        assert_eq!(layout.registers.len(), 15);
        assert!(layout
            .overlaps(layout.resolve("A").unwrap(), layout.resolve("AH").unwrap())
            .unwrap());
        assert!(!layout
            .overlaps(layout.resolve("A").unwrap(), layout.resolve("X").unwrap())
            .unwrap());
        let mut file = RegisterFile::new(layout).unwrap();
        file.write_u64("A", 0x11223344).unwrap();
        file.write_u64("X", 0xaabbccdd).unwrap();
        assert_eq!(
            file.read_u64("AH").unwrap(),
            if big { 0x1122 } else { 0x3344 }
        );
        assert_eq!(file.read_u64("AB").unwrap(), if big { 0x11 } else { 0x44 });
        file.write_u64("AH", 0x5566).unwrap();
        assert_eq!(
            file.read_u64("A").unwrap(),
            if big { 0x55663344 } else { 0x11225566 }
        );
        file.write_u64("AB", 0x77).unwrap();
        assert_eq!(
            file.read_u64("A").unwrap(),
            if big { 0x77663344 } else { 0x11225577 }
        );
        assert_eq!(file.read_u64("X").unwrap(), 0xaabbccdd);
        let before = file.clone();
        assert!(file.write_u64("AB", 0x100).is_err());
        assert!(file.write_bytes("AH", &[0]).is_err());
        assert!(file.write_bytes("unknown", &[0]).is_err());
        assert_eq!(file, before);
    }
    for source in [EBPF, EBPF_BE] {
        let mut file = RegisterFile::new(compile_layout_source(source).unwrap()).unwrap();
        file.write_u64("R0", 0x1122334455667788).unwrap();
        assert_eq!(file.read_u64("R0").unwrap(), 0x1122334455667788);
        assert_eq!(
            file.read_bytes("R0").unwrap()[0],
            if source == EBPF { 0x88 } else { 0x11 }
        );
    }
}

#[test]
fn layout_validation_refuses_ambiguous_names_unknown_units_and_unbounded_storage() {
    for malformed in [
        EBPF.replace("byte little", "word little"),
        EBPF.replace("default_space \"ram\";", "default_space \"missing\";"),
        EBPF.replace("register 4 byte", "register 0 byte"),
        EBPF.replace("\"R0\" \"register\" 0 8", "\"R0\" \"ram\" 0 8"),
        EBPF.replace("\"R1\" \"register\" 8 8", "\"R0\" \"register\" 8 8"),
        EBPF.replace(
            "\"R0\" \"register\" 0 8",
            "\"R0\" \"register\" 4294967295 8",
        ),
        EBPF.replace("space \"syscall\"", "space \"ram\""),
        EBPF.replace("\"R0\" \"register\" 0 8", "\"R0\" \"register\" 0 0"),
        EBPF.to_owned() + " trailing",
        EBPF.replace(
            "default_space \"ram\";",
            "default_space \"\"; default_space \"ram\";",
        ),
    ] {
        assert!(compile_layout_source(&malformed).is_err(), "{malformed}");
    }
    let huge = compile_layout_source(
        &EBPF.replace("\"PC\" \"register\" 88 8", "\"PC\" \"register\" 33554432 8"),
    )
    .unwrap();
    assert!(RegisterFile::new(huge).is_err());
    let wide = compile_layout_source(
        &EBPF.replace("\"PC\" \"register\" 88 8", "\"PC\" \"register\" 88 512"),
    )
    .unwrap();
    let mut file = RegisterFile::new(wide).unwrap();
    file.write_bytes("PC", &[0x55; 512]).unwrap();
    assert_eq!(file.read_bytes("PC").unwrap(), &[0x55; 512]);
    assert!(file.read_u64("PC").is_err());
}

#[test]
fn abi_links_ordered_register_identities_and_refuses_source_width_mismatch() {
    let abi = compile_abi_source(ABI).unwrap();
    for source in [EBPF, EBPF_BE] {
        let layout = compile_layout_source(source).unwrap();
        let linked = link_abi(&abi, &layout).unwrap();
        assert_eq!(layout.view(linked.stack_register).unwrap().offset, 80);
        let c = &linked.conventions[0];
        let offsets: Vec<_> = c
            .inputs
            .iter()
            .map(|e| {
                (
                    layout.view(e.register).unwrap().offset,
                    e.min_bytes,
                    e.max_bytes,
                )
            })
            .collect();
        assert_eq!(
            offsets,
            [(8, 1, 8), (16, 1, 8), (24, 1, 8), (32, 1, 8), (40, 1, 8)]
        );
        assert_eq!(layout.view(c.outputs[0].register).unwrap().name, "R0");
        assert_eq!(
            c.preserved_registers
                .iter()
                .map(|&id| layout.view(id).unwrap().name.as_str())
                .collect::<Vec<_>>(),
            ["R6", "R7", "R8", "R9", "R10"]
        );
        assert_eq!(linked.abi.conventions[0].preserved_memory[0].offset, 8);
    }
    let layout = compile_layout_source(EBPF).unwrap();
    for malformed in [
        ABI.replace("input_register \"R1\" 1 8", "input_register \"R1\" 1 9"),
        ABI.replace("input_register \"R1\"", "input_register \"unknown\""),
        ABI.replace("input_register \"R2\"", "input_register \"R1\""),
        ABI.replace("global_space \"ram\"", "global_space \"register\""),
        ABI.replace(
            "preserved_register \"R6\";",
            "preserved_register \"R6\"; clobbered_register \"R6\";",
        ),
        ABI.replace(
            "preserved_memory \"ram\" 8 8",
            "preserved_memory \"syscall\" 4294967295 8",
        ),
    ] {
        assert!(link_abi(&compile_abi_source(&malformed).unwrap(), &layout).is_err());
    }
    let bpf = compile_layout_source(BPF).unwrap();
    let bpf_abi = compile_abi_source(BPF_ABI).unwrap();
    assert!(link_abi(&bpf_abi, &bpf)
        .unwrap_err()
        .message
        .contains("width 4 differs from ABI pointer_size 8"));
    // Synthetic consistent pointer metadata isolates alias-conflict checking.
    let alias_conflict = compile_abi_source(
        &BPF_ABI.replace("pointer_size 8", "pointer_size 4").replace(
            "stackshift 0;",
            "stackshift 0; preserved_register \"A\"; clobbered_register \"AH\";",
        ),
    )
    .unwrap();
    assert!(link_abi(&alias_conflict, &bpf)
        .unwrap_err()
        .message
        .contains("overlap"));
}

#[test]
fn bound_fir_matches_byte_storage_oracle_for_all_ebpf_selectors() {
    let package = compile_source(ADD).unwrap();
    let abi = compile_abi_source(ABI).unwrap();
    let binding = RegisterBinding {
        registers: (0..11).map(|i| format!("R{i}")).collect(),
        flags: vec![],
    };
    let mut count = 0;
    for source in [EBPF, EBPF_BE] {
        let layout = compile_layout_source(source).unwrap();
        link_abi(&abi, &layout).unwrap();
        for dst in 0..11u8 {
            for src in 0..11u8 {
                for (left, right) in [
                    (0, 0),
                    (u64::MAX, 1),
                    (u64::MAX, u64::MAX),
                    (123456789, 987654321),
                ] {
                    let decoded = package
                        .decode_bytes(
                            "ebpf.le.add64.register",
                            &[0x0f, dst | src << 4, 0, 0, 0, 0, 0, 0],
                        )
                        .unwrap()
                        .unwrap();
                    let mut file = RegisterFile::new(layout.clone()).unwrap();
                    for i in 0..11 {
                        file.write_u64(&format!("R{i}"), i + 77).unwrap();
                    }
                    file.write_u64("PC", 0x123456789abcdef0).unwrap();
                    file.write_u64(&format!("R{dst}"), left).unwrap();
                    file.write_u64(&format!("R{src}"), right).unwrap();
                    // Oracle works directly on backing bytes, independent of the
                    // FIR evaluator and storage integer read/write helpers.
                    let mut expected = (0..11)
                        .flat_map(|i| file.read_bytes(&format!("R{i}")).unwrap().to_vec())
                        .collect::<Vec<_>>();
                    let read = |i: u8| {
                        let bytes: [u8; 8] = expected[i as usize * 8..i as usize * 8 + 8]
                            .try_into()
                            .unwrap();
                        if source == EBPF {
                            u64::from_le_bytes(bytes)
                        } else {
                            u64::from_be_bytes(bytes)
                        }
                    };
                    let value =
                        ((u128::from(read(dst)) + u128::from(read(src))) % (1u128 << 64)) as u64;
                    expected[dst as usize * 8..dst as usize * 8 + 8].copy_from_slice(&if source
                        == EBPF
                    {
                        value.to_le_bytes()
                    } else {
                        value.to_be_bytes()
                    });
                    assert_eq!(
                        execute_bound(&package, &decoded, &binding, &mut file).unwrap(),
                        ExecutionStatus::Success
                    );
                    let observed = (0..11)
                        .flat_map(|i| file.read_bytes(&format!("R{i}")).unwrap().to_vec())
                        .collect::<Vec<_>>();
                    assert_eq!(observed, expected);
                    assert_eq!(file.read_u64("PC").unwrap(), 0x123456789abcdef0);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 968);
    println!("bound FIR byte-storage comparisons={count}; LE instruction profile with two register-storage byte orders; not big-endian eBPF decoding");
}

#[test]
fn binding_refusals_preserve_storage_before_effects() {
    let package = compile_source(ADD).unwrap();
    let decoded = package
        .decode_bytes("ebpf.le.add64.register", &[0x0f, 0x12, 0, 0, 0, 0, 0, 0])
        .unwrap()
        .unwrap();
    let mut file = RegisterFile::new(compile_layout_source(EBPF).unwrap()).unwrap();
    file.write_u64("R1", u64::MAX).unwrap();
    let before = file.clone();
    for registers in [vec!["R0", "R0", "R2"], vec!["R0", "unknown", "R2"]] {
        let binding = RegisterBinding {
            registers: registers.into_iter().map(str::to_owned).collect(),
            flags: vec![],
        };
        assert!(execute_bound(&package, &decoded, &binding, &mut file).is_err());
        assert_eq!(file, before);
    }
    let binding = RegisterBinding {
        registers: vec!["R0".into()],
        flags: vec![],
    };
    assert_eq!(
        execute_bound(&package, &decoded, &binding, &mut file).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(file, before);
    let binding = RegisterBinding {
        registers: vec!["R0".into(), "R1".into(), "R2".into()],
        flags: vec![],
    };
    let narrow = compile_source(&ADD.replace("u64", "u32")).unwrap();
    assert!(execute_bound(&narrow, &decoded, &binding, &mut file).is_err());
    assert_eq!(file, before);
    let mut tampered = decoded.clone();
    tampered.fields[0].1 = 9;
    assert!(execute_bound(&package, &tampered, &binding, &mut file).is_err());
    assert_eq!(file, before);
}

#[test]
fn native_cli_links_and_executes_migrated_layout_without_sleigh() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let bin = env!("CARGO_BIN_EXE_fslc");
    let result = Command::new(bin)
        .args(["link-abi"])
        .arg(root.join("specs/ebpf.fslabi"))
        .arg(root.join("specs/ebpf.le.registers.fslregs"))
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("stack=R10@register:80+8"));
    let result = Command::new(bin)
        .args(["link-abi"])
        .arg(root.join("specs/bpf.fslabi"))
        .arg(root.join("specs/bpf.le.registers.fslregs"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("width 4 differs from ABI pointer_size 8")
    );
    let path = std::env::temp_dir().join(format!("fsl-layout-{}.fslc", std::process::id()));
    fs::write(&path, compile_source(ADD).unwrap().encode_binary().unwrap()).unwrap();
    let result = Command::new(bin)
        .args(["execute-layout"])
        .arg(root.join("specs/ebpf.le.registers.fslregs"))
        .arg(root.join("specs/ebpf.fslabi"))
        .arg(&path)
        .args([
            "ebpf.le.add64.register",
            "0f12000000000000",
            "R0,R1,R2",
            "0,1,0xffffffffffffffff",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        "status=Success registers=[0, 1, 0]"
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn generic_flag_storage_binding_executes_carry_and_rejects_invalid_flag_bytes() {
    // Mechanical binding fixture, not an imported GPU register layout/ABI.
    let source = r#"layout synthetic.flags {
        evidence "test" "local" "1" "Synthetic byte storage contract";
        default_space "ram";
        space "ram" memory 8 byte little;
        space "register" register 4 byte little;
        register "A" "register" 0 4;
        register "B" "register" 4 4;
        register "C" "register" 8 4;
        register "F" "register" 12 1;
    }"#;
    let package = compile_source(include_str!("../specs/amdgcn-gfx900-saddc-u32.fsl")).unwrap();
    let decoded = package
        .decode_bytes("amdgcn.gfx900.saddc_u32", &[0, 1, 2, 0x82])
        .unwrap()
        .unwrap();
    let binding = RegisterBinding {
        registers: vec!["A".into(), "B".into(), "C".into()],
        flags: vec!["F".into()],
    };
    let mut file = RegisterFile::new(compile_layout_source(source).unwrap()).unwrap();
    file.write_u64("A", u32::MAX as u64).unwrap();
    file.write_u64("B", 0).unwrap();
    file.write_u64("F", 1).unwrap();
    assert_eq!(
        execute_bound(&package, &decoded, &binding, &mut file).unwrap(),
        ExecutionStatus::Success
    );
    assert_eq!(file.read_u64("C").unwrap(), 0);
    assert_eq!(file.read_u64("F").unwrap(), 1);
    file.write_u64("F", 2).unwrap();
    let before = file.clone();
    assert_eq!(
        execute_bound(&package, &decoded, &binding, &mut file).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(file, before);
    let mut missing = binding;
    missing.flags.clear();
    assert_eq!(
        execute_bound(&package, &decoded, &missing, &mut file).unwrap(),
        ExecutionStatus::InvalidState
    );
    assert_eq!(file, before);
}
