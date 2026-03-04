//! Hex view, byte patching, and binary saving.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::MAX_HEX_READ;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Get hex view data starting at an address.
#[tauri::command]
pub async fn get_hex_view(
    address: u64,
    length: usize,
    state: State<'_, AppState>,
) -> CmdResult<HexViewData> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let total_size = binary
        .sections
        .iter()
        .map(|s| s.virtual_address + s.virtual_size)
        .max()
        .unwrap_or(0);

    // Clamp length to avoid excessive memory
    let actual_len = length.min(MAX_HEX_READ);
    let bytes = binary.get_bytes(address, actual_len).unwrap_or_default();

    let mut rows = Vec::new();
    for chunk_start in (0..bytes.len()).step_by(16) {
        let chunk_end = (chunk_start + 16).min(bytes.len());
        let chunk = &bytes[chunk_start..chunk_end];

        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();

        rows.push(HexRow {
            offset: format!("0x{:08x}", address + chunk_start as u64),
            hex,
            ascii,
        });
    }

    Ok(HexViewData { rows, total_size })
}

/// Patch bytes at a virtual address.
/// Returns the original bytes that were replaced.
#[tauri::command]
pub async fn patch_bytes(
    address: u64,
    bytes: Vec<u8>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<u8>> {
    let mut inner = state.inner.lock().await;
    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    let original = binary.patch_bytes_va(address, &bytes).ok_or_else(|| {
        CmdError::other(format!(
            "Patch failed: address 0x{:x} out of range",
            address
        ))
    })?;

    inner.loaded_binary = Some(std::sync::Arc::new(binary));
    Ok(original)
}

/// Save the (potentially patched) binary to a new file path.
#[tauri::command]
pub async fn save_patched_binary(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    binary
        .save_as(&path)
        .map_err(|e| CmdError::other(format!("Save failed: {e}")))
}
