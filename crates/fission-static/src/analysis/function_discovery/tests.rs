use fission_loader::loader::{DataBuffer, FunctionCandidateInfo, LoadedBinaryBuilder, SectionInfo};
use fission_sleigh::runtime::{DecodedInstruction, DecodedReference};
use std::sync::{Mutex, OnceLock};

use super::discover_functions_with_runtime;
use crate::analysis::function_discovery::ranges::{executable_ranges, is_in_executable_ranges};
use crate::analysis::function_discovery::targets::{
    collect_instruction_targets, discovery_candidate_targets,
};

/// Serialize tests that construct `RuntimeSleighFrontend`; parallel harness runs have flaked
/// when multiple threads initialize/use the same x86-64 decode path concurrently.
fn sleigh_runtime_discovery_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pe64_executable_shell() -> fission_loader::LoadedBinary {
    LoadedBinaryBuilder::new("unit.bin".to_string(), DataBuffer::Heap(vec![0xcc; 0x1000]))
        .format("PE")
        .image_base(0x400000)
        .entry_point(0x401000)
        .arch_spec("x86:LE:64:default")
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x1000,
            file_offset: 0,
            file_size: 0x1000,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("synthetic binary")
}

fn pe64_symbol_seed_shell() -> fission_loader::LoadedBinary {
    let mut bytes = vec![0xcc; 0x1000];
    bytes[0x100] = 0xc3; // RET
    bytes[0x120..0x130].fill(0xff); // executable-section data, not a routine
    bytes[0x140..0x146].copy_from_slice(&[0xff, 0x25, 0xba, 0x01, 0x00, 0x00]);

    let mut binary =
        LoadedBinaryBuilder::new("symbol-seeds.bin".to_string(), DataBuffer::Heap(bytes))
            .format("PE")
            .image_base(0x400000)
            .entry_point(0x401000)
            .arch_spec("x86:LE:64:default")
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x401000,
                virtual_size: 0x1000,
                file_offset: 0,
                file_size: 0x1000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_iat_symbol(0x401300, "example.dll!target".to_string())
            .build()
            .expect("synthetic binary");
    binary.function_candidates = vec![
        FunctionCandidateInfo {
            address: 0x401100,
            name: "return_seed".to_string(),
            origin: "synthetic-symbol".to_string(),
            source_section: Some(".text".to_string()),
        },
        FunctionCandidateInfo {
            address: 0x401120,
            name: "data_seed".to_string(),
            origin: "synthetic-symbol".to_string(),
            source_section: Some(".text".to_string()),
        },
        FunctionCandidateInfo {
            address: 0x401140,
            name: "import_seed".to_string(),
            origin: "synthetic-symbol".to_string(),
            source_section: Some(".text".to_string()),
        },
    ];
    binary
}

const SAMPLE_RIP_TARGET: u64 = 0x401800;

fn call_with_rip_reference() -> DecodedInstruction {
    use fission_sleigh::runtime::{DecodedFlowKind, DecodedReferenceKind};
    DecodedInstruction {
        address: 0x4014ed,
        bytes: vec![0xe8, 0x0e, 0x0d, 0x00, 0x00],
        length: 5,
        mnemonic: "CALL".into(),
        operands_text: String::new(),
        flow_kind: DecodedFlowKind::Call,
        direct_target: None,
        references: vec![DecodedReference {
            target: SAMPLE_RIP_TARGET,
            kind: DecodedReferenceKind::RipRelativeAddress,
            operand_index: 0,
        }],
        pending_context_commits: Vec::new(),
    }
}

fn jmp_with_rip_reference_flow_none() -> DecodedInstruction {
    use fission_sleigh::runtime::{DecodedFlowKind, DecodedReferenceKind};
    DecodedInstruction {
        address: 0x4014ed,
        bytes: vec![0xe9, 0x0e, 0x0d, 0x00, 0x00],
        length: 5,
        mnemonic: "JMP".into(),
        operands_text: String::new(),
        flow_kind: DecodedFlowKind::None,
        direct_target: None,
        references: vec![DecodedReference {
            target: SAMPLE_RIP_TARGET,
            kind: DecodedReferenceKind::RipRelativeAddress,
            operand_index: 0,
        }],
        pending_context_commits: Vec::new(),
    }
}

#[test]
fn collect_instruction_targets_treats_rip_relative_as_call_on_call_flow() {
    let binary = pe64_executable_shell();
    let mut calls = Vec::new();
    let mut jumps = Vec::new();
    collect_instruction_targets(
        &binary,
        &call_with_rip_reference(),
        &mut calls,
        &mut jumps,
        &mut Vec::new(),
    );
    assert!(calls.contains(&SAMPLE_RIP_TARGET));
    assert!(jumps.is_empty());
}

#[test]
fn collect_instruction_targets_treats_rip_relative_as_jump_for_jmp_mnemonic() {
    let binary = pe64_executable_shell();
    let mut calls = Vec::new();
    let mut jumps = Vec::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
        &mut Vec::new(),
    );
    assert!(calls.is_empty());
    assert!(jumps.contains(&SAMPLE_RIP_TARGET));
}

#[test]
fn discovery_candidate_targets_exclude_jump_only() {
    let calls = vec![0x401000_u64];
    let jumps = vec![0x402200_u64];
    let candidates = discovery_candidate_targets(calls, &jumps);
    assert_eq!(candidates, vec![0x401000]);
    assert!(
        !candidates.contains(&0x402200),
        "Jump targets are no longer blindly accepted"
    );
}

#[test]
fn discover_accepts_call_target_inside_executable_when_not_known() {
    let binary = pe64_executable_shell();
    assert!(binary.function_addr_index.get(&SAMPLE_RIP_TARGET).is_none());

    let mut calls = Vec::new();
    let mut jumps = Vec::new();
    collect_instruction_targets(
        &binary,
        &call_with_rip_reference(),
        &mut calls,
        &mut jumps,
        &mut Vec::new(),
    );

    let candidates = discovery_candidate_targets(calls, &jumps);
    let executable_ranges = executable_ranges(&binary);
    let accepted: Vec<_> = candidates
        .into_iter()
        .filter(|&t| {
            !binary.function_addr_index.contains_key(&t)
                && is_in_executable_ranges(t, &executable_ranges)
        })
        .collect();
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0], SAMPLE_RIP_TARGET);
}

#[test]
fn discover_skips_jump_only_even_when_executable() {
    let binary = pe64_executable_shell();
    let mut calls = Vec::new();
    let mut jumps = Vec::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
        &mut Vec::new(),
    );
    let candidates = discovery_candidate_targets(calls, &jumps);
    assert!(candidates.is_empty());
    assert!(binary.functions.is_empty());
}

#[test]
fn discover_excludes_jump_target_inside_executable() {
    let binary = pe64_executable_shell();
    let mut calls = Vec::new();
    let mut jumps = Vec::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
        &mut Vec::new(),
    );
    let candidates = discovery_candidate_targets(calls, &jumps);
    let executable_ranges = executable_ranges(&binary);
    let accepted: Vec<_> = candidates
        .into_iter()
        .filter(|&t| {
            !binary.function_addr_index.contains_key(&t)
                && is_in_executable_ranges(t, &executable_ranges)
        })
        .collect();
    assert_eq!(
        accepted.len(),
        0,
        "Jump targets are no longer blindly accepted"
    );
}

/// Unknown `language_id` must resolve as unsupported **before** mutating discovered functions.
#[test]
fn function_discovery_fails_closed_for_unsupported_runtime() {
    let _guard = sleigh_runtime_discovery_lock();
    let mut binary = LoadedBinaryBuilder::new(
        "missing-runtime.bin".to_string(),
        DataBuffer::Heap(vec![0xcc; 0x20]),
    )
    .format("PE")
    .image_base(0x140000000)
    .entry_point(0x140001000)
    .arch_spec("__NONEXISTENT_SLEIGH_LANGUAGE__:LE:32:default")
    .is_64bit(false)
    .build()
    .expect("builder");

    let report = discover_functions_with_runtime(&mut binary);

    assert!(report.unsupported_runtime);
    assert_eq!(report.accepted_function_count, 0);
    assert!(binary.functions.is_empty());
}

#[test]
fn function_discovery_promotes_only_sleigh_validated_symbol_seeds() {
    let _guard = sleigh_runtime_discovery_lock();
    let mut binary = pe64_symbol_seed_shell();

    let report = discover_functions_with_runtime(&mut binary);

    assert!(!report.unsupported_runtime);
    assert!(binary.function_addr_index.contains_key(&0x401100));
    assert!(binary.function_addr_index.contains_key(&0x401140));
    assert!(!binary.function_addr_index.contains_key(&0x401120));
    assert_eq!(
        binary
            .function_at(0x401100)
            .map(|function| function.name.as_str()),
        Some("return_seed")
    );
    let thunk = binary.function_at(0x401140).expect("import thunk");
    assert!(thunk.is_thunk_like);
    assert_eq!(thunk.name, "target");
    assert_eq!(thunk.kind.as_deref(), Some("import_thunk"));
    assert_eq!(thunk.thunk_target, Some(0x401300));
    assert_eq!(thunk.external_library.as_deref(), Some("example.dll"));
}

#[test]
fn discovery_keeps_known_call_targets_and_still_promotes_unknown_targets() {
    let _guard = sleigh_runtime_discovery_lock();
    let mut binary = pe64_executable_shell();
    // Two direct calls, to 0x401020 and 0x401040, followed by a return.
    binary
        .patch_bytes(0, &[0xe8, 0x1b, 0, 0, 0, 0xe8, 0x36, 0, 0, 0, 0xc3])
        .unwrap();
    for offset in [0x20, 0x40] {
        // push rbp; mov rbp, rsp; pop rbp; ret
        binary
            .patch_bytes(offset, &[0x55, 0x48, 0x89, 0xe5, 0x5d, 0xc3])
            .unwrap();
    }
    binary.functions.push(fission_loader::FunctionInfo {
        address: 0x401020,
        size: 6,
        name: "known_callee".into(),
        ..Default::default()
    });
    binary.rebuild_function_indices();

    let report = discover_functions_with_runtime(&mut binary);

    assert_eq!(report.call_target_count, 2);
    assert_eq!(report.accepted_function_count, 1);
    let known = binary.function_at(0x401020).unwrap();
    assert_eq!(known.name, "known_callee");
    assert_eq!(known.size, 6);
    assert!(binary.function_at(0x401040).is_some());
}

/// Twenty "known" (symbol-seeded) functions share an identical two-instruction
/// prologue (`push r15; push r14`), separated by `0xcc` padding, plus one more
/// identically shaped function in the same padding pattern that nothing
/// references. A scanner that trusted the binary's own recurring prologue shape
/// would recover it; discovery deliberately does not (see
/// `recurring_prologue_in_padding_gap_is_not_guessed`).
fn pe64_aif_gap_shell() -> (fission_loader::LoadedBinary, u64) {
    const FUNC_BYTES: [u8; 5] = [0x41, 0x57, 0x41, 0x56, 0xc3]; // push r15; push r14; ret
    const STRIDE: u64 = 9; // 5-byte body + 4-byte 0xcc pad
    const KNOWN_COUNT: usize = 20;
    const BASE: u64 = 0x401000;

    let mut bytes = vec![0xcc_u8; 0x1000];
    for slot in 0..=KNOWN_COUNT {
        let offset = (slot as u64 * STRIDE) as usize;
        bytes[offset..offset + FUNC_BYTES.len()].copy_from_slice(&FUNC_BYTES);
    }
    let hidden_address = BASE + (KNOWN_COUNT as u64) * STRIDE;

    let mut binary = LoadedBinaryBuilder::new("aif-gap.bin".to_string(), DataBuffer::Heap(bytes))
        .format("PE")
        .image_base(0x400000)
        .entry_point(BASE)
        .arch_spec("x86:LE:64:default")
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: BASE,
            virtual_size: 0x1000,
            file_offset: 0,
            file_size: 0x1000,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("synthetic binary");

    binary.function_candidates = (0..KNOWN_COUNT)
        .map(|slot| FunctionCandidateInfo {
            address: BASE + (slot as u64) * STRIDE,
            name: format!("known_{slot}"),
            origin: "synthetic-symbol".to_string(),
            source_section: Some(".text".to_string()),
        })
        .collect();

    (binary, hidden_address)
}

#[test]
fn recurring_prologue_in_padding_gap_is_not_guessed() {
    let _guard = sleigh_runtime_discovery_lock();
    let (mut binary, hidden_address) = pe64_aif_gap_shell();

    discover_functions_with_runtime(&mut binary);

    assert!(
        !binary.function_addr_index.contains_key(&hidden_address),
        "a function nothing references is not promoted on prologue shape alone: \
         on 224 stripped benchmark binaries the self-calibrating prologue scanner \
         added 6 addresses and none was a benchmark target"
    );
}

#[test]
fn validate_after_condition_handles_section_start_defined_ptr_and_padding() {
    use super::ghidra_patterns::matches_pattern_after_condition;

    let mut bytes = vec![0x00; 0x100];
    bytes[0x10] = 0x90; // NOP padding before 0x401011
    bytes[0x20] = 0xcc; // INT3 padding before 0x401021
    bytes[0x30] = 0x55; // non-padding code byte before 0x401031
    let binary = LoadedBinaryBuilder::new("after.bin".to_string(), DataBuffer::Heap(bytes))
        .format("PE")
        .image_base(0x400000)
        .entry_point(0x401000)
        .arch_spec("x86:LE:64:default")
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x100,
            file_offset: 0,
            file_size: 0x100,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("synthetic binary");
    let exec_ranges = executable_ranges(&binary);
    let mut refs = std::collections::HashSet::new();
    refs.insert(0x401050);

    // `after="defined"` must accept the start of an executable section
    assert!(matches_pattern_after_condition(
        &binary,
        0x401000,
        "defined",
        &exec_ranges,
        Some(&refs)
    ));
    // `after="ptr"` requires a pointer/reference targeting `addr`
    assert!(matches_pattern_after_condition(
        &binary,
        0x401050,
        "ptr",
        &exec_ranges,
        Some(&refs)
    ));
    assert!(!matches_pattern_after_condition(
        &binary,
        0x401054,
        "ptr",
        &exec_ranges,
        Some(&refs)
    ));
    // `after="data"` accepts 0x00, 0x90, and 0xcc alignment padding
    assert!(matches_pattern_after_condition(
        &binary,
        0x401011,
        "data",
        &exec_ranges,
        Some(&refs)
    ));
    assert!(matches_pattern_after_condition(
        &binary,
        0x401021,
        "data",
        &exec_ranges,
        Some(&refs)
    ));
    assert!(!matches_pattern_after_condition(
        &binary,
        0x401031,
        "data",
        &exec_ranges,
        Some(&refs)
    ));
}

#[test]
fn instruction_coverage_merges_overlapping_and_adjacent_intervals() {
    use super::instruction_boundaries::InstructionCoverage;
    use std::collections::HashSet;

    // Simulate overlapping instruction disassemblies:
    // Inst 1: [0x1000, 0x1005)
    // Inst 2: [0x1002, 0x1006)
    // Inst 3: [0x1010, 0x1014)
    // Inst 4: [0x1014, 0x1018)  (adjacent)
    let raw = vec![
        (0x1000, 0x1005),
        (0x1002, 0x1006),
        (0x1010, 0x1014),
        (0x1014, 0x1018),
    ];
    let known = HashSet::from([0x1000]);
    let coverage = InstructionCoverage::from_raw_boundaries(raw, known);

    // Verify merged intervals:
    // [0x1000, 0x1006) and [0x1010, 0x1018)
    assert_eq!(coverage.intervals(), &[(0x1000, 0x1006), (0x1010, 0x1018)]);

    // Instruction starts and interior bytes are covered.
    assert!(coverage.covers(0x1000));
    assert!(coverage.covers(0x1001));
    assert!(coverage.covers(0x1002));
    assert!(coverage.covers(0x1005));
    assert!(!coverage.covers(0x1006));
    assert!(!coverage.covers(0x1009));
    assert!(coverage.covers(0x1010));
    assert!(coverage.covers(0x1014));
    assert!(coverage.covers(0x1017));
    assert!(!coverage.covers(0x1018));
}

#[test]
fn walk_function_boundaries_stops_at_tail_call_to_known_function() {
    let _guard = sleigh_runtime_discovery_lock();
    use super::instruction_boundaries::walk_function_boundaries;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::collections::HashSet;

    // Layout:
    // Func 1:
    // 0x401000: jmp +0x10 -> 0x401012 (eb 10) - tail call to Func 2!
    // Func 2:
    // 0x401012: nop (90)
    // 0x401013: ret (c3)
    let mut bytes = vec![0xcc_u8; 0x1000];
    bytes[0] = 0xeb;
    bytes[1] = 0x10;
    bytes[0x12] = 0x90;
    bytes[0x13] = 0xc3;

    let binary = LoadedBinaryBuilder::new("tailcall-test.bin".to_string(), DataBuffer::Heap(bytes))
        .format("PE")
        .image_base(0x400000)
        .entry_point(0x401000)
        .arch_spec("x86:LE:64:default")
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x1000,
            file_offset: 0,
            file_size: 0x1000,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("synthetic binary");

    let load_spec = binary.load_spec().expect("load spec");
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec).expect("frontend");

    // 0x401012 is known as a separate function!
    let known_functions = HashSet::from([0x401000, 0x401012]);
    let boundaries = walk_function_boundaries(
        &binary,
        &frontend,
        0x401000,
        &known_functions,
        &executable_ranges(&binary),
        None,
    );
    let sweep = super::control_flow_scan::scan_executable_sections(&binary, &frontend);
    let entry_bytes = binary.view_bytes(0x401000, 15).unwrap();
    assert!(
        sweep
            .coverage_decodes
            .contains_key(&(0x401000, entry_bytes.as_ptr() as usize))
    );
    assert_eq!(
        walk_function_boundaries(
            &binary,
            &frontend,
            0x401000,
            &known_functions,
            &executable_ranges(&binary),
            Some(&sweep.coverage_decodes),
        ),
        boundaries
    );

    assert!(
        boundaries.contains(&(0x401000, 0x401002)),
        "should contain initial jmp"
    );
    assert!(
        !boundaries.iter().any(|&(s, _)| s >= 0x401012),
        "must NOT envelope tail call target function"
    );
}

#[test]
fn walk_function_boundaries_coalesces_fallthrough_without_covering_a_branch_gap() {
    use super::instruction_boundaries::walk_function_boundaries;
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::collections::HashSet;
    use std::sync::Arc;

    let _guard = sleigh_runtime_discovery_lock();
    let mut binary = pe64_executable_shell();
    let start = binary.entry_point;
    let text = Arc::make_mut(&mut binary.data).to_mut_vec();
    // test eax,eax; jne +8; ret. The other return skips seven padding bytes.
    text[..5].copy_from_slice(&[0x85, 0xc0, 0x75, 0x08, 0xc3]);
    text[12] = 0xc3;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().unwrap()).unwrap();
    let intervals = walk_function_boundaries(
        &binary,
        &frontend,
        start,
        &HashSet::from([start]),
        &executable_ranges(&binary),
        None,
    );
    assert_eq!(intervals, [(start, start + 5), (start + 12, start + 13)]);
}

#[test]
fn walk_function_boundaries_terminates_on_noreturn_call() {
    let _guard = sleigh_runtime_discovery_lock();
    use super::instruction_boundaries::walk_function_boundaries;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::collections::HashSet;

    // Layout:
    // 0x401000: call 0x401020 (e8 1b 00 00 00) -> calls ExitProcess
    // 0x401005: nop (90)  <-- dead code / data / next function; should NOT be enveloped!
    // 0x401006: ret (c3)
    // 0x401020: jmp [rip + 0x2fe0] -> IAT slot 0x404006 (ff 25 e0 2f 00 00)
    let mut bytes = vec![0xcc_u8; 0x1000];
    // call 0x401020 (rel = 0x401020 - 0x401005 = 0x1b)
    bytes[0] = 0xe8;
    bytes[1] = 0x1b;
    bytes[2] = 0x00;
    bytes[3] = 0x00;
    bytes[4] = 0x00;
    // dead code after noreturn:
    bytes[5] = 0x90; // NOP
    bytes[6] = 0xc3; // RET

    let binary = LoadedBinaryBuilder::new("noreturn-test.bin".to_string(), DataBuffer::Heap(bytes))
        .format("PE")
        .image_base(0x400000)
        .entry_point(0x401000)
        .arch_spec("x86:LE:64:default")
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x1000,
            file_offset: 0,
            file_size: 0x1000,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_iat_symbol(0x401020, "KERNEL32.DLL!ExitProcess".to_string())
        .build()
        .expect("synthetic binary");

    let load_spec = binary.load_spec().expect("load spec");
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec).expect("frontend");

    let known_functions = HashSet::from([0x401000]);
    let boundaries = walk_function_boundaries(
        &binary,
        &frontend,
        0x401000,
        &known_functions,
        &executable_ranges(&binary),
        None,
    );

    assert!(
        boundaries.contains(&(0x401000, 0x401005)),
        "should contain noreturn call instruction"
    );
    assert!(
        !boundaries.iter().any(|&(s, _)| s >= 0x401005),
        "must NOT fall through past noreturn call"
    );
}

fn arm_mixed_mode_fixture(name: &str) -> fission_loader::LoadedBinary {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-loader/testdata")
        .join(name);
    fission_loader::LoadedBinary::from_file(&path).expect("load mixed ARM/Thumb fixture")
}

#[test]
fn a_thumb_pattern_hit_in_a_stripped_mixed_image_is_found_and_recorded_as_thumb() {
    // arm_a/b/c are ARM and thumb_a/b/c are Thumb; no symbols survive. The ARM
    // prologue patterns find the ARM functions. `thumb_c` opens `push {..., lr}`
    // and matches a Thumb pattern, which sets TMode: validated as ARM it is
    // rejected, validated as Thumb it is a function start.
    let mut binary = arm_mixed_mode_fixture("arm_mixed_mode_stripped.elf");
    assert!(binary.inner().low_bit_mode_starts.is_empty());
    discover_functions_with_runtime(&mut binary);
    let thumb_c = 0x201f4;
    assert!(
        binary.functions.iter().any(|f| f.address == thumb_c),
        "thumb_c was not discovered"
    );
    assert!(binary.runs_in_low_bit_mode(thumb_c));
    // The ARM functions found by the same scan stay ARM.
    for arm in [0x20140, 0x201b0, 0x20214] {
        assert!(!binary.runs_in_low_bit_mode(arm), "{arm:#x}");
    }
}

#[test]
fn a_symbol_marked_thumb_function_decodes_as_thumb_not_as_arm() {
    // thumb_a at 0x20188: the same bytes read as ARM are nonsense (this is what
    // decompiled to calls into unmapped addresses before the mode was kept).
    let binary = arm_mixed_mode_fixture("arm_mixed_mode.elf");
    let frontend = fission_sleigh::runtime::RuntimeSleighFrontend::new_for_load_spec(
        binary.load_spec().expect("load spec"),
    )
    .expect("ARM runtime");
    let address = 0x20188;
    let bytes = binary.view_bytes(address, 16).expect("bytes");
    let mnemonics = |decoded: Vec<fission_sleigh::runtime::DecodedInstruction>| {
        decoded
            .into_iter()
            .map(|instruction| instruction.mnemonic)
            .collect::<Vec<_>>()
    };

    let as_arm = mnemonics(
        frontend
            .decode_window_no_pcode(bytes, address, 3)
            .expect("arm"),
    );
    let thumb_context = frontend.low_bit_code_mode_override();
    let as_thumb = mnemonics(
        frontend
            .decode_window_no_pcode_in_context(bytes, address, 3, thumb_context)
            .expect("thumb"),
    );
    assert_ne!(
        as_arm, as_thumb,
        "the two readings must differ for this to prove anything"
    );

    let context = crate::analysis::decode_context_for_address(&binary, &frontend, address, None);
    let chosen = mnemonics(
        frontend
            .decode_window_no_pcode_in_context(bytes, address, 3, context)
            .expect("chosen"),
    );
    assert_eq!(chosen, as_thumb);

    // An ARM function of the same image still decodes as ARM.
    let arm_address = 0x201b0;
    let arm_bytes = binary.view_bytes(arm_address, 16).expect("bytes");
    let context =
        crate::analysis::decode_context_for_address(&binary, &frontend, arm_address, None);
    assert!(context.is_none());
    let arm_chosen = frontend
        .decode_window_no_pcode_in_context(arm_bytes, arm_address, 3, context)
        .expect("arm function");
    let arm_plain = frontend
        .decode_window_no_pcode(arm_bytes, arm_address, 3)
        .expect("arm function plain");
    assert_eq!(mnemonics(arm_chosen), mnemonics(arm_plain));
}

#[test]
fn a_code_pointer_with_the_mode_bit_set_is_a_thumb_function_start() {
    // `fn_table` in .rodata holds 0x20249: thumb_d (0x20248) with the ABI's
    // Thumb bit. The scan strips the bit to find the function, validates it in
    // Thumb, and says it is one.
    let binary = arm_mixed_mode_fixture("arm_mixed_mode_stripped.elf");
    let frontend = fission_sleigh::runtime::RuntimeSleighFrontend::new_for_load_spec(
        binary.load_spec().expect("load spec"),
    )
    .expect("ARM runtime");
    let executable_ranges = super::ranges::executable_ranges(&binary);
    // The functions thumb_d calls are already known, as they would be from
    // earlier scans.
    let known: std::collections::HashSet<u64> = [0x20140, 0x20188, 0x201b0].into();
    let found = super::data_references::scan_data_code_pointers(
        &binary,
        &frontend,
        &executable_ranges,
        &known,
        None,
    );
    let thumb_d = found
        .iter()
        .find(|start| start.address == 0x20248)
        .unwrap_or_else(|| panic!("thumb_d was not found among {found:?}"));
    assert!(thumb_d.low_bit_mode);
    assert!(
        found.iter().all(|start| start.address & 1 == 0),
        "no returned address keeps the mode bit: {found:?}"
    );
}

/// A boundary probe looks at the two bytes ending at the last non-padding byte
/// before a candidate. When that byte opens a section, the byte before it is the
/// last one of the previous section, so the window comes back one byte long.
/// The probe must read it as "no indirect jump here", not index past it.
#[test]
fn strict_boundary_probe_survives_a_window_cut_by_the_section_start() {
    use super::instruction_boundaries::is_plausible_function_boundary;

    let mut bytes = vec![0xcc_u8; 0x20];
    bytes[0x0f] = 0xff; // last byte of the first section: an `FF` opcode with no ModRM in reach
    bytes[0x10] = 0x55; // first byte of the second section, `push rbp`
    let section = |name: &str, va: u64, file_offset: u64| SectionInfo {
        name: name.to_string(),
        virtual_address: va,
        virtual_size: 0x10,
        file_offset,
        file_size: 0x10,
        is_executable: true,
        is_readable: true,
        is_writable: false,
    };
    let binary = LoadedBinaryBuilder::new("split-text.bin".to_string(), DataBuffer::Heap(bytes))
        .format("ELF")
        .image_base(0x400000)
        .entry_point(0x401000)
        .arch_spec("x86:LE:64:default")
        .architecture(fission_core::architecture::ArchitectureDescriptor {
            processor: "x86".into(),
            endian: "little".into(),
            bitness: 64,
            variant: "default".into(),
            abi: None,
            raw_machine: "x86_64".into(),
        })
        .add_section(section(".text", 0x401000, 0))
        .add_section(section(".text.cold", 0x401010, 0x10))
        .build()
        .expect("synthetic binary");

    assert!(!is_plausible_function_boundary(&binary, None, 0x401011));
}

#[test]
fn scan_data_references_decodes_big_endian_pointers() {
    let mut bytes = vec![0u8; 0x200];
    // .text at 0x1000..0x1020:
    //   0x1000: blr (0x4e80_0020) -- known callee & strict terminator before 0x1004
    //   0x1004: bl 0x1000 (0x4bff_fffd) -- call to known function so `has_call_to_valid` holds
    //   0x1008: blr (0x4e80_0020)
    bytes[0x00..0x04].copy_from_slice(&0x4e80_0020u32.to_be_bytes());
    bytes[0x04..0x08].copy_from_slice(&0x4bff_fffdu32.to_be_bytes());
    bytes[0x08..0x0c].copy_from_slice(&0x4e80_0020u32.to_be_bytes());
    // .rodata at 0x2000: big-endian pointer to 0x1004
    bytes[0x100..0x104].copy_from_slice(&0x1004u32.to_be_bytes());

    let binary = LoadedBinaryBuilder::new("be_data_ref.elf".to_string(), DataBuffer::Heap(bytes))
        .format("ELF")
        .entry_point(0x1000)
        .arch_spec("PowerPC:BE:32:default")
        .architecture(fission_core::architecture::ArchitectureDescriptor {
            processor: "PowerPC".into(),
            endian: "big".into(),
            bitness: 32,
            variant: "default".into(),
            abi: None,
            raw_machine: "EM_PPC".into(),
        })
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x1000,
            virtual_size: 0x20,
            file_offset: 0,
            file_size: 0x20,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_section(SectionInfo {
            name: ".rodata".to_string(),
            virtual_address: 0x2000,
            virtual_size: 0x20,
            file_offset: 0x100,
            file_size: 0x20,
            is_executable: false,
            is_readable: true,
            is_writable: false,
        })
        .build()
        .expect("be ELF");

    let frontend = fission_sleigh::runtime::RuntimeSleighFrontend::new_for_load_spec(
        binary.load_spec().expect("load spec"),
    )
    .expect("PowerPC runtime");
    let executable_ranges = super::ranges::executable_ranges(&binary);
    let known: std::collections::HashSet<u64> = [0x1000].into();
    let found = super::data_references::scan_data_code_pointers(
        &binary,
        &frontend,
        &executable_ranges,
        &known,
        None,
    );
    assert!(
        found.iter().any(|s| s.address == 0x1004),
        "expected big-endian .rodata pointer 0x1004 in {found:?}"
    );
}
