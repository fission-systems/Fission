use fission_loader::loader::{DataBuffer, FunctionCandidateInfo, LoadedBinaryBuilder, SectionInfo};
use fission_sleigh::runtime::{DecodedInstruction, DecodedReference};
use std::sync::{Mutex, OnceLock};

use super::{FunctionDiscoveryProfile, discover_functions_with_runtime};
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
fn discovery_candidate_targets_balanced_excludes_jump_only() {
    let calls = vec![0x401000_u64];
    let jumps = vec![0x402200_u64];
    let balanced = discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
    assert_eq!(balanced, vec![0x401000]);
    let aggressive =
        discovery_candidate_targets(FunctionDiscoveryProfile::Aggressive, balanced, &jumps);
    assert!(aggressive.contains(&0x401000));
    assert!(
        !aggressive.contains(&0x402200),
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

    let candidates = discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
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
fn discover_balanced_skips_jump_only_even_when_executable() {
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
    let candidates = discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
    assert!(candidates.is_empty());
    assert!(binary.functions.is_empty());
}

#[test]
fn discover_aggressive_excludes_jump_target_inside_executable() {
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
    let candidates =
        discovery_candidate_targets(FunctionDiscoveryProfile::Aggressive, calls, &jumps);
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

    let report = discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Aggressive);

    assert!(report.unsupported_runtime);
    assert_eq!(report.accepted_function_count, 0);
    assert!(binary.functions.is_empty());
}

#[test]
fn function_discovery_promotes_only_sleigh_validated_symbol_seeds() {
    let _guard = sleigh_runtime_discovery_lock();
    let mut binary = pe64_symbol_seed_shell();

    let report =
        discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Conservative);

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

/// Ghidra `AggressiveInstructionFinderAnalyzer` scorecard item. Lays out 20
/// "known" (symbol-seeded) functions sharing an identical two-instruction
/// prologue (`push r15; push r14`, a deliberately uncommon register choice
/// so this doesn't also happen to collide with a real signature in
/// `fission-signatures`' Ghidra XML pattern DB, which `scan_ghidra_patterns`
/// already checks under every non-Conservative profile), each terminated by
/// `ret`, separated from each other by 4-byte `0xcc` padding runs -- plus
/// one more, identically-shaped function placed the same way but *not*
/// seeded, simulating a function hidden in a stripped binary's padding gap.
/// `scan_dynamic_prologues` should recognize the hidden function's prologue
/// as recurring >= 4 times among the 20 known functions and recover it --
/// but only under `Aggressive`, mirroring Ghidra's own AIF being strictly
/// riskier than the reference/signature-driven scanners that also run under
/// `Balanced`.
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
fn aggressive_instruction_finder_recovers_recurring_gap_prologue() {
    let _guard = sleigh_runtime_discovery_lock();
    let (mut binary, hidden_address) = pe64_aif_gap_shell();

    let report = discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Aggressive);

    assert!(!report.unsupported_runtime);
    assert!(
        binary.function_addr_index.contains_key(&hidden_address),
        "AIF should have recovered the padding-hidden function sharing the \
         20 known functions' recurring prologue shape"
    );
}

#[test]
fn aggressive_instruction_finder_does_not_run_under_balanced_or_conservative() {
    let _guard = sleigh_runtime_discovery_lock();

    let (mut conservative_binary, hidden_address) = pe64_aif_gap_shell();
    discover_functions_with_runtime(
        &mut conservative_binary,
        FunctionDiscoveryProfile::Conservative,
    );
    assert!(
        !conservative_binary
            .function_addr_index
            .contains_key(&hidden_address),
        "Conservative must not run the gap scanner at all"
    );

    let (mut balanced_binary, hidden_address) = pe64_aif_gap_shell();
    discover_functions_with_runtime(&mut balanced_binary, FunctionDiscoveryProfile::Balanced);
    assert!(
        !balanced_binary
            .function_addr_index
            .contains_key(&hidden_address),
        "Balanced must not run the riskier AIF-style scanner -- only Aggressive should"
    );
}
