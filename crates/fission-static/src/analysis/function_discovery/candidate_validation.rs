use std::collections::{HashMap, HashSet};

use fission_loader::LoadedBinary;
use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedReferenceKind, PackedContextOverride, RuntimeSleighFrontend,
};

use super::targets::{is_target_no_return, normalize_target};
use super::thumb::decode_context_for_address;

#[derive(Clone, Copy, Debug)]
pub(super) struct CandidateValidation {
    count: usize,
    did_terminate: bool,
    pub(super) has_call_to_valid: bool,
    adds_info: bool,
}

pub(super) fn validate_subroutine_candidate(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
    min_instructions: usize,
    max_instructions: usize,
    must_terminate: bool,
    known_functions: &HashSet<u64>,
    cache: &mut HashMap<u64, CandidateValidation>,
    global_references: Option<&HashSet<u64>>,
) -> (bool, CandidateValidation) {
    let decode_context = decode_context_for_address(binary, frontend, addr, None);
    validate_subroutine_candidate_in_context(
        binary,
        frontend,
        addr,
        min_instructions,
        max_instructions,
        must_terminate,
        known_functions,
        cache,
        global_references,
        decode_context,
    )
}

/// [`validate_subroutine_candidate`] in an explicit decode context.
pub(super) fn validate_subroutine_candidate_in_context(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
    min_instructions: usize,
    max_instructions: usize,
    must_terminate: bool,
    known_functions: &HashSet<u64>,
    cache: &mut HashMap<u64, CandidateValidation>,
    global_references: Option<&HashSet<u64>>,
    decode_context: Option<PackedContextOverride>,
) -> (bool, CandidateValidation) {
    let res = if let Some(&cached) = cache.get(&addr) {
        cached
    } else {
        // Safety check: candidate must be in executable memory
        let exec_ranges = super::ranges::executable_ranges(binary);
        if !super::ranges::is_in_executable_ranges(addr, &exec_ranges) {
            let val = CandidateValidation {
                count: 0,
                did_terminate: false,
                has_call_to_valid: false,
                adds_info: false,
            };
            cache.insert(addr, val);
            return (false, val);
        }

        let mut visited = HashSet::new();
        let mut worklist = vec![addr];
        let mut count = 0;
        let mut adds_info = false;
        let mut did_terminate = false;
        let mut has_call_to_valid = false;
        let mut invalid = false;

        while let Some(current) = worklist.pop() {
            if visited.contains(&current) {
                continue;
            }

            let mut ip = current;
            loop {
                if count >= max_instructions {
                    break;
                }

                if !visited.insert(ip) {
                    break;
                }

                if !super::ranges::is_in_executable_ranges(ip, &exec_ranges) {
                    invalid = true;
                    break;
                }

                let Some(bytes) = binary.view_bytes(ip, 15) else {
                    invalid = true;
                    break;
                };

                // `validate_subroutine_candidate` only ever looks at
                // mnemonic/length/flow_kind/direct_target below -- never
                // p-code -- so this uses the decode-only path instead of
                // `decode_window` (which always does a full p-code lift per
                // instruction and discards it). This loop is exactly the
                // hot path profiled as the Balanced discovery profile's
                // dominant cost: up to `max_instructions` decodes per
                // candidate, across every candidate from every scanner
                // (ghidra-patterns, tail-call recovery, dynamic prologues).
                let Ok(inst) = frontend.decode_single_no_pcode(bytes, ip, decode_context) else {
                    invalid = true;
                    break;
                };
                let inst = &inst;

                if inst.mnemonic.is_empty() || inst.mnemonic.to_lowercase() == "invalid" {
                    invalid = true;
                    break;
                }

                // [G5] Offcut Reference Check
                // Reject the candidate if any known global reference points to the *middle* of an instruction.
                if let Some(refs) = global_references {
                    for offset in 1..inst.length {
                        if refs.contains(&(ip + offset as u64)) {
                            invalid = true;
                            break;
                        }
                    }
                }
                if invalid {
                    break;
                }

                count += 1;

                // Check if it calls/jmps to a known function
                if let Some(target) = inst.direct_target {
                    let norm_target = normalize_target(binary, target);
                    if known_functions.contains(&norm_target) {
                        adds_info = true;
                        if inst.flow_kind == DecodedFlowKind::Call {
                            has_call_to_valid = true;
                        }
                    }
                }

                if inst.flow_kind == DecodedFlowKind::Call {
                    let call_target = inst.direct_target.or_else(|| {
                        inst.references.iter().find_map(|r| match r.kind {
                            DecodedReferenceKind::CallTarget
                            | DecodedReferenceKind::RipRelativeAddress => Some(r.target),
                            _ => None,
                        })
                    });
                    if let Some(target) = call_target {
                        let norm_target = normalize_target(binary, target);
                        if is_target_no_return(binary, norm_target) {
                            did_terminate = true;
                            break;
                        }
                    }
                }

                match inst.flow_kind {
                    DecodedFlowKind::Return => {
                        did_terminate = true;
                        break;
                    }
                    DecodedFlowKind::Jump => {
                        if let Some(target) = inst.direct_target {
                            let norm_target = normalize_target(binary, target);
                            // If it's a tail call to a known function, treat as termination
                            if known_functions.contains(&norm_target) {
                                did_terminate = true;
                                adds_info = true;
                                break;
                            }
                            if super::ranges::is_in_executable_ranges(norm_target, &exec_ranges) {
                                worklist.push(norm_target);
                            } else {
                                invalid = true;
                                break;
                            }
                        } else {
                            // STRICT MODE: do not consider indirect jumps as valid termination for candidate testing
                            // Because garbage often decodes to `ff 20` etc.
                        }
                        break;
                    }
                    DecodedFlowKind::ConditionalJump => {
                        if let Some(target) = inst.direct_target {
                            let norm_target = normalize_target(binary, target);
                            if super::ranges::is_in_executable_ranges(norm_target, &exec_ranges) {
                                worklist.push(norm_target);
                            }
                        }
                        ip += inst.length as u64;
                    }
                    DecodedFlowKind::Interrupt | DecodedFlowKind::Syscall => {
                        did_terminate = true;
                        break;
                    }
                    _ => {
                        ip += inst.length as u64;
                    }
                }
            }
            if invalid {
                break;
            }
        }

        let val = if invalid {
            CandidateValidation {
                count: 0,
                did_terminate: false,
                has_call_to_valid: false,
                adds_info: false,
            }
        } else {
            CandidateValidation {
                count,
                did_terminate,
                has_call_to_valid,
                adds_info,
            }
        };
        cache.insert(addr, val);
        val
    };

    let is_valid = (!must_terminate || res.did_terminate || res.has_call_to_valid)
        && res.count >= min_instructions;
    (is_valid, res)
}

/// Check that a minimum number of consecutive instructions decodes in the supplied context.
pub(super) fn validate_contiguous_instructions_in_context(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
    min_instructions: usize,
    executable_ranges: &[(u64, u64)],
    decode_context: Option<PackedContextOverride>,
) -> bool {
    let mut ip = addr;
    for _ in 0..min_instructions {
        if !super::ranges::is_in_executable_ranges(ip, executable_ranges) {
            return false;
        }
        let Some(bytes) = binary.view_bytes(ip, 15) else {
            return false;
        };
        let Ok(inst) = frontend.decode_single_no_pcode(bytes, ip, decode_context) else {
            return false;
        };
        if inst.length == 0
            || inst.mnemonic.is_empty()
            || inst.mnemonic.eq_ignore_ascii_case("invalid")
        {
            return false;
        }
        ip = ip.saturating_add(inst.length as u64);
    }
    true
}
