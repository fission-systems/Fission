//! SLEIGH-runtime function discovery.
//!
//! This module owns analyzer-level function discovery. `fission-loader` only
//! contributes authoritative binary metadata; direct-control-flow recovery is
//! derived from decoded instructions here.

use std::collections::BTreeSet;

use fission_loader::{FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedReferenceKind, RuntimeFrontendStatus, RuntimeSleighFrontend,
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
    let Some(language) = runtime_language_for(binary) else {
        report.unsupported_runtime = true;
        return report;
    };

    let Ok(frontend) = RuntimeSleighFrontend::new_for_language(language) else {
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

            for reference in instruction.references {
                match reference.kind {
                    DecodedReferenceKind::CallTarget => {
                        call_targets.insert(normalize_target(binary, reference.target));
                    }
                    DecodedReferenceKind::BranchTarget => {
                        jump_targets.insert(normalize_target(binary, reference.target));
                    }
                    DecodedReferenceKind::MemoryAddress
                    | DecodedReferenceKind::ImmediateAddress
                    | DecodedReferenceKind::RipRelativeAddress => {}
                }
            }
        }
    }

    report.call_target_count = call_targets.len();
    report.jump_target_count = jump_targets.len();

    let mut candidates = call_targets;
    if profile == FunctionDiscoveryProfile::Aggressive {
        candidates.extend(jump_targets);
    }

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
            });
        }
        binary.functions.sort_by_key(|function| function.address);
        binary.functions_sorted = true;
        binary.rebuild_function_indices();
    }

    report
}

fn runtime_language_for(binary: &LoadedBinary) -> Option<&str> {
    binary.sleigh_language_id()
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

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    fn synthetic_binary(bytes: Vec<u8>, is_64bit: bool) -> LoadedBinary {
        LoadedBinaryBuilder::new("synthetic.bin".to_string(), DataBuffer::Heap(bytes.clone()))
            .arch_spec(if is_64bit {
                "x86:LE:64:default"
            } else {
                "x86:LE:32:default"
            })
            .entry_point(0x1000)
            .image_base(0)
            .is_64bit(is_64bit)
            .format("test")
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: bytes.len() as u64,
                file_offset: 0,
                file_size: bytes.len() as u64,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("synthetic binary")
    }

    #[test]
    fn function_discovery_collects_direct_call_targets() {
        let mut bytes = vec![0xcc; 0x200];
        bytes[0..5].copy_from_slice(&[0xe8, 0xfb, 0x00, 0x00, 0x00]);
        let mut binary = synthetic_binary(bytes, true);

        let report =
            discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Balanced);

        assert!(!report.unsupported_runtime);
        assert!(report.decoded_instruction_count > 0);
        assert!(report.call_target_count >= 1);
        assert_eq!(report.accepted_function_count, 1);
        assert!(binary.function_at_exact(0x1100).is_some());
    }

    #[test]
    fn function_discovery_only_collects_jump_targets_when_aggressive() {
        let mut bytes = vec![0xcc; 0x200];
        bytes[0..5].copy_from_slice(&[0xe9, 0xfb, 0x00, 0x00, 0x00]);
        let mut balanced = synthetic_binary(bytes.clone(), true);
        let mut aggressive = synthetic_binary(bytes, true);

        let balanced_report =
            discover_functions_with_runtime(&mut balanced, FunctionDiscoveryProfile::Balanced);
        let aggressive_report =
            discover_functions_with_runtime(&mut aggressive, FunctionDiscoveryProfile::Aggressive);

        assert!(!balanced_report.unsupported_runtime);
        assert_eq!(balanced_report.accepted_function_count, 0);
        assert!(!aggressive_report.unsupported_runtime);
        assert_eq!(aggressive_report.accepted_function_count, 1);
        assert!(aggressive.function_at_exact(0x1100).is_some());
    }

    #[test]
    fn function_discovery_fails_closed_for_unsupported_runtime() {
        let mut binary = synthetic_binary(vec![0xcc; 0x20], false);

        let report =
            discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Aggressive);

        assert!(report.unsupported_runtime);
        assert_eq!(report.accepted_function_count, 0);
        assert!(binary.functions.is_empty());
    }
}
