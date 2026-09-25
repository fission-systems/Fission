//! Taint reaches a sink, and a clean run reports nothing.
//!
//! The fixture is the emulator's own concolic-branch ELF: it reads a byte from
//! stdin and branches on it. Built from source in this repository -- never a
//! corpus binary, which is the one thing not to point dynamic analysis at.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::ShadowMode;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(stdin: &[u8]) -> Emulator {
    let path = fixture_path();
    let binary = LoadedBinary::from_file(&path).expect("load");
    build_from_binary(stdin, binary)
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf")
}

fn build_with_inserted_code(stdin: &[u8], offset: usize, code: &[u8], suffix: &str) -> Emulator {
    let path = fixture_path();
    let mut bytes = std::fs::read(&path).expect("read fixture");
    bytes.splice(offset..offset, code.iter().copied());
    for field in [0x60, 0x68] {
        let old = u64::from_le_bytes(bytes[field..field + 8].try_into().unwrap());
        bytes[field..field + 8].copy_from_slice(&(old + code.len() as u64).to_le_bytes());
    }
    let binary = LoadedBinary::from_bytes(bytes, format!("{}-{suffix}", path.display()))
        .expect("load modified fixture");
    build_from_binary(stdin, binary)
}

fn build_with_many_active_control_branches(stdin: &[u8], branch_count: usize) -> Emulator {
    let path = fixture_path();
    let mut bytes = std::fs::read(&path).expect("read fixture");
    let code_offset = bytes.len();
    let old_file_size = u64::from_le_bytes(bytes[0x60..0x68].try_into().unwrap());
    let old_memory_size = u64::from_le_bytes(bytes[0x68..0x70].try_into().unwrap());
    assert_eq!(old_file_size, code_offset as u64);

    // Append a small x86-64 program to the fixture's executable PT_LOAD:
    // read one tainted byte, then execute distinct conditional branches that
    // all reconverge after the final branch. The concrete input keeps each
    // JNE on its fallthrough path, so every proven scope overlaps at the cap.
    let mut code = vec![
        0xb8, 0, 0, 0, 0, // mov eax, 0 (read)
        0x31, 0xff, // xor edi, edi
        0x48, 0x89, 0xe6, // mov rsi, rsp
        0xba, 1, 0, 0, 0, // mov edx, 1
        0x0f, 0x05, // syscall
    ];
    let mut branch_displacements = Vec::with_capacity(branch_count);
    for _ in 0..branch_count {
        code.extend_from_slice(&[
            0x80, 0x3c, 0x24, 0x41, // cmp byte ptr [rsp], 'A'
            0x0f, 0x85, // jne rel32 to common join
        ]);
        branch_displacements.push(code.len());
        code.extend_from_slice(&[0; 4]);
    }
    code.extend_from_slice(&[
        0x31, 0xff, // xor edi, edi
        0xe9, // jmp rel32 to common join
    ]);
    let final_jump_displacement = code.len();
    code.extend_from_slice(&[0; 4]);
    let common_join_offset = code.len();
    code.extend_from_slice(&[
        0xb8, 60, 0, 0, 0, // mov eax, 60 (exit)
        0x0f, 0x05, // syscall
    ]);

    let image_base = 0x400000u64;
    let target = image_base + code_offset as u64 + common_join_offset as u64;
    for displacement_offset in branch_displacements
        .into_iter()
        .chain(std::iter::once(final_jump_displacement))
    {
        let instruction_end = image_base + code_offset as u64 + displacement_offset as u64 + 4;
        let displacement = i32::try_from(target as i64 - instruction_end as i64)
            .expect("generated branch target fits rel32");
        code[displacement_offset..displacement_offset + 4]
            .copy_from_slice(&displacement.to_le_bytes());
    }

    let entry = image_base + code_offset as u64;
    bytes[0x18..0x20].copy_from_slice(&entry.to_le_bytes());
    bytes.extend_from_slice(&code);
    bytes[0x60..0x68].copy_from_slice(&(old_file_size + code.len() as u64).to_le_bytes());
    bytes[0x68..0x70].copy_from_slice(&(old_memory_size + code.len() as u64).to_le_bytes());
    let binary = LoadedBinary::from_bytes(bytes, format!("{}-many-branches", path.display()))
        .expect("load generated executable");
    build_from_binary(stdin, binary)
}

fn build_with_clean_overwrite(stdin: &[u8]) -> Emulator {
    // Insert `mov rdi, 0` at the diamond's join, immediately before the exit
    // syscall. This makes the joined value independent of the branch.
    build_with_inserted_code(
        stdin,
        0xB2,
        &[0x48, 0xC7, 0xC7, 0, 0, 0, 0],
        "clean-overwrite",
    )
}

fn build_with_direct_write(stdin: &[u8]) -> Emulator {
    // After read(0, rsp, 1), write(1, rsp, 1) sends the untrusted byte to a
    // syscall sink before the fixture tests it in a branch.
    build_with_inserted_code(
        stdin,
        0x96,
        &[
            0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
            0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
            0x48, 0x89, 0xE6, // mov rsi, rsp
            0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00, // mov rdx, 1
            0x0F, 0x05, // syscall
        ],
        "direct-write",
    )
}

fn build_with_transformed_write(stdin: &[u8]) -> Emulator {
    // Exercise data taint through MOVZX (IntZExt), which has no dedicated JIT
    // shadow callback, before the value is stored and sent to write(2).
    build_with_inserted_code(
        stdin,
        0x96,
        &[
            0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
            0x88, 0x44, 0x24, 0x01, // mov byte ptr [rsp+1], al
            0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1
            0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00, // mov rdi, 1
            0x48, 0x8D, 0x74, 0x24, 0x01, // lea rsi, [rsp+1]
            0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00, // mov rdx, 1
            0x0F, 0x05, // syscall
        ],
        "transformed-write",
    )
}

fn build_from_binary(stdin: &[u8], binary: LoadedBinary) -> Emulator {
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
        .with_max_inst(Some(4096));
    emu.apply_linux_image(info).expect("image");
    emu.seed_stdin(stdin);
    emu
}

#[test]
fn taint_is_off_unless_asked_for() {
    let mut emu = build(b"A");
    assert_eq!(emu.shadow_mode(), ShadowMode::Off);
    let _ = emu.run();
    assert!(
        emu.taint.is_empty(),
        "a run nobody asked to taint reported taint anyway"
    );
}

#[test]
fn branch_controlled_syscall_argument_is_reported_as_control_flow() {
    for force_interpreter in [false, true] {
        let mut emu = build(b"A");
        emu.force_interpreter = force_interpreter;
        emu.set_shadow_mode(ShadowMode::Taint);
        let _ = emu.run();

        // `read` filled a guest buffer from outside, so the run has a source.
        let sources: Vec<&str> = emu
            .taint
            .sources()
            .iter()
            .map(|s| s.label.as_str())
            .collect();
        assert!(
            sources.contains(&"read"),
            "stdin should be a source, got {sources:?}"
        );

        assert!(
            emu.taint.hits.iter().any(|hit| {
                hit.kind == fission_emulator::TaintDependencyKind::Control
                    && hit.sink == "syscall arg"
                    && hit.sources.iter().any(|source| source == "read")
            }),
            "the branch-selected exit argument should retain control dependence (interpreter={force_interpreter}): {:?}",
            emu.taint.hits
        );
        assert!(
            !emu.taint.hits.iter().any(|hit| {
                hit.kind == fission_emulator::TaintDependencyKind::Data
                    && hit.sink == "syscall arg"
                    && hit.sources.iter().any(|source| source == "read")
            }),
            "a branch-only source must not be mislabeled direct data: {:?}",
            emu.taint.hits
        );

        let hit = emu
            .taint
            .hits
            .iter()
            .find(|hit| hit.kind == fission_emulator::TaintDependencyKind::Control)
            .expect("control hit was checked above");
        let serialized = serde_json::to_value(fission_emulator::metrics::TaintHitReport {
            pc: hit.pc,
            kind: hit.kind,
            sink: hit.sink.clone(),
            detail: hit.detail.clone(),
            sources: hit.sources.clone(),
        })
        .expect("taint hit serializes");
        assert_eq!(serialized["kind"], "control");

        let report = fission_emulator::SandboxMetricsReport::from_run(
            "generated.elf",
            "ELF",
            true,
            emu.pc,
            emu.metrics.clone(),
            None,
        )
        .with_taint(&emu.taint);
        let json = serde_json::to_value(report).expect("taint report serializes");
        assert_eq!(json["behavior"]["taint"]["control_tracking_complete"], true);
        assert_eq!(json["behavior"]["taint"]["control_scopes_dropped"], 0);
    }
}

#[test]
fn control_scope_truncation_is_reported_by_jit_and_test_interpreter() {
    let mut engine_results = Vec::new();
    for force_interpreter in [false, true] {
        let mut emu = build_with_many_active_control_branches(
            b"A",
            fission_emulator::TaintState::DEFAULT_CONTROL_SCOPE_CAP + 1,
        );
        emu.force_interpreter = force_interpreter;
        emu.set_shadow_mode(ShadowMode::Taint);
        let _ = emu.run();

        assert_eq!(emu.taint.control_scopes_dropped(), 1);
        assert!(!emu.taint.control_tracking_complete());
        assert!(emu.taint.hits.iter().any(|hit| {
            hit.kind == fission_emulator::TaintDependencyKind::Control
                && hit.sink == "syscall arg"
                && hit.sources.iter().any(|source| source == "read")
        }));

        let report = fission_emulator::SandboxMetricsReport::from_run(
            "generated-many-branches.elf",
            "ELF",
            true,
            emu.pc,
            emu.metrics.clone(),
            None,
        )
        .with_taint(&emu.taint);
        let json = serde_json::to_value(report).expect("taint report serializes");
        assert_eq!(
            json["behavior"]["taint"]["control_tracking_complete"],
            false
        );
        assert_eq!(json["behavior"]["taint"]["control_scopes_dropped"], 1);
        engine_results.push(emu.taint.hits.clone());
    }
    assert_eq!(engine_results[0], engine_results[1]);
}

#[test]
fn a_clean_write_at_reconvergence_removes_control_taint() {
    for force_interpreter in [false, true] {
        let mut emu = build_with_clean_overwrite(b"A");
        emu.force_interpreter = force_interpreter;
        emu.set_shadow_mode(ShadowMode::Taint);
        let _ = emu.run();

        assert!(
            !emu.taint.hits.iter().any(|hit| {
                hit.kind == fission_emulator::TaintDependencyKind::Control
                    && hit.sink == "syscall arg"
                    && hit.detail == "exit arg0"
            }),
            "a clean post-join overwrite retained control taint (interpreter={force_interpreter}): {:?}",
            emu.taint.hits
        );
    }
}

#[test]
fn direct_input_reaching_a_syscall_buffer_stays_data_taint() {
    for force_interpreter in [false, true] {
        let mut emu = build_with_direct_write(b"A");
        emu.force_interpreter = force_interpreter;
        emu.set_shadow_mode(ShadowMode::Taint);
        let _ = emu.run();

        assert!(
            emu.taint.hits.iter().any(|hit| {
                hit.kind == fission_emulator::TaintDependencyKind::Data
                    && hit.sink == "syscall buffer"
                    && hit.detail.starts_with("write arg1")
                    && hit.sources.iter().any(|source| source == "read")
            }),
            "the byte read from stdin should remain direct data taint (interpreter={force_interpreter}): {:?}",
            emu.taint.hits
        );
    }
}

#[test]
fn direct_data_taint_survives_an_unary_extension_in_both_engines() {
    for force_interpreter in [false, true] {
        let mut emu = build_with_transformed_write(b"A");
        emu.force_interpreter = force_interpreter;
        emu.set_shadow_mode(ShadowMode::Taint);
        let _ = emu.run();

        assert!(
            emu.taint.hits.iter().any(|hit| {
                hit.kind == fission_emulator::TaintDependencyKind::Data
                    && hit.sink == "syscall buffer"
                    && hit.detail.starts_with("write arg1")
                    && hit.sources.iter().any(|source| source == "read")
            }),
            "MOVZX must preserve direct data provenance (interpreter={force_interpreter}): {:?}",
            emu.taint.hits
        );
    }
}

#[test]
fn taint_range_marks_guest_memory_as_a_source() {
    // Independent of any syscall: explicitly mark a guest memory range and
    // verify that it has a source label.
    let mut emu = build(b"A");
    emu.set_shadow_mode(ShadowMode::Taint);
    let scratch = 0x7FFF_0000u64;
    emu.taint_range(scratch, 8, "test source");
    let ram = emu.state.ram_space();

    let set = emu
        .state
        .get_shadow_memory(ram, scratch)
        .expect("marking a range left it clean");
    assert_eq!(emu.taint.sources().len(), 1, "one call, one source");
    assert_eq!(emu.taint.labels(set), vec!["test source"]);
}
