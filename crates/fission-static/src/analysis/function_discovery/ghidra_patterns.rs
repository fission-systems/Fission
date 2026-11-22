use std::collections::{HashMap, HashSet};

use fission_loader::LoadedBinary;
use fission_signatures::load_ghidra_patterns;
use fission_sleigh::runtime::RuntimeSleighFrontend;

use super::candidate_validation::{
    validate_contiguous_instructions_in_context, validate_subroutine_candidate_in_context,
};
use super::discovery_diag_enabled;
use super::instruction_boundaries::{InstructionCoverage, is_plausible_function_boundary};
use super::ranges::is_in_executable_ranges;
use super::thumb::decode_context_for_address;
use super::types::FunctionStartCandidate;

/// Resolve the `arch_tag` `load_ghidra_patterns` expects from the binary's
/// actual detected architecture, instead of assuming x86.
///
/// `binary.architecture.processor` carries the exact processor name Ghidra
/// itself uses (read straight from the `.opinion` file's `processor="..."`
/// attribute during auto-detection -- see `fission_core::architecture`), so
/// this only needs to fold in endianness for the handful of processors
/// whose own `patternconstraints.xml` actually splits by it (confirmed
/// against each processor's real constraint file under
/// `vendor/ghidra/.../Processors/<name>/data/patterns/`, not guessed).
/// Returns `None` for any processor `load_ghidra_patterns` doesn't have
/// pattern data for -- callers already handle that as "no patterns found".
fn resolve_pattern_arch_tag(binary: &LoadedBinary) -> Option<String> {
    let arch = binary.architecture.as_ref()?;
    let be = arch.endian == "big";
    Some(match arch.processor.as_str() {
        "x86" => {
            if binary.is_64bit {
                "x86-64win".to_string()
            } else {
                "x86win".to_string()
            }
        }
        "ARM" => (if be { "ARM_BE" } else { "ARM_LE" }).to_string(),
        "AARCH64" => "AARCH64".to_string(),
        "MIPS" => (if be { "MIPS_BE" } else { "MIPS_LE" }).to_string(),
        "PowerPC" => (if be { "PowerPC_BE" } else { "PowerPC_LE" }).to_string(),
        "Sparc" => "Sparc".to_string(),
        "SuperH4" => "SuperH4".to_string(),
        "V850" => "V850".to_string(),
        "68000" => "68000".to_string(),
        "Atmel" => "avr8".to_string(),
        "Loongarch" => "Loongarch".to_string(),
        "NDS32" => "NDS32".to_string(),
        "PA-RISC" => "PA-RISC".to_string(),
        "RISCV" => "RISCV".to_string(),
        "tricore" => "tricore".to_string(),
        "Xtensa" => "Xtensa".to_string(),
        _ => return None,
    })
}

/// Scan binary executable sections using Ghidra XML static patterns.
///
/// **Two-phase approach (mirrors Ghidra's pipeline):**
/// 1. Raw byte scan with first-byte index — O(section_size), no SLEIGH
/// 2. SLEIGH validation on unique raw hits — O(hits), once per address
pub(super) fn scan_ghidra_patterns(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    executable_ranges: &[(u64, u64)],
    known_functions: &HashSet<u64>,
    coverage: &InstructionCoverage,
    global_references: Option<&HashSet<u64>>,
) -> Vec<FunctionStartCandidate> {
    let Some(arch_tag) = resolve_pattern_arch_tag(binary) else {
        return Vec::new();
    };
    let compiler_id = binary.get_ghidra_compiler_id();
    let patterns = load_ghidra_patterns(&arch_tag, compiler_id.as_deref());
    if patterns.is_empty() {
        return Vec::new();
    }

    // Pre-index by first fixed byte of post_bytes + whether each pattern has pre_bytes
    let mut by_first_byte: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut wildcard_start: Vec<usize> = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        if pat.post_bytes.is_empty() {
            continue;
        }
        match pat.post_bytes[0].fixed() {
            Some(b) => by_first_byte.entry(b).or_default().push(i),
            None => wildcard_start.push(i),
        }
    }

    // Phase 1: raw byte scan — collect (addr, (Option<usize>, bool)) where value is (min valid_code_min, requires_contiguous)
    let mut raw_hits: HashMap<(u64, bool), (Option<usize>, bool)> = HashMap::new();

    // Build AhoCorasick automaton for the first contiguous block of fixed bytes
    let mut ac_patterns = Vec::new();
    let mut pattern_map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut current_ac_index = 0;

    for (i, pat) in patterns.iter().enumerate() {
        let mut prefix = Vec::new();
        for b in &pat.post_bytes {
            if let Some(byte) = b.fixed() {
                prefix.push(byte);
            } else {
                break;
            }
        }

        if prefix.is_empty() {
            // Pattern starts with a wildcard (rare, but possible)
            wildcard_start.push(i);
        } else {
            // Find if this prefix already exists
            if let Some(pos) = ac_patterns.iter().position(|p| p == &prefix) {
                pattern_map.entry(pos).or_default().push(i);
            } else {
                ac_patterns.push(prefix);
                pattern_map.insert(current_ac_index, vec![i]);
                current_ac_index += 1;
            }
        }
    }

    // `find_overlapping_iter` below only supports `MatchKind::Standard`
    // (aho-corasick panics on any overlapping search otherwise -- confirmed
    // via a real binary crashing 100% of the time under `--function-
    // discovery-profile balanced`/`aggressive`, since `LeftmostFirst` was
    // used here previously). `Standard` is also the right *semantic* fit:
    // every raw hit below is re-verified in full against `patterns[i]`
    // afterward, so this automaton only needs to be a fast, complete
    // "does any registered prefix start here" pre-filter -- it should
    // report every prefix occurrence (including overlapping ones), not
    // silently drop some in favor of a single "best" match per position
    // the way `LeftmostFirst` would if it supported overlapping search.
    let ac = aho_corasick::AhoCorasickBuilder::new()
        .match_kind(aho_corasick::MatchKind::Standard)
        .build(&ac_patterns)
        .unwrap();

    use rayon::prelude::*;

    for section in &binary.sections {
        if !section.is_executable {
            continue;
        }
        let Some(bytes) = binary.view_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            continue;
        };
        let base = section.virtual_address;

        let section_hits: Vec<(u64, Option<usize>, bool, bool)> = ac
            .find_overlapping_iter(bytes)
            .map(|mat| {
                let offset = mat.start();
                let ac_idx = mat.pattern().as_usize();
                let addr = base + offset as u64;
                (offset, ac_idx, addr)
            })
            .collect::<Vec<_>>()
            .into_par_iter()
            .filter_map(|(_offset, ac_idx, addr)| {
                if !is_in_executable_ranges(addr, executable_ranges) {
                    return None;
                }

                if let Some(indices) = pattern_map.get(&ac_idx) {
                    for &i in indices.iter() {
                        if patterns[i].matches(bytes, base, addr) {
                            let pat = &patterns[i];
                            let boundary_ok = if pat.pre_bytes.is_empty() {
                                if let Some(cond) = &pat.after_cond {
                                    matches_pattern_after_condition(
                                        binary,
                                        addr,
                                        cond,
                                        executable_ranges,
                                        global_references,
                                    )
                                } else {
                                    is_plausible_function_boundary(binary, Some(frontend), addr)
                                }
                            } else {
                                is_plausible_function_boundary(binary, Some(frontend), addr)
                            };

                            if boundary_ok {
                                return Some((
                                    addr,
                                    pat.valid_code_min,
                                    pat.contiguous,
                                    pattern_selects_low_bit_mode(pat),
                                ));
                            }
                        }
                    }
                }
                None
            })
            .collect();

        // Sequential fallback for patterns that start with wildcard
        let wildcard_hits: Vec<(u64, Option<usize>, bool, bool)> = (0..bytes.len())
            .into_par_iter()
            .filter_map(|offset| {
                if wildcard_start.is_empty() {
                    return None;
                }
                let addr = base + offset as u64;
                if !is_in_executable_ranges(addr, executable_ranges) {
                    return None;
                }

                for &i in &wildcard_start {
                    if patterns[i].matches(bytes, base, addr) {
                        let pat = &patterns[i];
                        let boundary_ok = if pat.pre_bytes.is_empty() {
                            if let Some(cond) = &pat.after_cond {
                                matches_pattern_after_condition(
                                    binary,
                                    addr,
                                    cond,
                                    executable_ranges,
                                    global_references,
                                )
                            } else {
                                is_plausible_function_boundary(binary, Some(frontend), addr)
                            }
                        } else {
                            is_plausible_function_boundary(binary, Some(frontend), addr)
                        };

                        if boundary_ok {
                            return Some((
                                addr,
                                pat.valid_code_min,
                                pat.contiguous,
                                pattern_selects_low_bit_mode(pat),
                            ));
                        }
                    }
                }
                None
            })
            .collect();

        for (addr, min_val, contiguous, low_bit) in section_hits.into_iter().chain(wildcard_hits) {
            let entry = raw_hits
                .entry((addr, low_bit))
                .or_insert((None, contiguous));
            if let Some(min) = min_val {
                entry.0 = Some(entry.0.map_or(min, |v| v.min(min)));
            }
            entry.1 &= contiguous;
        }
    }

    // Coverage is unchanged until the validated batch is registered.
    // Reject covered instruction starts and offcuts before walking their CFGs.
    let raw_hit_count = raw_hits.len();
    raw_hits.retain(|(addr, _), _| !coverage.covers(*addr));
    if discovery_diag_enabled() {
        eprintln!("[ghidra-patterns] {arch_tag}: raw hits = {}", raw_hit_count);
        eprintln!(
            "DISCOVERY_DIAG: xml_covered_hits={}",
            raw_hit_count - raw_hits.len()
        );
    }

    // Phase 2: SLEIGH validation -- once per unique (address, mode)
    let alternate_mode = frontend.low_bit_code_mode_override();
    let mut hits: Vec<FunctionStartCandidate> = raw_hits
        .into_par_iter()
        .filter_map(|((addr, low_bit), (valid_code_min, contiguous))| {
            // A pattern that sets the alternate mode (Thumb `TMode`) is a
            // function start only in that mode, so validate it there.
            let decode_context = if low_bit {
                alternate_mode.clone()?.into()
            } else {
                decode_context_for_address(binary, frontend, addr, None)
            };
            let min_inst = valid_code_min.unwrap_or(3);
            if contiguous
                && !validate_contiguous_instructions_in_context(
                    binary,
                    frontend,
                    addr,
                    min_inst,
                    executable_ranges,
                    decode_context.clone(),
                )
            {
                return None;
            }
            let mut local_cache = HashMap::new();
            if validate_subroutine_candidate_in_context(
                binary,
                frontend,
                addr,
                min_inst,
                4000,
                true,
                known_functions,
                &mut local_cache,
                global_references,
                decode_context,
            )
            .0
            {
                Some(FunctionStartCandidate {
                    address: addr,
                    low_bit_mode: low_bit,
                })
            } else {
                None
            }
        })
        .collect();

    if discovery_diag_enabled() {
        eprintln!(
            "[ghidra-patterns] after SLEIGH validation: {} hits",
            hits.len()
        );
    }

    // An address that validates in both modes is reported once, in the default
    // one: nothing proved it is not ordinary code.
    hits.sort_unstable_by_key(|hit| (hit.address, hit.low_bit_mode));
    hits.dedup_by_key(|hit| hit.address);
    hits
}

/// Whether `pattern` decodes its match in the alternate code mode: it sets a
/// low-bit context field (ARM `TMode`, MIPS `ISA_MODE`) to something non-zero.
fn pattern_selects_low_bit_mode(pattern: &fission_signatures::GhidraFuncPattern) -> bool {
    pattern.context.iter().any(|setting| {
        setting.value != 0 && fission_sleigh::runtime::is_low_bit_code_context_field(&setting.name)
    })
}

/// Strict boundary gate for XML patterns that have no pre_bytes context.
/// Requires the bytes immediately before `addr` to be padding (CC/90)
/// or a terminator (RET variants, JMP near/short, CALL).
///
/// CALL-terminated boundaries: On x86, code can flow directly after a
/// CALL sequence (e.g. `CALL rel32; <new function>`) when the compiler
/// places adjacent subroutines with no padding. The CALL itself is a
/// valid function-end because the next sequential address is either
/// unreachable (tail-call) or a new function entry point.

pub(super) fn matches_pattern_after_condition(
    binary: &LoadedBinary,
    addr: u64,
    cond: &str,
    executable_ranges: &[(u64, u64)],
    global_references: Option<&HashSet<u64>>,
) -> bool {
    if addr == 0 {
        return false;
    }
    let prev_addr = addr - 1;

    match cond {
        "defined" => {
            let at_section_start = executable_ranges.iter().any(|&(start, _)| addr == start);
            at_section_start || is_in_executable_ranges(prev_addr, executable_ranges)
        }
        "data" => {
            if !is_in_executable_ranges(prev_addr, executable_ranges) {
                return true;
            }
            if let Some(sec) = binary.sections.iter().find(|s| {
                prev_addr >= s.virtual_address
                    && prev_addr < s.virtual_address.saturating_add(s.virtual_size)
            }) {
                if !sec.is_executable {
                    return true;
                }
            }
            if let Some(bytes) = binary.view_bytes(prev_addr, 1) {
                if matches!(bytes[0], 0x00 | 0x90 | 0xcc) {
                    return true;
                }
            }
            false
        }
        "ptr" => {
            global_references.is_some_and(|refs| refs.contains(&addr))
                || binary.relocations.iter().any(|reloc| {
                    let target = reloc.addend as u64;
                    target != 0 && super::targets::normalize_target(binary, target) == addr
                })
        }
        "function" => is_plausible_function_boundary(binary, None, addr),
        _ => true,
    }
}
