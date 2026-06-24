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
    let is_sorted = binary
        .sections
        .windows(2)
        .all(|w| w[0].virtual_address <= w[1].virtual_address);
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
///
/// `jump_edges` receives (src_addr, dst_addr) pairs for unconditional far JMPs.
/// This enables G2 (SharedReturnAnalysis) which requires both src and dst to
/// determine whether a jump crosses a function boundary.
pub(crate) fn collect_instruction_targets(
    binary: &LoadedBinary,
    instruction: &DecodedInstruction,
    call_targets: &mut Vec<u64>,
    jump_targets: &mut Vec<u64>,
    jump_edges: &mut Vec<(u64, u64)>, // (src, dst) pairs for unconditional JMPs
) {
    let inst_addr = instruction.address;

    match instruction.flow_kind {
        DecodedFlowKind::Call => {
            if let Some(target) = instruction.direct_target {
                call_targets.push(normalize_target(binary, target));
            }
        }
        DecodedFlowKind::Jump => {
            if let Some(target) = instruction.direct_target {
                let norm_target = normalize_target(binary, target);
                let distance = if inst_addr > norm_target {
                    inst_addr - norm_target
                } else {
                    norm_target - inst_addr
                };
                let is_long_or_cross =
                    distance > 512 || !same_section(binary, inst_addr, norm_target);
                if inst_addr != norm_target {
                    jump_edges.push((inst_addr, norm_target));
                }
                if is_long_or_cross {
                    jump_targets.push(norm_target);
                }
            }
        }
        DecodedFlowKind::ConditionalJump => {}
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
                if instruction.flow_kind == DecodedFlowKind::Jump {
                    let distance = if inst_addr > norm_target {
                        inst_addr - norm_target
                    } else {
                        norm_target - inst_addr
                    };
                    let is_long_or_cross =
                        distance > 512 || !same_section(binary, inst_addr, norm_target);
                    if inst_addr != norm_target {
                        jump_edges.push((inst_addr, norm_target));
                    }
                    if is_long_or_cross {
                        jump_targets.push(norm_target);
                    }
                }
            }
            DecodedReferenceKind::RipRelativeAddress => match instruction.flow_kind {
                DecodedFlowKind::Call => {
                    call_targets.push(normalize_target(binary, reference.target));
                }
                DecodedFlowKind::Jump => {
                    let norm_target = normalize_target(binary, reference.target);
                    let distance = if inst_addr > norm_target {
                        inst_addr - norm_target
                    } else {
                        norm_target - inst_addr
                    };
                    let is_long_or_cross =
                        distance > 512 || !same_section(binary, inst_addr, norm_target);
                    if inst_addr != norm_target {
                        jump_edges.push((inst_addr, norm_target));
                    }
                    if is_long_or_cross {
                        jump_targets.push(norm_target);
                    }
                }
                DecodedFlowKind::ConditionalJump => {}
                DecodedFlowKind::None if instruction.mnemonic.eq_ignore_ascii_case("jmp") => {
                    let norm_target = normalize_target(binary, reference.target);
                    let distance = if inst_addr > norm_target {
                        inst_addr - norm_target
                    } else {
                        norm_target - inst_addr
                    };
                    let is_long_or_cross =
                        distance > 512 || !same_section(binary, inst_addr, norm_target);
                    if inst_addr != norm_target {
                        jump_edges.push((inst_addr, norm_target));
                    }
                    if is_long_or_cross {
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
    _profile: FunctionDiscoveryProfile,
    mut call_targets: Vec<u64>,
    _jump_targets: &[u64],
) -> Vec<u64> {
    // Jump targets are no longer blindly merged into call targets.
    // They will be validated dynamically in discover.rs under the Aggressive profile.
    call_targets.sort_unstable();
    call_targets.dedup();
    call_targets
}
