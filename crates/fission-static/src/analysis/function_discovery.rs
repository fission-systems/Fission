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
                    // x86-64 PC-relative operands surface as RipRelativeAddress while still being
                    // direct control-flow edges — mirror Ghidra-style CALL/JMP continuation facts.
                    DecodedReferenceKind::RipRelativeAddress => match instruction.flow_kind {
                        DecodedFlowKind::Call => {
                            call_targets.insert(normalize_target(binary, reference.target));
                        }
                        DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
                            jump_targets.insert(normalize_target(binary, reference.target));
                        }
                        // Unconditional `jmp rel*` sometimes retains `FlowKind::None` while still
                        // emitting a PC-relative operand reference (mirrors Ghidra listing quirks).
                        DecodedFlowKind::None
                            if instruction.mnemonic.eq_ignore_ascii_case("jmp") =>
                        {
                            jump_targets.insert(normalize_target(binary, reference.target));
                        }
                        _ => {}
                    },
                    DecodedReferenceKind::MemoryAddress
                    | DecodedReferenceKind::ImmediateAddress => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    /// Absolute VA + bytes borrowed from `vendor_x86_pe_call_rel32_uses_construct_inst_next_extent`
    /// (`fission-sleigh` compiled_table tests). At this IP the decoder materializes
    /// `BoundOperand::Relative`, which `discover_functions_with_runtime` maps to `direct_target`.
    const VENDOR_DECODE_VA: u64 = 0x4014ed;
    const VENDOR_CALL_REL32: [u8; 5] = [0xe8, 0x0e, 0x0d, 0x00, 0x00];
    const VENDOR_CALL_TARGET: u64 = 0x402200;
    /// Same PC-relative displacement as [`VENDOR_CALL_REL32`]; unconditional encode only swaps opcode.
    const VENDOR_JUMP_REL32: [u8; 5] = [
        0xe9,
        VENDOR_CALL_REL32[1],
        VENDOR_CALL_REL32[2],
        VENDOR_CALL_REL32[3],
        VENDOR_CALL_REL32[4],
    ];

    fn synthetic_pe64_vendor_site(first_insn: [u8; 5]) -> LoadedBinary {
        let mut bytes = vec![0xcc; 0x3000];
        bytes[0..5].copy_from_slice(&first_insn);
        LoadedBinaryBuilder::new("synthetic.bin".to_string(), DataBuffer::Heap(bytes.clone()))
            .format("PE")
            .image_base(0x400000)
            .entry_point(VENDOR_DECODE_VA)
            .arch_spec("x86:LE:64:default")
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: VENDOR_DECODE_VA,
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
        let mut binary = synthetic_pe64_vendor_site(VENDOR_CALL_REL32);

        let report =
            discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Balanced);

        assert!(!report.unsupported_runtime);
        assert!(report.decoded_instruction_count > 0);
        assert!(report.call_target_count >= 1);
        assert_eq!(report.accepted_function_count, 1);
        assert!(binary.function_at_exact(VENDOR_CALL_TARGET).is_some());
    }

    #[test]
    fn function_discovery_only_collects_jump_targets_when_aggressive() {
        let mut balanced = synthetic_pe64_vendor_site(VENDOR_JUMP_REL32);
        let mut aggressive = synthetic_pe64_vendor_site(VENDOR_JUMP_REL32);

        let balanced_report =
            discover_functions_with_runtime(&mut balanced, FunctionDiscoveryProfile::Balanced);
        let aggressive_report =
            discover_functions_with_runtime(&mut aggressive, FunctionDiscoveryProfile::Aggressive);

        assert!(!balanced_report.unsupported_runtime);
        assert_eq!(balanced_report.accepted_function_count, 0);
        assert!(!aggressive_report.unsupported_runtime);
        assert_eq!(aggressive_report.accepted_function_count, 1);
        assert!(aggressive.function_at_exact(VENDOR_CALL_TARGET).is_some());
    }

    /// Unknown `language_id` must resolve as unsupported **before** mutating discovered functions.
    #[test]
    fn function_discovery_fails_closed_for_unsupported_runtime() {
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
