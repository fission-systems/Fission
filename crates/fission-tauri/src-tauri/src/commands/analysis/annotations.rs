//! User annotations — renames, comments, bookmarks, and navigation.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::parse_address;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Rename a function at the given address.
#[tauri::command]
pub async fn rename_function(
    address: u64,
    new_name: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let mut inner = state.inner.lock().await;

    // Verify the binary is loaded
    if inner.loaded_binary.is_none() {
        return Err(CmdError::other("No binary loaded"));
    }

    if new_name.trim().is_empty() {
        // Remove the rename (revert to original)
        inner.renamed_functions.remove(&address);
        inner.manual_renamed_functions.remove(&address);
        inner.auto_renamed_functions.remove(&address);
    } else {
        inner
            .renamed_functions
            .insert(address, new_name.trim().to_string());
        inner.manual_renamed_functions.insert(address);
        inner.auto_renamed_functions.remove(&address);
    }

    Ok(())
}

/// Add or update a comment at the given address.
#[tauri::command]
pub async fn add_comment(address: u64, text: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut inner = state.inner.lock().await;

    if text.trim().is_empty() {
        inner.comments.remove(&address);
    } else {
        inner.comments.insert(address, text.trim().to_string());
    }

    Ok(())
}

/// Get all comments.
#[tauri::command]
pub async fn get_comments(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, String>> {
    let inner = state.inner.lock().await;
    let comments = inner
        .comments
        .iter()
        .map(|(addr, text)| (format!("0x{:x}", addr), text.clone()))
        .collect();
    Ok(comments)
}

/// Toggle a bookmark at the given address.
#[tauri::command]
pub async fn toggle_bookmark(
    address: String,
    label: String,
    state: State<'_, AppState>,
) -> CmdResult<bool> {
    let mut inner = state.inner.lock().await;

    // Check if bookmark already exists
    if let Some(pos) = inner.bookmarks.iter().position(|b| b.address == address) {
        inner.bookmarks.remove(pos);
        Ok(false) // removed
    } else {
        let func_name = parse_address(&address).and_then(|addr| {
            inner
                .loaded_binary
                .as_ref()
                .and_then(|b| b.function_at(addr))
                .map(|f| {
                    inner
                        .renamed_functions
                        .get(&addr)
                        .cloned()
                        .unwrap_or_else(|| f.name.clone())
                })
        });

        inner.bookmarks.push(BookmarkDto {
            address,
            label,
            function_name: func_name,
        });
        Ok(true) // added
    }
}

/// Get all bookmarks.
#[tauri::command]
pub async fn get_bookmarks(state: State<'_, AppState>) -> CmdResult<Vec<BookmarkDto>> {
    let inner = state.inner.lock().await;
    Ok(inner.bookmarks.clone())
}

/// Resolve a goto input (hex address or symbol name) to a concrete address.
#[tauri::command]
pub async fn goto_address(input: String, state: State<'_, AppState>) -> CmdResult<GotoResult> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let trimmed = input.trim();

    // Try parsing as hex address
    if let Some(addr) = parse_address(trimmed) {
        let func_name = binary.function_at(addr).map(|f| {
            inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone())
        });

        return Ok(GotoResult {
            address: format!("0x{:x}", addr),
            function_name: func_name,
            found: true,
        });
    }

    // Try matching by symbol name (original or renamed)
    // First check renamed functions
    for (addr, name) in &inner.renamed_functions {
        if name.eq_ignore_ascii_case(trimmed) {
            return Ok(GotoResult {
                address: format!("0x{:x}", addr),
                function_name: Some(name.clone()),
                found: true,
            });
        }
    }

    // Then check original function names
    for f in &binary.functions {
        if f.name.eq_ignore_ascii_case(trimmed) {
            let display = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());

            return Ok(GotoResult {
                address: format!("0x{:x}", f.address),
                function_name: Some(display),
                found: true,
            });
        }
    }

    Ok(GotoResult {
        address: String::new(),
        function_name: None,
        found: false,
    })
}
