use super::{FunctionDiscoveryProfile, FunctionInfo, LoadedBinary};
use fission_disasm::DisasmEngine;
use std::collections::HashSet;

const X64_CONSERVATIVE_PROLOGUES: &[&[u8]] = &[
    &[0x55, 0x48, 0x89, 0xe5],
    &[0x48, 0x83, 0xec],
    &[0x48, 0x81, 0xec],
    &[0x55, 0x48, 0x8d, 0x2c, 0x24],
    &[0x40, 0x55, 0x48, 0x8b, 0xec],
    &[0x48, 0x8b, 0xc4],
];

const X64_BALANCED_PROLOGUES: &[&[u8]] = &[
    &[0x55, 0x48, 0x89, 0xe5],
    &[0x48, 0x83, 0xec],
    &[0x48, 0x81, 0xec],
    &[0x55, 0x48, 0x8d, 0x2c, 0x24],
    &[0x40, 0x55, 0x48, 0x8b, 0xec],
    &[0x48, 0x8b, 0xc4],
    &[0x48, 0x89, 0x5c, 0x24],
    &[0x48, 0x89, 0x4c, 0x24],
    &[0x48, 0x89, 0x54, 0x24],
    &[0x48, 0x89, 0x44, 0x24],
];

const X64_AGGRESSIVE_PROLOGUES: &[&[u8]] = &[
    &[0x55, 0x48, 0x89, 0xe5],
    &[0x48, 0x83, 0xec],
    &[0x48, 0x81, 0xec],
    &[0x55, 0x48, 0x8d, 0x2c, 0x24],
    &[0x40, 0x55, 0x48, 0x8b, 0xec],
    &[0x48, 0x8b, 0xc4],
    &[0x48, 0x89, 0x5c, 0x24],
    &[0x48, 0x89, 0x4c, 0x24],
    &[0x48, 0x89, 0x54, 0x24],
    &[0x48, 0x89, 0x44, 0x24],
    &[0x40, 0x55],
    &[0x48, 0x55],
];

const X86_CONSERVATIVE_PROLOGUES: &[&[u8]] =
    &[&[0x55, 0x89, 0xe5], &[0x55, 0x8b, 0xec], &[0x83, 0xec], &[0x81, 0xec]];

const X86_BALANCED_PROLOGUES: &[&[u8]] = &[
    &[0x55, 0x57, 0x8B, 0xEC],
    &[0x55, 0x56, 0x8B, 0xEC],
    &[0x55, 0x53, 0x8B, 0xEC],
    &[0x57, 0x55, 0x8B, 0xEC],
    &[0x56, 0x55, 0x8B, 0xEC],
    &[0x53, 0x55, 0x8B, 0xEC],
    &[0x55, 0x89, 0xe5],
    &[0x55, 0x8b, 0xec],
    &[0x83, 0xec],
    &[0x81, 0xec],
];

const X86_AGGRESSIVE_PROLOGUES: &[&[u8]] = &[
    &[0x55, 0x57, 0x8B, 0xEC],
    &[0x55, 0x56, 0x8B, 0xEC],
    &[0x55, 0x53, 0x8B, 0xEC],
    &[0x57, 0x55, 0x8B, 0xEC],
    &[0x56, 0x55, 0x8B, 0xEC],
    &[0x53, 0x55, 0x8B, 0xEC],
    &[0x55, 0x89, 0xe5],
    &[0x55, 0x8b, 0xec],
    &[0x83, 0xec],
    &[0x81, 0xec],
];

fn prologue_patterns(profile: FunctionDiscoveryProfile, is_64bit: bool) -> &'static [&'static [u8]] {
    if is_64bit {
        match profile {
            FunctionDiscoveryProfile::Conservative => X64_CONSERVATIVE_PROLOGUES,
            FunctionDiscoveryProfile::Balanced => X64_BALANCED_PROLOGUES,
            FunctionDiscoveryProfile::Aggressive => X64_AGGRESSIVE_PROLOGUES,
        }
    } else {
        match profile {
            FunctionDiscoveryProfile::Conservative => X86_CONSERVATIVE_PROLOGUES,
            FunctionDiscoveryProfile::Balanced => X86_BALANCED_PROLOGUES,
            FunctionDiscoveryProfile::Aggressive => X86_AGGRESSIVE_PROLOGUES,
        }
    }
}

fn max_prologue_scan_bytes(profile: FunctionDiscoveryProfile) -> usize {
    match profile {
        FunctionDiscoveryProfile::Conservative => 512 * 1024,
        FunctionDiscoveryProfile::Balanced => 2 * 1024 * 1024,
        FunctionDiscoveryProfile::Aggressive => usize::MAX,
    }
}

fn collect_rel_jmp_targets(
    data: &[u8],
    section_base: u64,
    targets: &mut HashSet<u64>,
) {
    let mut i = 0usize;
    while i + 5 <= data.len() {
        if data[i] == 0xE9 {
            let rel = i32::from_le_bytes([data[i + 1], data[i + 2], data[i + 3], data[i + 4]]);
            let insn_addr = section_base + i as u64;
            let target = (insn_addr.wrapping_add(5)).wrapping_add(rel as i64 as u64);
            targets.insert(target);
            i += 5;
            continue;
        }
        i += 1;
    }
}

fn has_entry_prefix_at(binary: &LoadedBinary, address: u64, patterns: &[&[u8]]) -> bool {
    let bytes = binary.data.as_slice();
    for section in &binary.sections {
        if !section.is_executable {
            continue;
        }

        let sec_start = section.virtual_address;
        let sec_end = section.virtual_address.saturating_add(section.virtual_size);
        if address < sec_start || address >= sec_end {
            continue;
        }

        let delta = address.saturating_sub(sec_start);
        let file_off = section.file_offset.saturating_add(delta) as usize;
        if file_off >= bytes.len() {
            return false;
        }

        return patterns
            .iter()
            .any(|pat| file_off + pat.len() <= bytes.len() && bytes[file_off..file_off + pat.len()] == **pat);
    }

    false
}

fn byte_at_executable_va(binary: &LoadedBinary, address: u64) -> Option<u8> {
    let bytes = binary.data.as_slice();
    for section in &binary.sections {
        if !section.is_executable {
            continue;
        }
        let sec_start = section.virtual_address;
        let sec_end = section.virtual_address.saturating_add(section.virtual_size);
        if address < sec_start || address >= sec_end {
            continue;
        }

        let delta = address.saturating_sub(sec_start);
        let file_off = section.file_offset.saturating_add(delta) as usize;
        if file_off < bytes.len() {
            return Some(bytes[file_off]);
        }
        return None;
    }
    None
}

fn is_padding_byte(b: u8) -> bool {
    matches!(b, 0x00 | 0x90 | 0xCC)
}

fn passes_short_prologue_gate(binary: &LoadedBinary, address: u64, is_64bit: bool) -> bool {
    let align = if is_64bit { 16 } else { 4 };
    if address % align == 0 {
        return true;
    }

    if address == 0 {
        return false;
    }

    match byte_at_executable_va(binary, address.saturating_sub(1)) {
        Some(prev) => is_padding_byte(prev),
        None => false,
    }
}

fn collect_executable_ranges(binary: &LoadedBinary) -> Vec<(u64, u64)> {
    binary
        .sections
        .iter()
        .filter(|s| s.is_executable)
        .map(|s| (s.virtual_address, s.virtual_address.saturating_add(s.virtual_size)))
        .collect()
}

fn is_in_executable_ranges(target: u64, ranges: &[(u64, u64)]) -> bool {
    ranges
        .iter()
        .any(|&(start, end)| target >= start && target < end)
}

fn normalize_target_addr(target: u64, is_64bit: bool) -> u64 {
    if is_64bit {
        target
    } else {
        target & 0xFFFF_FFFF
    }
}

fn insert_discovery_candidate(
    candidates: &mut HashSet<u64>,
    known: &std::collections::HashMap<u64, usize>,
    target: u64,
    is_64bit: bool,
    exec_ranges: &[(u64, u64)],
) {
    let normalized = normalize_target_addr(target, is_64bit);
    if known.contains_key(&normalized) {
        return;
    }
    if is_in_executable_ranges(normalized, exec_ranges) {
        candidates.insert(normalized);
    }
}

impl LoadedBinary {
    /// Discover internal functions by scanning executable code for CALL instructions
    /// This finds functions that are called but not exported/imported
    pub fn discover_internal_functions(&mut self) {
        self.discover_internal_functions_with_profile(FunctionDiscoveryProfile::Conservative);
    }

    /// Profile-aware internal function discovery. Higher profiles increase recall
    /// with extra branch-target harvesting heuristics.
    pub fn discover_internal_functions_with_profile(&mut self, profile: FunctionDiscoveryProfile) {
        let engine = match DisasmEngine::new(self.is_64bit) {
            Ok(e) => e,
            Err(_) => return,
        };

        let executable_ranges = collect_executable_ranges(self);

        let total_code_size: u64 = executable_ranges.iter().map(|(s, e)| e - s).sum();
        let estimated_functions = (total_code_size / 100) as usize;
        let mut discovered: HashSet<u64> = HashSet::with_capacity(estimated_functions.max(64));
        let entry_gate_patterns = prologue_patterns(FunctionDiscoveryProfile::Balanced, self.is_64bit);

        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            let start = section.file_offset as usize;
            let size = section.file_size as usize;
            if start + size > self.data.as_slice().len() {
                continue;
            }
            let bytes = &self.data.as_slice()[start..start + size];

            let targets = engine.discover_call_targets(bytes, section.virtual_address);

            for target in targets {
                insert_discovery_candidate(
                    &mut discovered,
                    &self.function_addr_index,
                    target,
                    self.is_64bit,
                    &executable_ranges,
                );
            }

            if matches!(profile, FunctionDiscoveryProfile::Aggressive) {
                let mut jmp_targets = HashSet::new();
                collect_rel_jmp_targets(bytes, section.virtual_address, &mut jmp_targets);

                for target in jmp_targets {
                    if !has_entry_prefix_at(self, target, entry_gate_patterns) {
                        continue;
                    }
                    insert_discovery_candidate(
                        &mut discovered,
                        &self.function_addr_index,
                        target,
                        self.is_64bit,
                        &executable_ranges,
                    );
                }
            }
        }

        self.functions.reserve(discovered.len());

        for addr in discovered {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
        }

        self.functions.sort_by_key(|f| f.address);
        self.functions_sorted = true;

        self.rebuild_function_indices();
    }

    /// Discover functions by scanning for common prologue patterns and CALL targets
    ///
    /// This is useful when the control flow is obfuscated (e.g., indirect calls)
    /// and standard call-graph usage fails to find all functions.
    pub fn discover_functions_by_prologue(&mut self) -> usize {
        self.discover_functions_by_prologue_with_profile(FunctionDiscoveryProfile::Conservative)
    }

    /// Profile-aware prologue discovery. Higher profiles scan wider sections and
    /// broaden pattern/branch matching to increase function recall.
    pub fn discover_functions_by_prologue_with_profile(
        &mut self,
        profile: FunctionDiscoveryProfile,
    ) -> usize {
        let mut count = 0;
        let mut candidates = HashSet::new();

        let patterns = prologue_patterns(profile, self.is_64bit);
        let entry_gate_patterns = prologue_patterns(FunctionDiscoveryProfile::Balanced, self.is_64bit);

        let exec_ranges = collect_executable_ranges(self);

        for section in &self.sections {
            if !section.is_executable {
                continue;
            }

            let start = section.file_offset as usize;
            let end = (section.file_offset + section.file_size) as usize;
            if end > self.data.as_slice().len() {
                continue;
            }

            let section_len = end.saturating_sub(start);
            let search_limit = section_len.min(max_prologue_scan_bytes(profile));
            let data = &self.data.as_slice()[start..start + search_limit];
            let va_start = section.virtual_address;

            for i in 0..data.len() {
                if i + 4 <= data.len() {
                    let window = &data[i..];
                    for pat in patterns {
                        if window.starts_with(pat) {
                            let potential_addr = va_start + i as u64;
                            if !self.function_addr_index.contains_key(&potential_addr) {
                                if pat.len() <= 3
                                    && !matches!(profile, FunctionDiscoveryProfile::Conservative)
                                    && !passes_short_prologue_gate(self, potential_addr, self.is_64bit)
                                {
                                    break;
                                }
                                candidates.insert(potential_addr);
                            }
                            break;
                        }
                    }
                }

                if i + 5 <= data.len() && data[i] == 0xE8 {
                    let rel_bytes = [data[i + 1], data[i + 2], data[i + 3], data[i + 4]];
                    let rel = i32::from_le_bytes(rel_bytes);

                    let call_insn_addr = va_start + i as u64;
                    let target_addr = (call_insn_addr.wrapping_add(5)).wrapping_add(rel as u64);

                    let addr_masked = normalize_target_addr(target_addr, self.is_64bit);
                    if is_in_executable_ranges(addr_masked, &exec_ranges)
                        && !self.function_addr_index.contains_key(&addr_masked)
                    {
                        candidates.insert(addr_masked);
                    }
                }

                if matches!(profile, FunctionDiscoveryProfile::Aggressive)
                    && i + 5 <= data.len()
                    && data[i] == 0xE9
                {
                    let rel_bytes = [data[i + 1], data[i + 2], data[i + 3], data[i + 4]];
                    let rel = i32::from_le_bytes(rel_bytes);

                    let jmp_insn_addr = va_start + i as u64;
                    let target_addr = (jmp_insn_addr.wrapping_add(5)).wrapping_add(rel as u64);

                    let addr_masked = normalize_target_addr(target_addr, self.is_64bit);
                    if is_in_executable_ranges(addr_masked, &exec_ranges)
                        && !self.function_addr_index.contains_key(&addr_masked)
                        && has_entry_prefix_at(self, addr_masked, entry_gate_patterns)
                    {
                        candidates.insert(addr_masked);
                    }
                }
            }
        }

        for addr in candidates {
            self.functions.push(FunctionInfo {
                name: format!("sub_{:x}_scanned", addr),
                address: addr,
                size: 0,
                is_export: false,
                is_import: false,
            });
            count += 1;
        }

        if count > 0 {
            self.functions.sort_by_key(|f| f.address);
            self.functions_sorted = true;
            self.rebuild_function_indices();
        }

        count
    }

    /// Rebuild function lookup indices after modifying the functions vector
    pub fn rebuild_function_indices(&mut self) {
        self.function_addr_index.clear();
        self.function_name_index.clear();

        let entries: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .map(|(idx, func)| (idx, func.address, func.name.clone()))
            .collect();

        for (idx, addr, name) in entries {
            self.function_addr_index.insert(addr, idx);
            if !name.is_empty() {
                self.function_name_index.insert(name, idx);
            }
        }
    }
}
