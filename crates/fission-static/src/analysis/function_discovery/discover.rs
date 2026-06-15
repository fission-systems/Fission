use fission_sleigh::runtime::DecodedFlowKind;
use fission_loader::{FunctionInfo, LoadedBinary};
use fission_sleigh::runtime::{RuntimeFrontendStatus, RuntimeSleighFrontend};
use fission_signatures::load_ghidra_patterns;

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

    struct ScanChunk<'a> {
        bytes: &'a [u8],
        virtual_address: u64,
    }

    let mut chunks = Vec::new();
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

        const DISCOVERY_CHUNK_SIZE: usize = 512 * 1024; // 512KB chunks
        let mut offset = 0;
        while offset < bytes.len() {
            let chunk_end = (offset + DISCOVERY_CHUNK_SIZE).min(bytes.len());
            let chunk_bytes = &bytes[offset..chunk_end];
            let chunk_va = section.virtual_address + offset as u64;
            chunks.push(ScanChunk {
                bytes: chunk_bytes,
                virtual_address: chunk_va,
            });
            offset = chunk_end;
        }
    }

    use rayon::prelude::*;

    let (total_decoded, call_targets, jump_targets, jump_edges) = chunks
        .into_par_iter()
        .map(|chunk| {
            let mut local_calls = Vec::with_capacity(4096);
            let mut local_jumps = Vec::with_capacity(4096);
            let mut local_edges = Vec::with_capacity(1024);
            let count = collect_section_targets(
                binary,
                &frontend,
                profile,
                chunk.bytes,
                chunk.virtual_address,
                &mut local_calls,
                &mut local_jumps,
                &mut local_edges,
            );
            local_calls.sort_unstable();
            local_calls.dedup();
            local_jumps.sort_unstable();
            local_jumps.dedup();
            local_edges.sort_unstable();
            local_edges.dedup();
            (count, local_calls, local_jumps, local_edges)
        })
        .reduce(
            || (0usize, Vec::new(), Vec::new(), Vec::new()),
            |(count_a, mut calls_a, mut jumps_a, mut edges_a), (count_b, mut calls_b, mut jumps_b, mut edges_b)| {
                calls_a.append(&mut calls_b);
                calls_a.sort_unstable();
                calls_a.dedup();

                jumps_a.append(&mut jumps_b);
                jumps_a.sort_unstable();
                jumps_a.dedup();

                edges_a.append(&mut edges_b);
                edges_a.sort_unstable();
                edges_a.dedup();

                (count_a + count_b, calls_a, jumps_a, edges_a)
            },
        );

    report.decoded_instruction_count = total_decoded;
    report.call_target_count = call_targets.len();
    report.jump_target_count = jump_targets.len();

    let mut tracker_seeds: std::collections::HashSet<u64> = binary.functions.iter().map(|f| f.address).collect();
    tracker_seeds.extend(call_targets.iter().copied());
    
    let pdb_keys: Vec<u64> = binary.pdb_functions.keys().copied().collect();
    let _ = std::fs::write(
        "/tmp/fission_pdb_truth.json",
        format!("{:?}", pdb_keys)
    );

    let mut all_references = std::collections::HashSet::new();
    all_references.extend(call_targets.iter().copied());
    all_references.extend(jump_targets.iter().copied());

    let mut candidates = discovery_candidate_targets(profile, call_targets, &jump_targets);

    let valid_call_targets: Vec<u64> = candidates
        .par_iter()
        .copied()
        .filter_map(|addr| {
            let bytes = binary.view_bytes(addr, 1)?;
            if bytes[0] == 0xcc || bytes[0] == 0x90 { return None; }
            let mut local_cache = std::collections::HashMap::new();
            let empty_known = std::collections::HashSet::new();
            let (valid, _) = validate_subroutine_candidate(
                binary, &frontend, addr, 3, 4000, true, &empty_known, &mut local_cache, Some(&all_references)
            );
            if valid { Some(addr) } else { None }
        })
        .collect();
    eprintln!("SCANNER_STATS: call_targets validated = {} / {}", valid_call_targets.len(), candidates.len());
    candidates = valid_call_targets.clone();

    if profile != FunctionDiscoveryProfile::Conservative {
        let mut tracker = InstructionBoundaryTracker::build(binary, &frontend, &tracker_seeds);
        let mut validation_cache = std::collections::HashMap::new();

        let mut all_known: std::collections::HashSet<u64> = binary.functions.iter().map(|f| f.address).collect();
        all_known.extend(candidates.iter().copied());

        if profile == FunctionDiscoveryProfile::Aggressive {
            // Disabled valid_jumps: it causes 1700+ FPs by blindly accepting jump targets
        }

        let mut data_refs = scan_data_references(binary, &frontend, &executable_ranges, &all_known, &mut validation_cache, Some(&all_references));
        data_refs.retain(|&addr| !tracker.is_overlap(addr));
        eprintln!("SCANNER_STATS: data_refs={}", data_refs.len());
        for &dr in &data_refs {
            tracker.add_function(binary, &frontend, dr);
        }
        candidates.extend(data_refs.clone());
        all_known.extend(data_refs.clone());

        // Ghidra XML static patterns (x86-64win / x86win)
        let mut xml_hits = scan_ghidra_patterns(binary, &frontend, &executable_ranges, &all_known, &tracker, &mut validation_cache, Some(&all_references));
        xml_hits.retain(|&addr| !tracker.is_overlap(addr));
        eprintln!("SCANNER_STATS: xml_hits={}", xml_hits.len());
        for &xh in &xml_hits {
            tracker.add_function(binary, &frontend, xh);
        }
        candidates.extend(xml_hits.clone());
        all_known.extend(xml_hits.clone());

        let mut dynamic = scan_dynamic_prologues(binary, &frontend, &executable_ranges, &all_known, &tracker, &mut validation_cache);
        dynamic.retain(|&addr| !tracker.is_overlap(addr));
        eprintln!("SCANNER_STATS: dynamic_prologues={}", dynamic.len());
        for &dyn_addr in &dynamic {
            tracker.add_function(binary, &frontend, dyn_addr);
        }
        candidates.extend(dynamic.clone());
        all_known.extend(dynamic.clone());

        let _ = std::fs::write("/tmp/fission_fp_stats.json", format!(
            "{{\"call_targets\": {:?}, \"data_refs\": {:?}, \"xml_hits\": {:?}, \"dynamic\": {:?}}}",
            valid_call_targets, data_refs, xml_hits, dynamic
        ));

        if candidates.contains(&0x10001000) { eprintln!("FOUND 10001000 in scanners"); }
        if jump_targets.contains(&0x10001000) { eprintln!("FOUND 10001000 in jump_targets"); }
        if binary.function_addr_index.contains_key(&0x10001000) { eprintln!("FOUND 10001000 in binary functions"); }

        // Disabled thunks: causes a massive amount of FPs
        // let mut thunks = scan_jmp_thunks(binary, &frontend, &executable_ranges, &all_known);
        // thunks.retain(|&addr| !tracker.is_overlap(addr));
        // eprintln!("SCANNER_STATS: thunks={}", thunks.len());
        // for &thunk_addr in &thunks {
        //     tracker.add_function(binary, &frontend, thunk_addr);
        // }
        // candidates.extend(thunks.clone());
        // all_known.extend(thunks);

        // Disabled cc_padding: blindly adds functions based on padding, causing FPs
        // let mut cc_total = 0;
        // loop {
        //     let mut cc_hits = scan_cc_padding_regions(binary, &frontend, &executable_ranges, &all_known, &tracker, &mut validation_cache);
        //     cc_hits.retain(|&addr| !tracker.is_overlap(addr));
        //     if cc_hits.is_empty() {
        //         break;
        //     }
        //     cc_total += cc_hits.len();
        //     for &cc in &cc_hits {
        //         tracker.add_function(binary, &frontend, cc);
        //     }
        //     candidates.extend(cc_hits.clone());
        //     all_known.extend(cc_hits);
        // }
        // eprintln!("SCANNER_STATS: cc_padding={}", cc_total);

        // [G2] Shared Return Analysis (Tail Call Recovery)
        if !jump_edges.is_empty() {
            let mut known_sorted: Vec<u64> = all_known.iter().copied().collect();
            known_sorted.sort_unstable();

            let mut shared_returns = Vec::new();
            for &(src, dest) in &jump_edges {
                if all_known.contains(&dest) {
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
                            shared_returns.push(dest);
                        }
                    }
                } else {
                    // Backward jump
                    if let Some(before) = function_before_src {
                        if dest < before {
                            shared_returns.push(dest);
                        }
                    }
                }
            }

            shared_returns.sort_unstable();
            shared_returns.dedup();
            
            let mut validated_shared_returns = Vec::new();
            for sr in shared_returns {
                if !tracker.is_overlap(sr) && is_strict_boundary(binary, sr) {
                    let (valid, _) = validate_subroutine_candidate(binary, &frontend, sr, 1, 4000, true, &all_known, &mut validation_cache, Some(&all_references));
                    if valid {
                        validated_shared_returns.push(sr);
                    }
                }
            }

            eprintln!("SCANNER_STATS: tail_calls={}", validated_shared_returns.len());
            for &sr in &validated_shared_returns {
                tracker.add_function(binary, &frontend, sr);
            }
            candidates.extend(validated_shared_returns.clone());
            all_known.extend(validated_shared_returns);
        }
    }

    candidates.sort_unstable();
    candidates.dedup();

    let mut accepted = Vec::new();
    for target in candidates {
        if binary.function_addr_index.contains_key(&target) { continue; }
        if crate::analysis::function_discovery::ranges::is_in_executable_ranges(target, &executable_ranges) {
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
    call_targets: &mut Vec<u64>,
    jump_targets: &mut Vec<u64>,
    jump_edges: &mut Vec<(u64, u64)>,
) -> usize {
    if profile == FunctionDiscoveryProfile::Conservative {
        let Ok(decoded) = frontend.decode_window(bytes, base_address, bytes.len()) else {
            return 0;
        };
        let decoded_count = decoded.len();
        for instruction in decoded {
            collect_instruction_targets(binary, &instruction, call_targets, jump_targets, jump_edges);
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
        jump_edges,
    )
}

fn collect_section_targets_resync(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    bytes: &[u8],
    base_address: u64,
    call_targets: &mut Vec<u64>,
    jump_targets: &mut Vec<u64>,
    jump_edges: &mut Vec<(u64, u64)>,
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
                    collect_instruction_targets(binary, instruction, call_targets, jump_targets, jump_edges);
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
                collect_instruction_targets(binary, &instruction, call_targets, jump_targets, jump_edges);
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


struct InstructionBoundaryTracker {
    boundaries: Vec<(u64, u64)>,
}

impl InstructionBoundaryTracker {
    fn build(
        binary: &LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        known_functions: &std::collections::HashSet<u64>,
    ) -> Self {
        use rayon::prelude::*;
        let boundaries: Vec<(u64, u64)> = known_functions
            .par_iter()
            .flat_map(|&addr| {
                let mut local_tracker = Self { boundaries: Vec::new() };
                local_tracker.add_function(binary, frontend, addr);
                local_tracker.boundaries
            })
            .collect();

        let mut tracker = Self { boundaries };
        tracker.boundaries.sort_unstable_by_key(|&(start, _)| start);
        tracker.boundaries.dedup_by_key(|&mut (start, _)| start);
        tracker
    }

    fn add_function(
        &mut self,
        binary: &LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        addr: u64,
    ) {
        let exec_ranges = super::ranges::executable_ranges(binary);
        let mut visited = std::collections::HashSet::new();
        let mut worklist = vec![addr];

        while let Some(curr) = worklist.pop() {
            if visited.contains(&curr) {
                continue;
            }

            let mut ip = curr;
            let mut count = 0;
            while count < 4000 {
                if !visited.insert(ip) {
                    break;
                }
                if !super::ranges::is_in_executable_ranges(ip, &exec_ranges) {
                    break;
                }
                    let Some(bytes) = binary.view_bytes(ip, 15) else {
                        break;
                    };
                    let Ok(decoded) = frontend.decode_window(bytes, ip, 1) else {
                        break;
                    };
                    if decoded.is_empty() {
                        break;
                    }
                    let inst = &decoded[0];
                    if inst.mnemonic.is_empty() || inst.mnemonic.to_lowercase() == "invalid" {
                        break;
                    }

                    self.boundaries.push((ip, ip + inst.length as u64));
                    count += 1;

                    match inst.flow_kind {
                        DecodedFlowKind::Return => {
                            break;
                        }
                        DecodedFlowKind::Jump => {
                            // Do not trace unconditional jumps to avoid enveloping tail call targets
                            break;
                        }
                        DecodedFlowKind::ConditionalJump => {
                            if let Some(target) = inst.direct_target {
                                let norm_target = crate::analysis::function_discovery::targets::normalize_target(binary, target);
                                if super::ranges::is_in_executable_ranges(norm_target, &exec_ranges) {
                                    worklist.push(norm_target);
                                }
                            }
                            ip += inst.length as u64;
                        }
                        DecodedFlowKind::Interrupt | DecodedFlowKind::Syscall => {
                            break;
                        }
                        _ => {
                            ip += inst.length as u64;
                        }
                    }
                }
            }

        // Merge into boundaries and deduplicate
        self.boundaries.sort_unstable_by_key(|&(start, _)| start);
        self.boundaries.dedup_by_key(|&mut (start, _)| start);
    }

    fn is_overlap(&self, addr: u64) -> bool {
        self.is_in_body(addr) || self.is_offcut(addr)
    }

    fn is_in_body(&self, addr: u64) -> bool {
        self.boundaries.binary_search_by_key(&addr, |&(start, _)| start).is_ok()
    }

    fn is_offcut(&self, addr: u64) -> bool {
        match self.boundaries.binary_search_by(|&(start, end)| {
            if addr <= start {
                std::cmp::Ordering::Greater
            } else if addr >= end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

fn validate_after_condition(
    binary: &LoadedBinary,
    addr: u64,
    cond: &str,
    executable_ranges: &[(u64, u64)],
) -> bool {
    if addr == 0 {
        return false;
    }
    let prev_addr = addr - 1;

    match cond {
        "defined" => {
            super::ranges::is_in_executable_ranges(prev_addr, executable_ranges)
        }
        "data" => {
            if !super::ranges::is_in_executable_ranges(prev_addr, executable_ranges) {
                return true;
            }
            if let Some(sec) = binary.sections.iter().find(|s| prev_addr >= s.virtual_address && prev_addr < s.virtual_address + s.virtual_size) {
                if !sec.is_executable {
                    return true;
                }
            }
            if let Some(bytes) = binary.view_bytes(prev_addr, 1) {
                if bytes[0] == 0x00 {
                    return true;
                }
            }
            false
        }
        "function" => {
            is_strict_boundary(binary, addr)
        }
        _ => true,
    }
}

/// Scan binary executable sections using Ghidra XML static patterns.
///
/// **Two-phase approach (mirrors Ghidra's pipeline):**
/// 1. Raw byte scan with first-byte index — O(section_size), no SLEIGH
/// 2. SLEIGH validation on unique raw hits — O(hits), once per address
fn scan_ghidra_patterns(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    executable_ranges: &[(u64, u64)],
    known_functions: &std::collections::HashSet<u64>,
    tracker: &InstructionBoundaryTracker,
    cache: &mut std::collections::HashMap<u64, ValidationResult>,
    global_references: Option<&std::collections::HashSet<u64>>,
) -> Vec<u64> {
    let arch_tag = if binary.is_64bit { "x86-64win" } else { "x86win" };
    let compiler_id = binary.get_ghidra_compiler_id();
    let patterns = load_ghidra_patterns(arch_tag, compiler_id.as_deref());
    if patterns.is_empty() {
        return Vec::new();
    }

    // Pre-index by first fixed byte of post_bytes + whether each pattern has pre_bytes
    let mut by_first_byte: std::collections::HashMap<u8, Vec<usize>> =
        std::collections::HashMap::new();
    let mut wildcard_start: Vec<usize> = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        if pat.post_bytes.is_empty() { continue; }
        match pat.post_bytes[0] {
            Some(b) => by_first_byte.entry(b).or_default().push(i),
            None    => wildcard_start.push(i),
        }
    }

    // Phase 1: raw byte scan — collect (addr, Option<usize>) where value is the minimum valid_code_min
    let mut raw_hits: std::collections::HashMap<u64, Option<usize>> =
        std::collections::HashMap::new();

    // Build AhoCorasick automaton for the first contiguous block of fixed bytes
    let mut ac_patterns = Vec::new();
    let mut pattern_map: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    let mut current_ac_index = 0;

    for (i, pat) in patterns.iter().enumerate() {
        let mut prefix = Vec::new();
        for b in &pat.post_bytes {
            if let Some(byte) = b {
                prefix.push(*byte);
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

    let ac = aho_corasick::AhoCorasickBuilder::new()
        .match_kind(aho_corasick::MatchKind::LeftmostFirst)
        .build(&ac_patterns)
        .unwrap();

    use rayon::prelude::*;

    for section in &binary.sections {
        if !section.is_executable { continue; }
        let Some(bytes) = binary.view_bytes(section.virtual_address, section.virtual_size as usize)
        else { continue; };
        let base = section.virtual_address;

        let section_hits: Vec<(u64, Option<usize>)> = ac.find_overlapping_iter(bytes)
            .map(|mat| {
                let offset = mat.start();
                let ac_idx = mat.pattern().as_usize();
                let addr = base + offset as u64;
                (offset, ac_idx, addr)
            })
            .collect::<Vec<_>>()
            .into_par_iter()
            .filter_map(|(offset, ac_idx, addr)| {
                if !is_in_executable_ranges(addr, executable_ranges) { return None; }

                if let Some(indices) = pattern_map.get(&ac_idx) {
                    for &i in indices.iter() {
                        if patterns[i].matches(bytes, base, addr) {
                            let pat = &patterns[i];
                            let boundary_ok = if pat.pre_bytes.is_empty() {
                                if let Some(cond) = &pat.after_cond {
                                    validate_after_condition(binary, addr, cond, executable_ranges)
                                } else {
                                    is_strict_boundary(binary, addr)
                                }
                            } else {
                                is_strict_boundary(binary, addr)
                            };

                            if boundary_ok {
                                return Some((addr, pat.valid_code_min));
                            }
                        }
                    }
                }
                None
            })
            .collect();

        // Sequential fallback for patterns that start with wildcard
        let wildcard_hits: Vec<(u64, Option<usize>)> = (0..bytes.len())
            .into_par_iter()
            .filter_map(|offset| {
                if wildcard_start.is_empty() { return None; }
                let addr = base + offset as u64;
                if !is_in_executable_ranges(addr, executable_ranges) { return None; }

                for &i in &wildcard_start {
                    if patterns[i].matches(bytes, base, addr) {
                        let pat = &patterns[i];
                        let boundary_ok = if pat.pre_bytes.is_empty() {
                            if let Some(cond) = &pat.after_cond {
                                validate_after_condition(binary, addr, cond, executable_ranges)
                            } else {
                                is_strict_boundary(binary, addr)
                            }
                        } else {
                            is_strict_boundary(binary, addr)
                        };

                        if boundary_ok {
                            return Some((addr, pat.valid_code_min));
                        }
                    }
                }
                None
            })
            .collect();

        for (addr, min_val) in section_hits.into_iter().chain(wildcard_hits) {
            let entry = raw_hits.entry(addr).or_insert(None);
            if let Some(min) = min_val {
                *entry = Some(entry.map_or(min, |v| v.min(min)));
            }
        }
    }

    eprintln!(
        "[ghidra-patterns] {}: raw hits = {}",
        if binary.is_64bit { "x86-64" } else { "x86" },
        raw_hits.len()
    );

    // Phase 2: SLEIGH validation — once per unique address
    let mut hits: Vec<u64> = raw_hits
        .into_par_iter()
        .filter_map(|(addr, valid_code_min)| {
            if tracker.is_offcut(addr) {
                return None;
            }
            let min_inst = valid_code_min.unwrap_or(3);
            let mut local_cache = std::collections::HashMap::new();
            if validate_subroutine_candidate(
                binary, frontend, addr, min_inst, 4000, true, known_functions, &mut local_cache, global_references,
            ).0 {
                Some(addr)
            } else {
                None
            }
        })
        .collect();

    eprintln!(
        "[ghidra-patterns] after SLEIGH validation: {} hits",
        hits.len()
    );

    hits.sort_unstable();
    hits
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
fn is_strict_boundary(binary: &LoadedBinary, addr: u64) -> bool {
    // Look back up to 16 bytes to find a boundary
    let mut check_addr = addr;
    let mut skipped = 0;
    while skipped < 16 && check_addr > 0 {
        check_addr -= 1;
        let Some(byte_slice) = binary.view_bytes(check_addr, 1) else { break; };
        let b = byte_slice[0];
        if b == 0xcc || b == 0x90 {
            skipped += 1;
            continue;
        }

        // We found a non-padding byte `b` at `check_addr`.
        // Check if the instruction ending at `check_addr` is a terminator.

        // 1. RET (0xc3)
        if b == 0xc3 {
            return true;
        }

        // 2. RET imm16 (0xc2 xx xx): last byte at check_addr, opcode at check_addr-2
        if check_addr >= 2 {
            if let Some(op) = binary.view_bytes(check_addr - 2, 1) {
                if op[0] == 0xc2 {
                    return true;
                }
            }
        }

        // 3. JMP short (0xeb xx): opcode at check_addr-1
        if check_addr >= 1 {
            if let Some(op) = binary.view_bytes(check_addr - 1, 1) {
                if op[0] == 0xeb {
                    return true;
                }
            }
        }

        // 4. JMP near (0xe9 xx xx xx xx): opcode at check_addr-4
        if check_addr >= 4 {
            if let Some(op) = binary.view_bytes(check_addr - 4, 5) {
                if op[0] == 0xe9 {
                    let rel32 = i32::from_le_bytes([op[1], op[2], op[3], op[4]]);
                    let next_ip = check_addr + 1;
                    let target = (next_ip as i64 + rel32 as i64) as u64;
                    let exec_ranges = crate::analysis::function_discovery::ranges::executable_ranges(binary);
                    if crate::analysis::function_discovery::ranges::is_in_executable_ranges(target, &exec_ranges) {
                        return true;
                    }
                }
            }
        }

        // 5. CALL near (0xe8 xx xx xx xx): opcode at check_addr-4
        //    CALL-terminated code is a valid boundary: the callee returns to
        //    the following byte, but the compiler may place a new subroutine there.
        if check_addr >= 4 {
            if let Some(op) = binary.view_bytes(check_addr - 4, 5) {
                if op[0] == 0xe8 {
                    let rel32 = i32::from_le_bytes([op[1], op[2], op[3], op[4]]);
                    let next_ip = check_addr + 1;
                    let target = (next_ip as i64 + rel32 as i64) as u64;
                    let exec_ranges = crate::analysis::function_discovery::ranges::executable_ranges(binary);
                    if crate::analysis::function_discovery::ranges::is_in_executable_ranges(target, &exec_ranges) {
                        return true;
                    }
                }
            }
        }

        // 6. JMP/CALL indirect via ModRM ff/2, ff/4, ff/d0..ff/d3 etc.
        //    These encode as: FF /4 (JMP [mem]) or FF /2 (CALL [mem])
        //    Min 2 bytes (FF D0), max 6 bytes (FF 15 addr32) before check_addr.
        // FF D0..FF D3 (CALL r/m32 with register operand), 2 bytes: opcode at check_addr-1
        if check_addr >= 1 {
            if let Some(op) = binary.view_bytes(check_addr - 1, 2) {
                if op[0] == 0xff {
                    let modrm = op[1];
                    let reg = (modrm >> 3) & 0x7;
                    // /2 = CALL r/m, /4 = JMP r/m
                    if (reg == 2 || reg == 4) && (modrm >> 6) == 3 {
                        return true;
                    }
                }
            }
        }
        // FF /2 or FF /4 with ModRM+SIB (3-6 bytes): check a few offsets
        for back in [2usize, 5, 6] {
            if check_addr as usize >= back {
                if let Some(op) = binary.view_bytes(check_addr - back as u64, 1) {
                    if op[0] == 0xff {
                        if let Some(mrm) = binary.view_bytes(check_addr - back as u64 + 1, 1) {
                            let reg = (mrm[0] >> 3) & 0x7;
                            if reg == 2 || reg == 4 {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        break;
    }

    // If we skipped at least 4 bytes of padding, accept it even if we couldn't
    // resolve the preceding terminator opcode.
    if skipped >= 4 {
        return true;
    }

    false
}

/// Fallback scanner: scan gaps between known functions for 4+ bytes of
/// `0xCC`/`0x90` padding followed by an un-discovered subroutine.
///
/// Rather than scanning the entire section byte-by-byte, this scanner
/// iterates over *gaps* between already-discovered functions. For each gap
/// it checks whether a 4+-byte padding run appears anywhere inside and, if
/// so, probes the first non-padding byte as a function candidate.
///
/// This avoids the O(section_size × validate_cost) blow-up that a full
/// section scan produces on large binaries like EverPlanet.
///
/// Uses `tracker.is_offcut()` to reject addresses that fall inside known
/// function bodies.
fn scan_cc_padding_regions(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    executable_ranges: &[(u64, u64)],
    known_functions: &std::collections::HashSet<u64>,
    tracker: &InstructionBoundaryTracker,
    cache: &mut std::collections::HashMap<u64, ValidationResult>,
    global_references: Option<&std::collections::HashSet<u64>>,
) -> Vec<u64> {
    // Allow 2+ cc/90 bytes as padding when the terminator just before them
    // already confirms a function-boundary (is_strict_boundary will verify
    // this for each candidate).  Smaller values reduce FP in the validate
    // stage, so we enforce is_strict_boundary as a mandatory gate.
    const MIN_CC_PADDING: usize = 2;
    // Require at least 4 instructions so that short shared-epilogue stubs
    // (epilogue-only fragments without a real prologue) do not accumulate as
    // spurious entries.
    const MIN_INSNS: usize = 4;
    let mut results = Vec::new();

    // Collect all executable section boundaries.
    let mut section_spans: Vec<(u64, u64)> = binary
        .sections
        .iter()
        .filter(|s| s.is_executable)
        .filter_map(|s| {
            binary
                .view_bytes(s.virtual_address, s.virtual_size as usize)
                .map(|_| (s.virtual_address, s.virtual_address + s.virtual_size as u64))
        })
        .collect();
    section_spans.sort_unstable();

    // Build a sorted list of known function addresses that fall within executable sections.
    let mut known_sorted: Vec<u64> = known_functions
        .iter()
        .copied()
        .filter(|&a| is_in_executable_ranges(a, executable_ranges))
        .collect();
    known_sorted.sort_unstable();
    known_sorted.dedup();

    // For every gap [gap_start, gap_end) between consecutive known functions,
    // search for the first 4+-byte cc/90 run and return the byte immediately
    // after it as a function-candidate address.
    //
    // We scan *forward* from gap_start because function-boundary INT3/NOP
    // padding may appear anywhere in the gap (not necessarily at the very end),
    // and the caller ensures gap boundaries are already constrained to within
    // a single executable section.
    let probe_gap = |gap_start: u64, gap_end: u64| -> Option<u64> {
        if gap_end <= gap_start + MIN_CC_PADDING as u64 {
            return None;
        }
        let len = (gap_end - gap_start) as usize;
        let Some(bytes) = binary.view_bytes(gap_start, len) else {
            return None;
        };

        let mut i = 0usize;
        while i < bytes.len() {
            let pad_start = i;
            while i < bytes.len() && (bytes[i] == 0xcc || bytes[i] == 0x90) {
                i += 1;
            }
            let pad_len = i - pad_start;

            if pad_len >= MIN_CC_PADDING && i < bytes.len() {
                let candidate = gap_start + i as u64;
                if bytes[i] != 0xcc
                    && bytes[i] != 0x90
                    && bytes[i] != 0x00
                    && !tracker.is_offcut(candidate)
                {
                    return Some(candidate);
                }
                // If rejected, keep scanning for more padding runs
                while i < bytes.len() && bytes[i] != 0xcc && bytes[i] != 0x90 {
                    i += 1;
                }
            } else if pad_len == 0 {
                i += 1;
            } else {
                while i < bytes.len() && bytes[i] != 0xcc && bytes[i] != 0x90 {
                    i += 1;
                }
            }
        }
        None
    };

    // Iterate over all gaps.
    // Include gaps before the first known function and after the last.
    for span in &section_spans {
        let (sec_start, sec_end) = *span;

        // Collect known functions within this section
        let in_section: Vec<u64> = known_sorted
            .iter()
            .copied()
            .filter(|&a| a >= sec_start && a < sec_end)
            .collect();

        // Build gap boundaries: [sec_start, first_fn), [fn_i, fn_{i+1}), [last_fn, sec_end)
        let mut boundaries: Vec<(u64, u64)> = Vec::new();
        if in_section.is_empty() {
            boundaries.push((sec_start, sec_end));
        } else {
            // Gap before first function
            if in_section[0] > sec_start {
                boundaries.push((sec_start, in_section[0]));
            }
            // Gaps between consecutive functions
            for w in in_section.windows(2) {
                boundaries.push((w[0], w[1]));
            }
            // Gap after last function
            if *in_section.last().unwrap() < sec_end {
                boundaries.push((*in_section.last().unwrap(), sec_end));
            }
        }

        for (gap_start, gap_end) in boundaries {
            let Some(candidate) = probe_gap(gap_start, gap_end) else {
                continue;
            };
            if known_functions.contains(&candidate) {
                continue;
            }
            if !is_in_executable_ranges(candidate, executable_ranges) {
                continue;
            }
            // Strict boundary gate: require a valid terminator (RET/JMP/CALL)
            // immediately before the padding run.  This eliminates interior
            // padding hits that lack a real function-end context, which is the
            // primary source of FP inflation on large binaries.
            if !is_strict_boundary(binary, candidate) {
                continue;
            }
            let (valid, _) = validate_subroutine_candidate(
                binary, frontend, candidate, MIN_INSNS, 4000, true, known_functions, cache, global_references,
            );
            if valid {
                results.push(candidate);
            }
        }
    }

    results
}


fn scan_data_references(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    executable_ranges: &[(u64, u64)],
    known_functions: &std::collections::HashSet<u64>,
    cache: &mut std::collections::HashMap<u64, ValidationResult>,
    global_references: Option<&std::collections::HashSet<u64>>,
) -> Vec<u64> {
    use rayon::prelude::*;

    let ptr_size = if binary.is_64bit { 8 } else { 4 };
    
    let mut candidates: Vec<u64> = binary.sections
        .par_iter()
        .filter(|sec| !sec.is_executable && sec.is_readable)
        .flat_map(|section| {
            let mut sec_candidates = Vec::new();
            if let Some(bytes) = binary.view_bytes(section.virtual_address, section.virtual_size as usize) {
                let mut offset = 0;
                while offset + ptr_size <= bytes.len() {
                    let val = if ptr_size == 8 {
                        u64::from_le_bytes(bytes[offset..offset+8].try_into().unwrap())
                    } else {
                        u32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap()) as u64
                    };
                    if crate::analysis::function_discovery::ranges::is_in_executable_ranges(val, executable_ranges) {
                        if is_strict_boundary(binary, val) {
                            sec_candidates.push(val);
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

    let refs: Vec<u64> = candidates
        .into_par_iter()
        .filter_map(|val| {
            let mut local_cache = std::collections::HashMap::new();
            let (is_valid, res) = validate_subroutine_candidate(binary, frontend, val, 1, 4000, true, known_functions, &mut local_cache, global_references);
            if is_valid && res.has_call_to_valid {
                Some(val)
            } else {
                None
            }
        })
        .collect();

    refs
}

fn scan_dynamic_prologues(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    _executable_ranges: &[(u64, u64)],
    known_functions: &std::collections::HashSet<u64>,
    tracker: &InstructionBoundaryTracker,
    cache: &mut std::collections::HashMap<u64, ValidationResult>,
) -> Vec<u64> {
    Vec::new()
}


fn scan_jmp_thunks(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    _executable_ranges: &[(u64, u64)],
    _known_functions: &std::collections::HashSet<u64>,
) -> Vec<u64> {
    let mut thunks = Vec::new();
    let exec_ranges = executable_ranges(binary);

    for section in &binary.sections {
        if !section.is_executable { continue; }
        let Some(bytes) = binary.view_bytes(section.virtual_address, section.virtual_size as usize) else { continue; };
        let mut offset = 0;
        while offset + 5 <= bytes.len() {
            if bytes[offset] == 0xe9 {
                let addr = section.virtual_address + offset as u64;
                if is_strict_boundary(binary, addr) {
                    if let Some(target_bytes) = binary.view_bytes(addr, 15) {
                        if let Ok(decoded) = frontend.decode_window(target_bytes, addr, 1) {
                            if !decoded.is_empty() && decoded[0].flow_kind == DecodedFlowKind::Jump {
                                if let Some(target) = decoded[0].direct_target {
                                    let norm_target = crate::analysis::function_discovery::targets::normalize_target(binary, target);
                                    if is_in_executable_ranges(norm_target, &exec_ranges) {
                                        thunks.push(addr);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            offset += 1;
        }
    }
    thunks
}


#[derive(Clone, Copy, Debug)]
pub(crate) struct ValidationResult {
    count: usize,
    did_terminate: bool,
    has_call_to_valid: bool,
    adds_info: bool,
}

pub(crate) fn validate_subroutine_candidate(
    binary: &LoadedBinary,
    frontend: &RuntimeSleighFrontend,
    addr: u64,
    min_instructions: usize,
    max_instructions: usize,
    must_terminate: bool,
    known_functions: &std::collections::HashSet<u64>,
    cache: &mut std::collections::HashMap<u64, ValidationResult>,
    global_references: Option<&std::collections::HashSet<u64>>,
) -> (bool, ValidationResult) {
    let res = if let Some(&cached) = cache.get(&addr) {
        cached
    } else {
        // Safety check: candidate must be in executable memory
        let exec_ranges = crate::analysis::function_discovery::ranges::executable_ranges(binary);
        if !crate::analysis::function_discovery::ranges::is_in_executable_ranges(addr, &exec_ranges) {
            let val = ValidationResult {
                count: 0,
                did_terminate: false,
                has_call_to_valid: false,
                adds_info: false,
            };
            cache.insert(addr, val);
            return (false, val);
        }

        let mut visited = std::collections::HashSet::new();
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

                if !crate::analysis::function_discovery::ranges::is_in_executable_ranges(ip, &exec_ranges) {
                    invalid = true;
                    break;
                }

                let Some(bytes) = binary.view_bytes(ip, 15) else {
                    invalid = true;
                    break;
                };

                let Ok(decoded) = frontend.decode_window(bytes, ip, 1) else {
                    invalid = true;
                    break;
                };

                if decoded.is_empty() {
                    invalid = true;
                    break;
                }

                let inst = &decoded[0];
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
                    let norm_target = crate::analysis::function_discovery::targets::normalize_target(binary, target);
                    if known_functions.contains(&norm_target) {
                        adds_info = true;
                        if inst.flow_kind == DecodedFlowKind::Call {
                            has_call_to_valid = true;
                            
                            // [G7] noReturn CALL handling
                            let mut is_noreturn = false;
                            let mut name = None;
                            let mut lib = None;
                            if let Some(&idx) = binary.function_addr_index.get(&norm_target) {
                                if let Some(f) = binary.functions.get(idx) {
                                    name = Some(f.name.as_str());
                                    lib = f.external_library.as_deref();
                                }
                            }
                            if name.is_none() {
                                if let Some(n) = binary.iat_symbols.get(&norm_target) {
                                    name = Some(n.as_str());
                                } else if let Some(n) = binary.global_symbols.get(&norm_target) {
                                    name = Some(n.as_str());
                                }
                            }
                            if let Some(n) = name {
                                let fmt = fission_core::core::ghidra_no_return::binary_format_to_ghidra_format(&binary.format).unwrap_or("");
                                let comp = binary.get_ghidra_compiler_id();
                                let idx = fission_core::core::ghidra_no_return::ghidra_no_return_index();
                                is_noreturn = idx.is_no_return(fmt, comp.as_deref(), lib, n);
                            }
                            if is_noreturn {
                                did_terminate = true;
                                break;
                            }
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
                            let norm_target = crate::analysis::function_discovery::targets::normalize_target(binary, target);
                            // If it's a tail call to a known function, treat as termination
                            if known_functions.contains(&norm_target) {
                                did_terminate = true;
                                adds_info = true;
                                break;
                            }
                            if crate::analysis::function_discovery::ranges::is_in_executable_ranges(norm_target, &exec_ranges) {
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
                            let norm_target = crate::analysis::function_discovery::targets::normalize_target(binary, target);
                            if crate::analysis::function_discovery::ranges::is_in_executable_ranges(norm_target, &exec_ranges) {
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
            ValidationResult {
                count: 0,
                did_terminate: false,
                has_call_to_valid: false,
                adds_info: false,
            }
        } else {
            ValidationResult {
                count,
                did_terminate,
                has_call_to_valid,
                adds_info,
            }
        };
        cache.insert(addr, val);
        val
    };

    let is_valid = (!must_terminate || res.did_terminate || res.has_call_to_valid) && res.count >= min_instructions;
    (is_valid, res)
}
