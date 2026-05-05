use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
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
    let mut calls = std::collections::BTreeSet::new();
    let mut jumps = std::collections::BTreeSet::new();
    collect_instruction_targets(&binary, &call_with_rip_reference(), &mut calls, &mut jumps);
    assert!(calls.contains(&SAMPLE_RIP_TARGET));
    assert!(jumps.is_empty());
}

#[test]
fn collect_instruction_targets_treats_rip_relative_as_jump_for_jmp_mnemonic() {
    let binary = pe64_executable_shell();
    let mut calls = std::collections::BTreeSet::new();
    let mut jumps = std::collections::BTreeSet::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
    );
    assert!(calls.is_empty());
    assert!(jumps.contains(&SAMPLE_RIP_TARGET));
}

#[test]
fn discovery_candidate_targets_balanced_excludes_jump_only() {
    let calls = std::collections::BTreeSet::from([0x401000_u64]);
    let jumps = std::collections::BTreeSet::from([0x402200_u64]);
    let balanced = discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
    assert_eq!(balanced, std::collections::BTreeSet::from([0x401000]));
    let aggressive =
        discovery_candidate_targets(FunctionDiscoveryProfile::Aggressive, balanced, &jumps);
    assert!(aggressive.contains(&0x401000));
    assert!(aggressive.contains(&0x402200));
}

#[test]
fn discover_accepts_call_target_inside_executable_when_not_known() {
    let binary = pe64_executable_shell();
    assert!(binary.function_addr_index.get(&SAMPLE_RIP_TARGET).is_none());

    let mut calls = std::collections::BTreeSet::new();
    let mut jumps = std::collections::BTreeSet::new();
    collect_instruction_targets(&binary, &call_with_rip_reference(), &mut calls, &mut jumps);

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
    let mut calls = std::collections::BTreeSet::new();
    let mut jumps = std::collections::BTreeSet::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
    );
    let candidates = discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
    assert!(candidates.is_empty());
    assert!(binary.functions.is_empty());
}

#[test]
fn discover_aggressive_accepts_jump_target_inside_executable() {
    let binary = pe64_executable_shell();
    let mut calls = std::collections::BTreeSet::new();
    let mut jumps = std::collections::BTreeSet::new();
    collect_instruction_targets(
        &binary,
        &jmp_with_rip_reference_flow_none(),
        &mut calls,
        &mut jumps,
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
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0], SAMPLE_RIP_TARGET);
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
