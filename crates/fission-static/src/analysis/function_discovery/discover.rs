use std::collections::BTreeSet;

use fission_loader::{FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::{RuntimeFrontendStatus, RuntimeSleighFrontend};

use super::ranges::{executable_ranges, is_in_executable_ranges, runtime_load_spec_for};
use super::targets::{collect_instruction_targets, discovery_candidate_targets};
use super::types::{FunctionDiscoveryProfile, FunctionDiscoveryReport};

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
        report.decoded_instruction_count += collect_section_targets(
            binary,
            &frontend,
            profile,
            bytes,
            section.virtual_address,
            &mut call_targets,
            &mut jump_targets,
        );
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

fn collect_section_targets(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    profile: FunctionDiscoveryProfile,
    bytes: &[u8],
    base_address: u64,
    call_targets: &mut BTreeSet<u64>,
    jump_targets: &mut BTreeSet<u64>,
) -> usize {
    if profile == FunctionDiscoveryProfile::Conservative {
        let Ok(decoded) = frontend.decode_window(bytes, base_address, bytes.len()) else {
            return 0;
        };
        let decoded_count = decoded.len();
        for instruction in decoded {
            collect_instruction_targets(binary, &instruction, call_targets, jump_targets);
        }
        return decoded_count;
    }

    collect_section_targets_resync(
        binary,
        frontend,
        bytes,
        base_address,
        call_targets,
        jump_targets,
    )
}

fn collect_section_targets_resync(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    bytes: &[u8],
    base_address: u64,
    call_targets: &mut BTreeSet<u64>,
    jump_targets: &mut BTreeSet<u64>,
) -> usize {
    let mut decoded_count = 0usize;
    let mut offset = 0usize;
    let mut current = base_address;

    while offset < bytes.len() {
        let remaining = &bytes[offset..];
        if let Ok(instructions) = frontend.decode_window(remaining, current, remaining.len().min(4096)) {
            if !instructions.is_empty() {
                let mut batch_bytes = 0;
                for instruction in &instructions {
                    batch_bytes += instruction.length;
                    collect_instruction_targets(binary, instruction, call_targets, jump_targets);
                    decoded_count += 1;
                }
                offset = offset.saturating_add(batch_bytes);
                current = current.saturating_add(batch_bytes as u64);
                continue;
            }
        }

        match frontend.decode_instruction_with_context_override(remaining, current, None) {
            Ok(instruction) if instruction.length > 0 && instruction.length <= remaining.len() => {
                let step = instruction.length;
                collect_instruction_targets(binary, &instruction, call_targets, jump_targets);
                decoded_count += 1;
                offset = offset.saturating_add(step);
                current = current.saturating_add(step as u64);
            }
            _ => {
                offset = offset.saturating_add(1);
                current = current.saturating_add(1);
            }
        }
    }

    decoded_count
}
