//! Binary search and string cross-reference analysis.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::{parse_address, MAX_SCAN_PER_SECTION};
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Search functions, strings, and addresses.
#[tauri::command]
pub async fn search_binary(
    query: String,
    state: State<'_, AppState>,
) -> CmdResult<Vec<SearchResultDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    let limit = 200;

    // Search functions (by name)
    for f in &binary.functions {
        let display_name = inner
            .renamed_functions
            .get(&f.address)
            .cloned()
            .unwrap_or_else(|| f.name.clone());

        if display_name.to_lowercase().contains(&q) {
            results.push(SearchResultDto {
                address: format!("0x{:x}", f.address),
                name: display_name,
                result_type: "function".to_string(),
                context: format!("size: {} bytes", f.size),
            });
            if results.len() >= limit {
                break;
            }
        }
    }

    // Search strings in readable sections (using view_bytes)
    if results.len() < limit {
        let min_len = fission_core::MIN_STRING_LENGTH;
        for section in &binary.sections {
            if results.len() >= limit {
                break;
            }
            // Skip empty or tiny sections
            if section.virtual_size < min_len as u64 {
                continue;
            }
            // Limit per-section scan to MAX_SCAN_PER_SECTION
            let scan_len = (section.virtual_size as usize).min(MAX_SCAN_PER_SECTION);
            if let Some(data) = binary.view_bytes(section.virtual_address, scan_len) {
                let mut current_start: Option<usize> = None;
                let mut current_str = Vec::new();

                for (i, &byte) in data.iter().enumerate() {
                    if (0x20..0x7f).contains(&byte) {
                        if current_start.is_none() {
                            current_start = Some(i);
                        }
                        current_str.push(byte);
                    } else {
                        if current_str.len() >= min_len {
                            if let (Some(start), Ok(s)) =
                                (current_start, std::str::from_utf8(&current_str))
                            {
                                if s.to_lowercase().contains(&q) {
                                    let addr = section.virtual_address + start as u64;
                                    results.push(SearchResultDto {
                                        address: format!("0x{:x}", addr),
                                        name: s.to_string(),
                                        result_type: "string".to_string(),
                                        context: format!(
                                            "section: {}, len: {}",
                                            section.name,
                                            s.len()
                                        ),
                                    });
                                    if results.len() >= limit {
                                        break;
                                    }
                                }
                            }
                        }
                        current_start = None;
                        current_str.clear();
                    }
                }
            }
        }
    }

    // If query looks like an address, add a direct match
    if let Some(addr) = parse_address(&q) {
        if results.len() < limit {
            let func_name = binary.function_at(addr).map(|f| {
                inner
                    .renamed_functions
                    .get(&f.address)
                    .cloned()
                    .unwrap_or_else(|| f.name.clone())
            });
            results.push(SearchResultDto {
                address: format!("0x{:x}", addr),
                name: func_name.unwrap_or_else(|| format!("0x{:x}", addr)),
                result_type: "address".to_string(),
                context: "direct address match".to_string(),
            });
        }
    }

    Ok(results)
}

/// Scan the binary for strings matching `search` (case-insensitive substring,
/// empty = all strings) and find every code location that references each
/// string's virtual address via a direct memory/immediate operand.
///
/// Strategy:
///  1. Scan every section with `binary.get_bytes(va, size)` looking for runs of
///     printable ASCII of at least `min_length`.  Record string_va → content.
///  2. Decode every executable section with iced-x86.  For each instruction
///     check RIP-relative, memory-displacement, and immediate operands for a
///     match against the string VA map.
///  3. Return at most 2 000 results sorted by descending reference count.
#[tauri::command]
pub async fn get_string_xrefs(
    search: String,
    min_length: usize,
    state: State<'_, AppState>,
) -> CmdResult<Vec<StringXrefDto>> {
    use iced_x86::{Decoder, DecoderOptions, OpKind};
    use std::collections::HashMap;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let min_len = min_length.max(2);
    let search_lc = search.to_lowercase();

    // ── Step 1: collect strings from all sections ────────────────────────────
    let mut string_map: HashMap<u64, String> = HashMap::new();
    for section in &binary.sections {
        let Some(bytes) = binary.get_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            continue;
        };

        // ASCII scan
        let mut c_start: Option<usize> = None;
        let mut acc: Vec<u8> = Vec::new();
        for (i, &byte) in bytes.iter().enumerate() {
            if (0x20..0x7f).contains(&byte) {
                if c_start.is_none() {
                    c_start = Some(i);
                }
                acc.push(byte);
            } else {
                if acc.len() >= min_len {
                    if let (Some(off), Ok(s)) = (c_start, std::str::from_utf8(&acc)) {
                        let va = section.virtual_address + off as u64;
                        let content = s.to_string();
                        if search_lc.is_empty() || content.to_lowercase().contains(&search_lc) {
                            string_map.insert(va, content);
                        }
                    }
                }
                c_start = None;
                acc.clear();
            }
            if string_map.len() >= 5_000 {
                break;
            }
        }
        // flush trailing run
        if acc.len() >= min_len {
            if let (Some(off), Ok(s)) = (c_start, std::str::from_utf8(&acc)) {
                let va = section.virtual_address + off as u64;
                let content = s.to_string();
                if search_lc.is_empty() || content.to_lowercase().contains(&search_lc) {
                    string_map.insert(va, content);
                }
            }
        }

        // UTF-16 LE scan
        if string_map.len() < 5_000 {
            let mut i = 0usize;
            while i + 1 < bytes.len() && string_map.len() < 5_000 {
                if bytes[i] >= 0x20 && bytes[i] < 0x7f && bytes[i + 1] == 0x00 {
                    let start = i;
                    let mut chars: Vec<char> = Vec::new();
                    while i + 1 < bytes.len()
                        && bytes[i] >= 0x20
                        && bytes[i] < 0x7f
                        && bytes[i + 1] == 0x00
                    {
                        chars.push(bytes[i] as char);
                        i += 2;
                    }
                    if chars.len() >= min_len {
                        let content: String = chars.into_iter().collect();
                        if search_lc.is_empty() || content.to_lowercase().contains(&search_lc) {
                            let va = section.virtual_address + start as u64;
                            string_map.insert(va, content);
                        }
                    }
                } else {
                    i += 1;
                }
            }
        }
    }

    if string_map.is_empty() {
        return Ok(Vec::new());
    }

    // ── Step 2: scan executable sections for references ──────────────────────
    let mut callsites: HashMap<u64, Vec<StringXrefCallsiteDto>> = HashMap::new();
    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };

    for section in &binary.sections {
        if !section.is_executable {
            continue;
        }
        let Some(bytes) = binary.get_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            continue;
        };

        let mut decoder = Decoder::with_ip(
            bitness,
            &bytes,
            section.virtual_address,
            DecoderOptions::NONE,
        );

        while decoder.can_decode() {
            let insn = decoder.decode();
            if insn.is_invalid() {
                break;
            }
            let ip = insn.ip();
            let mut addrs: Vec<u64> = Vec::new();

            // RIP-relative memory (x64 primary case: lea rax, [rip+offset])
            if insn.is_ip_rel_memory_operand() {
                addrs.push(insn.ip_rel_memory_address());
            }

            for op_i in 0..insn.op_count() {
                match insn.op_kind(op_i) {
                    OpKind::Memory => {
                        let disp = insn.memory_displacement64();
                        if disp > binary.image_base {
                            addrs.push(disp);
                        }
                    }
                    OpKind::Immediate32 | OpKind::Immediate32to64 => {
                        let imm = insn.immediate32() as u64;
                        if imm > binary.image_base {
                            addrs.push(imm);
                        }
                    }
                    OpKind::Immediate64 => {
                        let imm = insn.immediate64();
                        if imm > binary.image_base {
                            addrs.push(imm);
                        }
                    }
                    _ => {}
                }
            }

            for addr in addrs {
                if string_map.contains_key(&addr) {
                    let func_name = binary.function_at(ip).map(|f| {
                        inner
                            .renamed_functions
                            .get(&f.address)
                            .cloned()
                            .unwrap_or_else(|| f.name.clone())
                    });
                    callsites
                        .entry(addr)
                        .or_default()
                        .push(StringXrefCallsiteDto {
                            from_address: format!("0x{:x}", ip),
                            from_function: func_name,
                        });
                }
            }
        }
    }

    // ── Step 3: build result list ────────────────────────────────────────────
    let mut results: Vec<StringXrefDto> = string_map
        .iter()
        .map(|(addr, content)| StringXrefDto {
            string_address: format!("0x{:x}", addr),
            string_value: content.clone(),
            refs: callsites.get(addr).cloned().unwrap_or_default(),
        })
        .collect();

    // Sort: most referenced first, then by address
    results.sort_by(|a, b| {
        b.refs
            .len()
            .cmp(&a.refs.len())
            .then(a.string_address.cmp(&b.string_address))
    });
    results.truncate(2_000);
    Ok(results)
}
