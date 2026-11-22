use std::collections::{HashMap, HashSet};

use fission_loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use rayon::prelude::*;

use super::candidate_validation::validate_subroutine_candidate;
use super::instruction_boundaries::{InstructionCoverage, is_plausible_function_boundary};

/// Find jump destinations that cross a known function boundary and validate them as starts.
pub(super) fn scan_tail_call_targets(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    jump_edges: &[(u64, u64)],
    known_functions: &HashSet<u64>,
    coverage: &InstructionCoverage,
    global_references: &HashSet<u64>,
) -> Vec<u64> {
    let mut known_sorted: Vec<u64> = known_functions.iter().copied().collect();
    known_sorted.sort_unstable();

    let mut candidate_targets = Vec::new();
    for &(src, dest) in jump_edges {
        if known_functions.contains(&dest) {
            continue;
        }

        let mut function_before_src = None;
        let mut function_after_src = None;

        match known_sorted.binary_search(&src) {
            Ok(idx) => {
                function_before_src = Some(known_sorted[idx]);
                if idx + 1 < known_sorted.len() {
                    function_after_src = Some(known_sorted[idx + 1]);
                }
            }
            Err(idx) => {
                if idx > 0 {
                    function_before_src = Some(known_sorted[idx - 1]);
                }
                if idx < known_sorted.len() {
                    function_after_src = Some(known_sorted[idx]);
                }
            }
        }

        if src < dest {
            // Forward jump
            if let Some(after) = function_after_src {
                if dest >= after {
                    candidate_targets.push(dest);
                }
            }
        } else {
            // Backward jump
            if let Some(before) = function_before_src {
                if dest < before {
                    candidate_targets.push(dest);
                }
            }
        }
    }

    candidate_targets.sort_unstable();
    candidate_targets.dedup();

    // CFG validation is independent for each target and can walk thousands
    // of instructions, so validate the targets in parallel.
    let mut validated_targets: Vec<u64> = candidate_targets
        .into_par_iter()
        .filter(|&target| {
            !coverage.covers(target)
                && is_plausible_function_boundary(binary, Some(frontend), target)
        })
        .filter_map(|target| {
            let mut local_cache = HashMap::new();
            let (valid, _) = validate_subroutine_candidate(
                binary,
                frontend,
                target,
                1,
                4000,
                true,
                known_functions,
                &mut local_cache,
                Some(global_references),
            );
            valid.then_some(target)
        })
        .collect();
    validated_targets.sort_unstable();
    validated_targets
}
