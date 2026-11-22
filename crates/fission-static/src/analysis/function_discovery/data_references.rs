use std::collections::{HashMap, HashSet};

use fission_loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

use super::candidate_validation::validate_subroutine_candidate_in_context;
use super::instruction_boundaries::is_plausible_function_boundary;
use super::ranges::is_in_executable_ranges;
use super::thumb::decode_context_for_address;
use super::types::FunctionStartCandidate;

pub(super) fn scan_data_code_pointers(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    executable_ranges: &[(u64, u64)],
    known_functions: &HashSet<u64>,
    global_references: Option<&HashSet<u64>>,
) -> Vec<FunctionStartCandidate> {
    use rayon::prelude::*;

    let ptr_size = if binary.is_64bit { 8 } else { 4 };
    let little_endian = binary.is_little_endian();

    let mut candidates: Vec<(u64, bool)> = binary
        .sections
        .par_iter()
        .filter(|sec| !sec.is_executable && sec.is_readable)
        .flat_map(|section| {
            let mut sec_candidates = Vec::new();
            let Ok(sec_size) = usize::try_from(section.virtual_size) else {
                return sec_candidates;
            };
            if let Some(bytes) = binary.view_bytes(section.virtual_address, sec_size) {
                let mut offset = 0;
                while offset + ptr_size <= bytes.len() {
                    let val = if ptr_size == 8 {
                        let raw: [u8; 8] = bytes[offset..offset + 8].try_into().unwrap();
                        if little_endian {
                            u64::from_le_bytes(raw)
                        } else {
                            u64::from_be_bytes(raw)
                        }
                    } else {
                        let raw: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap();
                        if little_endian {
                            u32::from_le_bytes(raw) as u64
                        } else {
                            u32::from_be_bytes(raw) as u64
                        }
                    };
                    // A code pointer on an architecture with a low-bit code mode
                    // (ARM/Thumb) carries the mode in bit 0: strip it to get the
                    // address, and remember it was set.
                    let pointer = frontend.normalize_low_bit_code_address(val);
                    let target = pointer.address;
                    if is_in_executable_ranges(target, executable_ranges) {
                        if is_plausible_function_boundary(binary, Some(frontend), target) {
                            sec_candidates.push((target, pointer.context_override.is_some()));
                        }
                    }
                    offset += ptr_size;
                }
            }
            sec_candidates
        })
        .collect();

    candidates.sort_unstable();
    candidates.dedup();

    let alternate_mode = frontend.low_bit_code_mode_override();
    let mut refs: Vec<FunctionStartCandidate> = candidates
        .into_par_iter()
        .filter_map(|(val, low_bit)| {
            let decode_context = if low_bit {
                alternate_mode
            } else {
                decode_context_for_address(binary, frontend, val, None)
            };
            let mut local_cache = HashMap::new();
            let (is_valid, res) = validate_subroutine_candidate_in_context(
                binary,
                frontend,
                val,
                1,
                4000,
                true,
                known_functions,
                &mut local_cache,
                global_references,
                decode_context,
            );
            if is_valid && res.has_call_to_valid {
                Some(FunctionStartCandidate {
                    address: val,
                    low_bit_mode: low_bit,
                })
            } else {
                None
            }
        })
        .collect();

    // The same function can be pointed at with and without the mode bit; the
    // bit is the ABI's own statement of the mode, so it wins.
    refs.sort_unstable_by_key(|start| (start.address, !start.low_bit_mode));
    refs.dedup_by_key(|start| start.address);
    refs
}
