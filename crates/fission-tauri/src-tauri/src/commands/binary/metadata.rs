//! Binary metadata — strings, imports, exports, sections.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::UNKNOWN_LIBRARY;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

/// Get extracted strings from the loaded binary.
#[tauri::command]
pub async fn get_strings(state: State<'_, AppState>) -> CmdResult<Vec<StringDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let data = binary.inner().data.as_slice();
    let mut strings = Vec::new();
    let min_len = fission_core::MIN_STRING_LENGTH;

    // ── ASCII pass ───────────────────────────────────────────────────────────
    let mut current_start = None;
    let mut current_str = Vec::new();

    for (i, &byte) in data.iter().enumerate() {
        if byte >= 0x20 && byte < 0x7f {
            if current_start.is_none() {
                current_start = Some(i);
            }
            current_str.push(byte);
        } else {
            if current_str.len() >= min_len {
                if let (Some(start), Ok(s)) = (current_start, std::str::from_utf8(&current_str)) {
                    strings.push(StringDto {
                        offset: format!("0x{:x}", start),
                        value: s.to_string(),
                        encoding: "ASCII".to_string(),
                    });
                }
            }
            current_start = None;
            current_str.clear();
        }

        if strings.len() >= 10000 {
            break;
        }
    }

    // ── UTF-16 LE pass ───────────────────────────────────────────────────────
    if strings.len() < 10000 {
        let mut i = 0usize;
        while i + 1 < data.len() && strings.len() < 10000 {
            if data[i] >= 0x20 && data[i] < 0x7f && data[i + 1] == 0x00 {
                let start = i;
                let mut chars: Vec<char> = Vec::new();
                while i + 1 < data.len() && data[i] >= 0x20 && data[i] < 0x7f && data[i + 1] == 0x00
                {
                    chars.push(data[i] as char);
                    i += 2;
                }
                if chars.len() >= min_len {
                    strings.push(StringDto {
                        offset: format!("0x{:x}", start),
                        value: chars.into_iter().collect(),
                        encoding: "UTF-16 LE".to_string(),
                    });
                }
            } else {
                i += 1;
            }
        }
    }

    // Sort by offset for consistent ordering
    strings.sort_by(|a, b| {
        a.offset
            .len()
            .cmp(&b.offset.len())
            .then(a.offset.cmp(&b.offset))
    });

    Ok(strings)
}

/// Get import table entries.
#[tauri::command]
pub async fn get_imports(state: State<'_, AppState>) -> CmdResult<Vec<ImportDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let imports: Vec<ImportDto> = binary
        .imports()
        .map(|f| {
            let display_name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());

            // Try to extract library name from the function name (e.g., "KERNEL32.dll!CreateFileW")
            let (library, name) = if let Some(idx) = display_name.find('!') {
                (
                    display_name[..idx].to_string(),
                    display_name[idx + 1..].to_string(),
                )
            } else {
                (UNKNOWN_LIBRARY.to_string(), display_name)
            };

            ImportDto {
                address: format!("0x{:x}", f.address),
                name,
                library,
                ordinal: None,
            }
        })
        .collect();

    Ok(imports)
}

/// Get export table entries (functions flagged as export).
#[tauri::command]
pub async fn get_exports(state: State<'_, AppState>) -> CmdResult<Vec<ExportDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let exports: Vec<ExportDto> = binary
        .exports()
        .map(|f| {
            let display_name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            ExportDto {
                address: format!("0x{:x}", f.address),
                name: display_name,
                ordinal: None,
                forwarder: None,
            }
        })
        .collect();

    Ok(exports)
}

/// Get section information.
#[tauri::command]
pub async fn get_sections(state: State<'_, AppState>) -> CmdResult<Vec<SectionDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let sections: Vec<SectionDto> = binary
        .sections
        .iter()
        .map(|s| {
            let mut flags = Vec::new();
            if s.is_executable {
                flags.push("X");
            }
            if s.is_writable {
                flags.push("W");
            }
            if s.is_readable {
                flags.push("R");
            }

            SectionDto {
                name: s.name.clone(),
                address: format!("0x{:x}", s.virtual_address),
                size: s.virtual_size,
                flags: flags.join(""),
            }
        })
        .collect();

    Ok(sections)
}
