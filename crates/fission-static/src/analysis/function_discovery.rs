//! SLEIGH-runtime function discovery.
//!
//! This module owns analyzer-level function discovery. `fission-loader` only
//! contributes authoritative binary metadata; direct-control-flow recovery is
//! derived from decoded instructions here.

use std::collections::BTreeSet;

use fission_loader::{FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReferenceKind, RuntimeFrontendStatus,
    RuntimeSleighFrontend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionDiscoveryProfile {
    /// Collect direct call targets only.
    Conservative,
    /// Collect direct call targets only; reserved for future analyzer budgets.
    Balanced,
    /// Collect direct call and branch targets.
    Aggressive,
}

impl Default for FunctionDiscoveryProfile {
    fn default() -> Self {
        Self::Conservative
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionDiscoveryReport {
    pub decoded_instruction_count: usize,
    pub call_target_count: usize,
    pub jump_target_count: usize,
    pub accepted_function_count: usize,
    pub unsupported_runtime: bool,
}

pub fn discover_functions_with_runtime(
    binary: &mut LoadedBinary,
    profile: FunctionDiscoveryProfile,
) -> FunctionDiscoveryReport {
    let mut report = FunctionDiscoveryReport::default();
    let Some(load_spec) = runtime_load_spec_for(binary) else {
        report.unsupported_runtime = true;
        return report;
    };

    let Ok(frontend) = RuntimeSleighFrontend::new_for_load_spec(load_spec) else {
        report.unsupported_runtime = true;
        return report;
    };
    if frontend.status() != RuntimeFrontendStatus::ExecutableCandidate {
        report.unsupported_runtime = true;
        return report;
    }

    let executable_ranges = executable_ranges(binary);
    let mut call_targets = BTreeSet::new();
    let mut jump_targets = BTreeSet::new();

    for section in binary
        .sections
        .iter()
        .filter(|section| section.is_executable)
    {
        let file_start = section.file_offset as usize;
        let size = section.file_size.min(section.virtual_size) as usize;
        if size == 0 || file_start >= binary.data.as_slice().len() {
            continue;
        }
        let file_end = file_start
            .saturating_add(size)
            .min(binary.data.as_slice().len());
        if file_end <= file_start {
            continue;
        };
        let bytes = &binary.data.as_slice()[file_start..file_end];
        let Ok(decoded) = frontend.decode_window(bytes, section.virtual_address, bytes.len())
        else {
            continue;
        };
        report.decoded_instruction_count += decoded.len();

        for instruction in decoded {
            collect_instruction_targets(binary, &instruction, &mut call_targets, &mut jump_targets);
        }
    }

    report.call_target_count = call_targets.len();
    report.jump_target_count = jump_targets.len();

    let candidates = discovery_candidate_targets(profile, call_targets, &jump_targets);

    let mut accepted = Vec::new();
    for target in candidates {
        if binary.function_addr_index.contains_key(&target) {
            continue;
        }
        if is_in_executable_ranges(target, &executable_ranges) {
            accepted.push(target);
        }
    }

    report.accepted_function_count = accepted.len();
    if !accepted.is_empty() {
        for address in accepted {
            binary.functions.push(FunctionInfo {
                name: format!("sub_{address:x}"),
                address,
                size: 0,
                is_export: false,
                is_import: false,
                ..Default::default()
            });
        }
        binary.functions.sort_by_key(|function| function.address);
        binary.functions_sorted = true;
        binary.rebuild_function_indices();
    }

    report
}

fn runtime_load_spec_for(binary: &LoadedBinary) -> Option<&fission_loader::loader::BinaryLoadSpec> {
    binary.load_spec()
}

fn executable_ranges(binary: &LoadedBinary) -> Vec<(u64, u64)> {
    binary
        .sections
        .iter()
        .filter(|section| section.is_executable)
        .map(|section| {
            (
                section.virtual_address,
                section.virtual_address.saturating_add(section.virtual_size),
            )
        })
        .collect()
}

fn is_in_executable_ranges(target: u64, ranges: &[(u64, u64)]) -> bool {
    ranges
        .iter()
        .any(|&(start, end)| target >= start && target < end)
}

fn normalize_target(binary: &LoadedBinary, target: u64) -> u64 {
    if binary.is_64bit {
        target
    } else {
        target & 0xffff_ffff
    }
}

/// Accumulate direct CFG targets from one decoded instruction (including PC-relative operands).
fn collect_instruction_targets(
    binary: &LoadedBinary,
    instruction: &DecodedInstruction,
    call_targets: &mut BTreeSet<u64>,
    jump_targets: &mut BTreeSet<u64>,
) {
    match instruction.flow_kind {
        DecodedFlowKind::Call => {
            if let Some(target) = instruction.direct_target {
                call_targets.insert(normalize_target(binary, target));
            }
        }
        DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
            if let Some(target) = instruction.direct_target {
                jump_targets.insert(normalize_target(binary, target));
            }
        }
        DecodedFlowKind::None
        | DecodedFlowKind::Return
        | DecodedFlowKind::Interrupt
        | DecodedFlowKind::Syscall => {}
    }

    for reference in &instruction.references {
        match reference.kind {
            DecodedReferenceKind::CallTarget => {
                call_targets.insert(normalize_target(binary, reference.target));
            }
            DecodedReferenceKind::BranchTarget => {
                jump_targets.insert(normalize_target(binary, reference.target));
            }
            DecodedReferenceKind::RipRelativeAddress => match instruction.flow_kind {
                DecodedFlowKind::Call => {
                    call_targets.insert(normalize_target(binary, reference.target));
                }
                DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
                    jump_targets.insert(normalize_target(binary, reference.target));
                }
                DecodedFlowKind::None if instruction.mnemonic.eq_ignore_ascii_case("jmp") => {
                    jump_targets.insert(normalize_target(binary, reference.target));
                }
                _ => {}
            },
            DecodedReferenceKind::MemoryAddress | DecodedReferenceKind::ImmediateAddress => {}
        }
    }
}

fn discovery_candidate_targets(
    profile: FunctionDiscoveryProfile,
    mut call_targets: BTreeSet<u64>,
    jump_targets: &BTreeSet<u64>,
) -> BTreeSet<u64> {
    if profile == FunctionDiscoveryProfile::Aggressive {
        call_targets.extend(jump_targets.iter().copied());
    }
    call_targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
    use fission_sleigh::runtime::{DecodedInstruction, DecodedReference};
    use std::sync::{Mutex, OnceLock};

    /// Serialize tests that construct `RuntimeSleighFrontend`; parallel harness runs have flaked
    /// when multiple threads initialize/use the same x86-64 decode path concurrently.
    fn sleigh_runtime_discovery_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn pe64_executable_shell() -> LoadedBinary {
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
        let mut calls = BTreeSet::new();
        let mut jumps = BTreeSet::new();
        collect_instruction_targets(&binary, &call_with_rip_reference(), &mut calls, &mut jumps);
        assert!(calls.contains(&SAMPLE_RIP_TARGET));
        assert!(jumps.is_empty());
    }

    #[test]
    fn collect_instruction_targets_treats_rip_relative_as_jump_for_jmp_mnemonic() {
        let binary = pe64_executable_shell();
        let mut calls = BTreeSet::new();
        let mut jumps = BTreeSet::new();
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
        let calls = BTreeSet::from([0x401000_u64]);
        let jumps = BTreeSet::from([0x402200_u64]);
        let balanced =
            discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
        assert_eq!(balanced, BTreeSet::from([0x401000]));
        let aggressive =
            discovery_candidate_targets(FunctionDiscoveryProfile::Aggressive, balanced, &jumps);
        assert!(aggressive.contains(&0x401000));
        assert!(aggressive.contains(&0x402200));
    }

    #[test]
    fn discover_accepts_call_target_inside_executable_when_not_known() {
        let binary = pe64_executable_shell();
        assert!(binary.function_addr_index.get(&SAMPLE_RIP_TARGET).is_none());

        let mut calls = BTreeSet::new();
        let mut jumps = BTreeSet::new();
        collect_instruction_targets(&binary, &call_with_rip_reference(), &mut calls, &mut jumps);

        let candidates =
            discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
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
        let mut calls = BTreeSet::new();
        let mut jumps = BTreeSet::new();
        collect_instruction_targets(
            &binary,
            &jmp_with_rip_reference_flow_none(),
            &mut calls,
            &mut jumps,
        );
        let candidates =
            discovery_candidate_targets(FunctionDiscoveryProfile::Balanced, calls, &jumps);
        assert!(candidates.is_empty());
        assert!(binary.functions.is_empty());
    }

    #[test]
    fn discover_aggressive_accepts_jump_target_inside_executable() {
        let binary = pe64_executable_shell();
        let mut calls = BTreeSet::new();
        let mut jumps = BTreeSet::new();
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

        let report =
            discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Aggressive);

        assert!(report.unsupported_runtime);
        assert_eq!(report.accepted_function_count, 0);
        assert!(binary.functions.is_empty());
    }
}
