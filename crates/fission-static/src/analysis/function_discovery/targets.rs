use std::collections::BTreeSet;

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
                let norm_target = normalize_target(binary, target);
                let inst_addr = instruction.address;
                let distance = if inst_addr > norm_target {
                    inst_addr - norm_target
                } else {
                    norm_target - inst_addr
                };
                if distance > 512 || !same_section(binary, inst_addr, norm_target) {
                    jump_targets.insert(norm_target);
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
                call_targets.insert(normalize_target(binary, reference.target));
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
                    jump_targets.insert(norm_target);
                }
            }
            DecodedReferenceKind::RipRelativeAddress => match instruction.flow_kind {
                DecodedFlowKind::Call => {
                    call_targets.insert(normalize_target(binary, reference.target));
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
                        jump_targets.insert(norm_target);
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
                        jump_targets.insert(norm_target);
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
    mut call_targets: BTreeSet<u64>,
    jump_targets: &BTreeSet<u64>,
) -> BTreeSet<u64> {
    if profile == FunctionDiscoveryProfile::Aggressive {
        call_targets.extend(jump_targets.iter().copied());
    }
    call_targets
}
