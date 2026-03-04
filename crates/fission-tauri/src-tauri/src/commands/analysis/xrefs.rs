//! Cross-reference analysis — callers and callees of a function.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::{MAX_XREF_DECODE, MAX_XREF_INCOMING, MAX_XREF_OUTGOING};
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Get cross-references for an address.
/// Scans all executable sections with iced-x86 to find CALL/JMP/Jcc instructions
/// that target `address`, and returns both incoming (callers) and outgoing refs.
#[tauri::command]
pub async fn get_xrefs(address: u64, state: State<'_, AppState>) -> CmdResult<Vec<XrefDto>> {
    use iced_x86::{Decoder, DecoderOptions, FlowControl};

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };
    let mut results: Vec<XrefDto> = Vec::new();

    for section in binary.executable_sections() {
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

            let target: u64 = match insn.flow_control() {
                FlowControl::Call
                | FlowControl::UnconditionalBranch
                | FlowControl::ConditionalBranch => insn.near_branch_target(),
                _ => 0,
            };

            if target != address {
                continue;
            }

            let from_addr = insn.ip();

            // Resolve enclosing function name
            let from_function = binary
                .functions
                .iter()
                .find(|f| {
                    from_addr >= f.address && from_addr < f.address.saturating_add(f.size.max(1))
                })
                .map(|f| {
                    inner
                        .renamed_functions
                        .get(&f.address)
                        .cloned()
                        .unwrap_or_else(|| f.name.clone())
                });

            let xref_type = match insn.flow_control() {
                FlowControl::Call => "call",
                FlowControl::ConditionalBranch => "jcc",
                _ => "jmp",
            };

            results.push(XrefDto {
                from_address: format!("0x{:x}", from_addr),
                to_address: format!("0x{:x}", address),
                xref_type: xref_type.to_string(),
                from_function,
            });

            if results.len() >= MAX_XREF_INCOMING {
                break;
            }
        }
    }

    // Also find outgoing refs FROM this address (what does this function call?)
    let func = binary.functions.iter().find(|f| f.address == address);
    if let Some(func) = func {
        let size = if func.size > 0 {
            func.size as usize
        } else {
            256
        };
        if let Some(bytes) = binary.get_bytes(address, size.min(MAX_XREF_DECODE)) {
            let mut decoder = Decoder::with_ip(bitness, &bytes, address, DecoderOptions::NONE);
            while decoder.can_decode() {
                let insn = decoder.decode();
                if insn.is_invalid() {
                    break;
                }
                let target: u64 = match insn.flow_control() {
                    FlowControl::Call
                    | FlowControl::UnconditionalBranch
                    | FlowControl::ConditionalBranch => insn.near_branch_target(),
                    _ => 0,
                };
                if target == 0 || target == address {
                    continue;
                }

                let to_function = binary
                    .functions
                    .iter()
                    .find(|f| f.address == target)
                    .map(|f| {
                        inner
                            .renamed_functions
                            .get(&f.address)
                            .cloned()
                            .unwrap_or_else(|| f.name.clone())
                    });

                let xref_type = match insn.flow_control() {
                    FlowControl::Call => "call",
                    FlowControl::ConditionalBranch => "jcc",
                    _ => "jmp",
                };

                results.push(XrefDto {
                    from_address: format!("0x{:x}", insn.ip()),
                    to_address: format!("0x{:x}", target),
                    xref_type: xref_type.to_string(),
                    from_function: to_function,
                });

                if results.len() >= MAX_XREF_OUTGOING {
                    break;
                }
            }
        }
    }

    Ok(results)
}
