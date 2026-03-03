//! Listing view — disassembly listing with labels, instructions, and comments.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::parse_address;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Return metadata for the listing view (entry point, address range, total executable bytes).
#[tauri::command]
pub async fn get_listing_info(state: State<'_, AppState>) -> CmdResult<ListingInfo> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let exec_sections = binary.executable_sections();
    if exec_sections.is_empty() {
        return Err(CmdError::other("No executable sections found"));
    }

    let first_addr = exec_sections
        .iter()
        .map(|s| s.virtual_address)
        .min()
        .unwrap_or(binary.image_base);
    let last_addr = exec_sections
        .iter()
        .map(|s| s.virtual_address + s.virtual_size)
        .max()
        .unwrap_or(first_addr);
    let total_exec_bytes: u64 = exec_sections.iter().map(|s| s.virtual_size).sum();

    Ok(ListingInfo {
        entry_point: format!("0x{:x}", binary.entry_point),
        first_addr: format!("0x{:x}", first_addr),
        last_addr: format!("0x{:x}", last_addr),
        total_exec_bytes,
    })
}

/// Decode up to `count` instructions starting from `start_address`.
/// Returns a flat list of `ListingRow` values that may include label rows
/// (function entry points) before their first instruction row.
#[tauri::command]
pub async fn get_listing_chunk(
    start_address: String,
    count: usize,
    state: State<'_, AppState>,
) -> CmdResult<Vec<ListingRow>> {
    use iced_x86::{Decoder, DecoderOptions, FlowControl, Formatter, IntelFormatter};
    use std::collections::HashMap;

    let start_address = parse_address(&start_address)
        .ok_or_else(|| CmdError::other(format!("Invalid address: {}", start_address)))?;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    // Build function address → display name map
    let func_names: HashMap<u64, String> = binary
        .functions
        .iter()
        .map(|f| {
            let name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            (f.address, name)
        })
        .collect();

    // Find which executable section contains start_address (or the nearest after it)
    let section = binary
        .sections
        .iter()
        .filter(|s| s.is_executable)
        .find(|s| {
            start_address >= s.virtual_address
                && start_address < s.virtual_address + s.virtual_size
        })
        .or_else(|| {
            // Pick the first executable section that starts after start_address
            binary
                .sections
                .iter()
                .filter(|s| s.is_executable && s.virtual_address >= start_address)
                .min_by_key(|s| s.virtual_address)
        })
        .ok_or_else(|| {
            CmdError::other(format!(
                "No executable section covers 0x{:x}",
                start_address
            ))
        })?;

    let effective_start = start_address.max(section.virtual_address);
    let section_end = section.virtual_address + section.virtual_size;
    let decode_size = (section_end - effective_start) as usize;

    let bytes = binary
        .get_bytes(effective_start, decode_size)
        .ok_or_else(|| {
            CmdError::other(format!(
                "Cannot read bytes at 0x{:x}",
                effective_start
            ))
        })?;

    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, &bytes, effective_start, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();

    let max_count = count.min(500); // safety cap
    let mut rows: Vec<ListingRow> = Vec::with_capacity(max_count + 10);
    let mut insn_count = 0;

    while decoder.can_decode() && insn_count < max_count {
        let insn = decoder.decode();
        if insn.is_invalid() {
            break;
        }

        let ip = insn.ip();

        // Insert a label row if a function starts here
        if let Some(name) = func_names.get(&ip) {
            rows.push(ListingRow {
                address: format!("0x{:x}", ip),
                bytes: String::new(),
                mnemonic: String::new(),
                operands: String::new(),
                label: Some(name.clone()),
                comment: None,
                row_type: "label".to_string(),
                mnemonic_type: String::new(),
            });
        }

        let start = (ip - effective_start) as usize;
        let end = start + insn.len();
        let hex_bytes: String = bytes[start..end]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        let mut out = String::new();
        formatter.format(&insn, &mut out);
        let parts: Vec<&str> = out.splitn(2, ' ').collect();
        let mnemonic = parts.first().unwrap_or(&"").to_string();
        let operands = parts.get(1).unwrap_or(&"").to_string();

        let comment = inner.comments.get(&ip).cloned();

        // Classify mnemonic for syntax highlighting (x64dbg-style categories)
        let mnemonic_type = match insn.flow_control() {
            FlowControl::Call | FlowControl::IndirectCall => "call",
            FlowControl::UnconditionalBranch | FlowControl::IndirectBranch => "jmp",
            FlowControl::ConditionalBranch => "cjmp",
            FlowControl::Return => "ret",
            FlowControl::Interrupt => "int",
            _ => {
                let m = mnemonic.as_str();
                if m == "nop" || m.starts_with("nop") {
                    "nop"
                } else if m == "push" || m == "pop" || m == "pusha" || m == "popa"
                    || m == "pushf" || m == "popf" || m == "pushfq" || m == "popfq"
                {
                    "push_pop"
                } else if m.starts_with("mov") || m == "lea" || m == "xchg" {
                    "mov"
                } else if m == "cmp" || m == "test" {
                    "cmp"
                } else {
                    "normal"
                }
            }
        };

        rows.push(ListingRow {
            address: format!("0x{:x}", ip),
            bytes: hex_bytes,
            mnemonic,
            operands,
            label: None,
            comment,
            row_type: "instruction".to_string(),
            mnemonic_type: mnemonic_type.to_string(),
        });

        insn_count += 1;
    }

    Ok(rows)
}
