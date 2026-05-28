
use fission_loader::LoadedBinary;
use fission_sleigh::runtime::{DecodedFlowKind, DecodedInstruction, DecodedReferenceKind};

use super::types::FunctionDiscoveryProfile;

pub(crate) fn normalize_target(binary: &LoadedBinary, target: u64) -> u64 {
    if binary.is_64bit {
        target
    } else {
        target & 0xffff_ffff
    }
}

fn same_section(binary: &LoadedBinary, addr1: u64, addr2: u64) -> bool {
    if binary.sections.is_empty() {
        return false;
    }
    if binary.sections.len() == 1 {
        let section = &binary.sections[0];
        let start = section.virtual_address;
        let end = start.saturating_add(section.virtual_size);
        return addr1 >= start && addr1 < end && addr2 >= start && addr2 < end;
    }

    // Fast path: check if binary sections are sorted by virtual_address.
    // PE/ELF/Mach-O sections are sorted by default.
    let is_sorted = binary.sections.windows(2).all(|w| w[0].virtual_address <= w[1].virtual_address);
    if is_sorted {
        let idx1 = binary.sections.binary_search_by(|section| {
            let start = section.virtual_address;
            let end = start.saturating_add(section.virtual_size);
            if addr1 < start {
                std::cmp::Ordering::Greater
            } else if addr1 >= end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });

        if let Ok(i1) = idx1 {
            let section = &binary.sections[i1];
            let start = section.virtual_address;
            let end = start.saturating_add(section.virtual_size);
            return addr2 >= start && addr2 < end;
        }
        return false;
    }

    // Fallback to sequential search if sections list is unsorted
    for section in &binary.sections {
        let start = section.virtual_address;
        let size = section.virtual_size;
        let end = start.saturating_add(size);
        if addr1 >= start && addr1 < end && addr2 >= start && addr2 < end {
            return true;
        }
    }
    false
}

/// Accumulate direct CFG targets from one decoded instruction (including PC-relative operands).
pub(crate) fn collect_instruction_targets(
    binary: &LoadedBinary,
    instruction: &DecodedInstruction,
    call_targets: &mut Vec<u64>,
    jump_targets: &mut Vec<u64>,
) {
    match instruction.flow_kind {
        DecodedFlowKind::Call => {
            if let Some(target) = instruction.direct_target {
                call_targets.push(normalize_target(binary, target));
            }
        }
        DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
            if let Some(target) = instruction.direct_target {
                let norm_target = normalize_target(binary, target);
                let inst_addr = instruction.address;
                let distance = if inst_addr > norm_target {
                    inst_addr - norm_target
                } else {
                    norm_target - inst_addr
                };
                if distance > 512 || !same_section(binary, inst_addr, norm_target) {
                    jump_targets.push(norm_target);
                }
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
                call_targets.push(normalize_target(binary, reference.target));
            }
            DecodedReferenceKind::BranchTarget => {
                let norm_target = normalize_target(binary, reference.target);
                let inst_addr = instruction.address;
                let distance = if inst_addr > norm_target {
                    inst_addr - norm_target
                } else {
                    norm_target - inst_addr
                };
                if distance > 512 || !same_section(binary, inst_addr, norm_target) {
                    jump_targets.push(norm_target);
                }
            }
            DecodedReferenceKind::RipRelativeAddress => match instruction.flow_kind {
                DecodedFlowKind::Call => {
                    call_targets.push(normalize_target(binary, reference.target));
                }
                DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
                    let norm_target = normalize_target(binary, reference.target);
                    let inst_addr = instruction.address;
                    let distance = if inst_addr > norm_target {
                        inst_addr - norm_target
                    } else {
                        norm_target - inst_addr
                    };
                    if distance > 512 || !same_section(binary, inst_addr, norm_target) {
                        jump_targets.push(norm_target);
                    }
                }
                DecodedFlowKind::None if instruction.mnemonic.eq_ignore_ascii_case("jmp") => {
                    let norm_target = normalize_target(binary, reference.target);
                    let inst_addr = instruction.address;
                    let distance = if inst_addr > norm_target {
                        inst_addr - norm_target
                    } else {
                        norm_target - inst_addr
                    };
                    if distance > 512 || !same_section(binary, inst_addr, norm_target) {
                        jump_targets.push(norm_target);
                    }
                }
                _ => {}
            },
            DecodedReferenceKind::MemoryAddress | DecodedReferenceKind::ImmediateAddress => {}
        }
    }
}

pub(crate) fn discovery_candidate_targets(
    profile: FunctionDiscoveryProfile,
    mut call_targets: Vec<u64>,
    jump_targets: &[u64],
) -> Vec<u64> {
    if profile == FunctionDiscoveryProfile::Aggressive {
        call_targets.extend_from_slice(jump_targets);
        call_targets.sort_unstable();
        call_targets.dedup();
    }
    call_targets
}
