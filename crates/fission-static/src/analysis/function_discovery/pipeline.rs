use std::collections::{HashMap, HashSet};

use fission_loader::{FunctionCandidateInfo, FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::{RuntimeFrontendStatus, RuntimeSleighFrontend};
use rayon::prelude::*;

use super::candidate_validation::validate_subroutine_candidate;
use super::control_flow_scan::{ControlFlowReferences, scan_executable_sections};
use super::data_references::scan_data_code_pointers;
use super::discovery_diag_enabled;
use super::ghidra_patterns::scan_ghidra_patterns;
use super::instruction_boundaries::InstructionCoverage;
use super::ranges::{executable_ranges, is_in_executable_ranges, runtime_load_spec_for};
use super::tail_calls::scan_tail_call_targets;
use super::targets::{discovery_candidate_targets, terminal_import_thunk_target};
use super::types::FunctionDiscoveryReport;

pub fn discover_functions_with_runtime(binary: &mut LoadedBinary) -> FunctionDiscoveryReport {
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
    let ControlFlowReferences {
        decoded_instruction_count: total_decoded,
        call_targets,
        jump_targets,
        jump_edges,
    } = scan_executable_sections(binary, &frontend);

    report.decoded_instruction_count = total_decoded;
    report.call_target_count = call_targets.len();
    report.jump_target_count = jump_targets.len();

    let known_function_addresses: HashSet<u64> =
        binary.functions.iter().map(|f| f.address).collect();
    let mut coverage_seeds = known_function_addresses.clone();
    coverage_seeds.extend(call_targets.iter().copied());

    let mut all_references = HashSet::new();
    all_references.extend(call_targets.iter().copied());
    all_references.extend(jump_targets.iter().copied());

    let mut candidates = discovery_candidate_targets(call_targets, &jump_targets);
    let call_candidate_count = candidates.len();
    // These addresses already seed the downstream known set and will be
    // rejected at insertion. Keep their references and coverage seeds, but
    // avoid decoding their bodies again solely to validate a new start.
    candidates.retain(|address| {
        !known_function_addresses.contains(address)
            || !binary.function_addr_index.contains_key(address)
    });
    if discovery_diag_enabled() {
        eprintln!(
            "DISCOVERY_DIAG: candidates={} already_known={} validate_stage_start",
            candidates.len(),
            call_candidate_count - candidates.len(),
        );
    }
    let validate_start = std::time::Instant::now();

    let valid_call_targets: Vec<u64> = candidates
        .par_iter()
        .copied()
        .filter_map(|addr| {
            let bytes = binary.view_bytes(addr, 1)?;
            if bytes[0] == 0xcc || bytes[0] == 0x90 {
                return None;
            }
            let mut local_cache = HashMap::new();
            let empty_known = HashSet::new();
            let (valid, _) = validate_subroutine_candidate(
                binary,
                &frontend,
                addr,
                3,
                4000,
                true,
                &empty_known,
                &mut local_cache,
                Some(&all_references),
            );
            if valid { Some(addr) } else { None }
        })
        .collect();
    if discovery_diag_enabled() {
        eprintln!(
            "DISCOVERY_DIAG: validate_stage={:?}",
            validate_start.elapsed()
        );
    }
    if discovery_diag_enabled() {
        eprintln!(
            "SCANNER_STATS: call_targets validated = {} / {}",
            valid_call_targets.len(),
            candidates.len()
        );
    }
    let mut candidates = valid_call_targets;
    let tail_start = std::time::Instant::now();

    let valid_symbol_seeds: Vec<FunctionCandidateInfo> = binary
        .function_candidates
        .par_iter()
        .filter(|seed| !known_function_addresses.contains(&seed.address))
        .filter_map(|seed| {
            let mut local_cache = HashMap::new();
            let (valid_routine, _) = validate_subroutine_candidate(
                binary,
                &frontend,
                seed.address,
                1,
                4000,
                true,
                &known_function_addresses,
                &mut local_cache,
                Some(&all_references),
            );
            (valid_routine
                || terminal_import_thunk_target(binary, &frontend, seed.address).is_some())
            .then(|| seed.clone())
        })
        .collect();
    candidates.extend(valid_symbol_seeds.iter().map(|seed| seed.address));
    // Starts that only a reference or signature scan names. See
    // `FunctionInfo::is_scan_inferred`.
    let mut scan_inferred: HashSet<u64> = HashSet::new();

    let mut coverage = InstructionCoverage::build(binary, &frontend, &coverage_seeds);

    let mut all_known = known_function_addresses;
    all_known.extend(candidates.iter().copied());

    let data_starts = scan_data_code_pointers(
        binary,
        &frontend,
        &executable_ranges,
        &all_known,
        Some(&all_references),
    );
    all_references.extend(data_starts.iter().map(|start| start.address));
    let mut data_refs: Vec<u64> = Vec::with_capacity(data_starts.len());
    for start in data_starts {
        if coverage.covers(start.address) {
            continue;
        }
        if start.low_bit_mode {
            binary.mark_low_bit_mode_start(start.address);
        }
        data_refs.push(start.address);
    }
    if discovery_diag_enabled() {
        eprintln!("SCANNER_STATS: data_refs={}", data_refs.len());
    }
    coverage.add_function_starts(binary, &frontend, &data_refs);
    candidates.extend(data_refs.clone());
    scan_inferred.extend(data_refs.iter().copied());
    all_known.extend(data_refs.clone());

    // MSVC C++ exception-handling FuncInfo tables (x86 only -- see
    // msvc_eh's module doc). Unlike every other scanner here, catch
    // handlers and unwind actions are reachable *only* from these
    // tables, never from code -- structurally invisible otherwise.
    //
    // Deliberately skips `validate_subroutine_candidate`'s CFG-shape
    // heuristic (does this decode to something that looks like a
    // normal function start/body?) and `is_plausible_function_boundary` (is there
    // padding or a terminator right before this address?): a real
    // catch/unwind handler routinely starts mid-instruction-stream
    // right after another call ends, or opens with a `call` of its
    // own rather than a typical prologue -- confirmed on a real
    // catch handler this scanner found (`sqlite3.dll`'s
    // `Catch_All@0x101c36c5` per Ghidra's own naming) that both of
    // those checks reject outright. The FuncInfo/UnwindMap/
    // TryBlockMap chain's own bounds/same-section checks (already
    // applied in `msvc_eh::scan_msvc_eh_funcinfo`) are the actual
    // validation here, the same way Ghidra's own `PEExceptionAnalyzer`
    // trusts a validated `EHFunctionInfoModel` without any additional
    // disassembly-based check.
    let raw_eh_candidates = super::msvc_eh::scan_msvc_eh_funcinfo(binary);
    let mut eh_funcinfo_hits: Vec<u64> = raw_eh_candidates
        .into_iter()
        .filter(|&addr| {
            !coverage.covers(addr)
                && !all_known.contains(&addr)
                && is_in_executable_ranges(addr, &executable_ranges)
        })
        .collect();
    eh_funcinfo_hits.sort_unstable();
    if discovery_diag_enabled() {
        eprintln!("SCANNER_STATS: msvc_eh_funcinfo={}", eh_funcinfo_hits.len());
    }
    coverage.add_function_starts(binary, &frontend, &eh_funcinfo_hits);
    candidates.extend(eh_funcinfo_hits.clone());
    all_known.extend(eh_funcinfo_hits);

    // SafeSEH handler table + Control Flow Guard check-function
    // pointer, from IMAGE_LOAD_CONFIG_DIRECTORY (x86 only -- see
    // load_config's module doc). Same "PE-metadata-only reachability"
    // rationale and same trust level as msvc_eh_funcinfo above: every
    // SafeSEH entry is, by construction, a real handler the OS loader
    // itself validates before ever running it.
    let raw_load_config_candidates = super::load_config::scan_load_config_candidates(binary);
    let mut load_config_hits: Vec<u64> = raw_load_config_candidates
        .into_iter()
        .filter(|&addr| {
            !coverage.covers(addr)
                && !all_known.contains(&addr)
                && is_in_executable_ranges(addr, &executable_ranges)
        })
        .collect();
    load_config_hits.sort_unstable();
    if discovery_diag_enabled() {
        eprintln!("SCANNER_STATS: load_config={}", load_config_hits.len());
    }
    coverage.add_function_starts(binary, &frontend, &load_config_hits);
    candidates.extend(load_config_hits.clone());
    all_known.extend(load_config_hits);

    // Ghidra XML static patterns (x86-64win / x86win)
    let pattern_starts = scan_ghidra_patterns(
        binary,
        &frontend,
        &executable_ranges,
        &all_known,
        &coverage,
        Some(&all_references),
    );
    // The pattern scan has already excluded starts covered by decoded instructions.
    // Record any alternate mode (Thumb) before registering the new starts.
    let mut xml_hits: Vec<u64> = Vec::with_capacity(pattern_starts.len());
    for start in pattern_starts {
        if start.low_bit_mode {
            binary.mark_low_bit_mode_start(start.address);
        }
        xml_hits.push(start.address);
    }
    if discovery_diag_enabled() {
        eprintln!("SCANNER_STATS: xml_hits={}", xml_hits.len());
    }
    coverage.add_function_starts(binary, &frontend, &xml_hits);
    candidates.extend(xml_hits.clone());
    scan_inferred.extend(xml_hits.iter().copied());
    all_known.extend(xml_hits.clone());

    if !jump_edges.is_empty() {
        let tail_call_targets = scan_tail_call_targets(
            binary,
            &frontend,
            &jump_edges,
            &all_known,
            &coverage,
            &all_references,
        );
        if discovery_diag_enabled() {
            eprintln!("SCANNER_STATS: tail_calls={}", tail_call_targets.len());
        }
        coverage.add_function_starts(binary, &frontend, &tail_call_targets);
        candidates.extend(tail_call_targets.iter().copied());
        scan_inferred.extend(tail_call_targets.iter().copied());
        all_known.extend(tail_call_targets);
    }

    candidates.sort_unstable();
    candidates.dedup();

    let valid_symbol_seed_by_address: HashMap<u64, FunctionCandidateInfo> = valid_symbol_seeds
        .into_iter()
        .map(|seed| (seed.address, seed))
        .collect();

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
            let seed = valid_symbol_seed_by_address.get(&address);
            let thunk_target = terminal_import_thunk_target(binary, &frontend, address);
            let import_identity = thunk_target
                .and_then(|target| binary.iat_symbols.get(&target))
                .map(|name| split_import_identity(name));
            let is_thunk_like = thunk_target.is_some();
            binary.functions.push(FunctionInfo {
                name: import_identity
                    .as_ref()
                    .map(|(_, name)| name.clone())
                    .or_else(|| seed.map(|seed| seed.name.clone()))
                    .unwrap_or_else(|| format!("sub_{address:x}")),
                address,
                size: 0,
                is_export: false,
                is_import: false,
                origin: if is_thunk_like {
                    Some("sleigh-import-thunk".to_string())
                } else if let Some(seed) = seed {
                    Some(seed.origin.clone())
                } else if scan_inferred.contains(&address) {
                    Some(FunctionInfo::SCAN_INFERRED_ORIGIN.to_string())
                } else {
                    None
                },
                kind: if is_thunk_like {
                    Some("import_thunk".to_string())
                } else {
                    seed.map(|_| "validated_symbol".to_string())
                },
                source_section: seed.and_then(|seed| seed.source_section.clone()),
                external_library: import_identity.and_then(|(library, _)| library),
                is_thunk_like,
                thunk_target,
            });
        }
        binary.functions.sort_by_key(|function| function.address);
        binary.functions_sorted = true;
        binary.rebuild_function_indices();
    }

    if discovery_diag_enabled() {
        eprintln!("DISCOVERY_DIAG: tail_stage={:?}", tail_start.elapsed());
    }
    report
}

fn split_import_identity(name: &str) -> (Option<String>, String) {
    match name.split_once('!') {
        Some((library, symbol)) if !symbol.is_empty() => {
            (Some(library.to_string()), symbol.to_string())
        }
        _ => (None, name.to_string()),
    }
}
