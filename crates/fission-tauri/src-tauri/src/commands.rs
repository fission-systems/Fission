//! Fission Tauri — Tauri command handlers.
//!
//! Each `#[tauri::command]` function bridges the frontend (React) to the
//! backend (fission-loader / fission-analysis).

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::{find_sla_dir, format_addr, parse_address,
    MAX_HEX_READ, MAX_SCAN_PER_SECTION, MAX_XREF_DECODE, MAX_XREF_INCOMING,
    MAX_XREF_OUTGOING, SETTINGS_FILENAME, UNKNOWN_LIBRARY};
use tracing::{error, warn};
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use tauri::State;
use tauri::Manager as _;

// ============================================================================
// File / Binary Operations
// ============================================================================

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> CmdResult<BinaryInfo> {
    let binary = tokio::task::spawn_blocking(move || {
        let mut binary = LoadedBinary::from_file(&path)
            .map_err(|e| CmdError::other(format!("Failed to load binary: {e}")))?;
        // Automatic multi-pass function discovery (runs in the worker thread)
        binary.discover_internal_functions();    // Pass 1: CALL target scan
        binary.discover_functions_by_prologue(); // Pass 2: prologue pattern scan
        Ok::<LoadedBinary, CmdError>(binary)
    })
    .await
    .map_err(|e| CmdError::other(format!("Task failed: {e}")))?
    ?;

    let info = binary_to_info(&binary);

    let binary_arc = Arc::new(binary);

    // Initialize decompiler if native_decomp feature is enabled
    #[cfg(feature = "native_decomp")]
    {
        let sla_dir = find_sla_dir();
        match fission_analysis::analysis::decomp::CachingDecompiler::new(
            &binary_arc,
            &sla_dir,
            200,
        ) {
            Ok(mut decomp) => {
                let bin_ref = binary_arc.clone();
                let sleigh_id = bin_ref.arch_spec.clone();
                let compiler_id = bin_ref.get_ghidra_compiler_id();
                if let Err(e) = decomp.inner_mut().load_binary(
                    bin_ref.data.as_slice(),
                    bin_ref.image_base,
                    bin_ref.is_64bit,
                    Some(sleigh_id.as_str()),
                    compiler_id.as_deref(),
                ) {
                    warn!(error = %e, "failed to load binary into decompiler");
                } else {
                    let inner_decomp = decomp.inner_mut();

                    // Register PE sections so Ghidra can map virtual addresses to file data
                    for section in &bin_ref.sections {
                        let _ = inner_decomp.add_memory_block(
                            &section.name,
                            section.virtual_address,
                            section.virtual_size,
                            section.file_offset,
                            section.file_size,
                            section.is_executable,
                            section.is_writable,
                        );
                    }

                    // Register IAT symbols (import functions)
                    let iat_symbols: std::collections::HashMap<u64, String> = bin_ref
                        .imports()
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    inner_decomp.add_symbols(&iat_symbols);

                    // Register all functions as global symbols
                    let global_symbols: std::collections::HashMap<u64, String> = bin_ref
                        .functions
                        .iter()
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    inner_decomp.add_global_symbols(&global_symbols);

                    // Add function entries so Ghidra knows about them
                    for func in &bin_ref.functions {
                        let _ = inner_decomp.add_function(func.address, Some(&func.name));
                    }

                    // Store decompiler in its own separate Mutex
                    let mut decomp_lock = state.decompiler.lock().await;
                    *decomp_lock = Some(decomp);
                    drop(decomp_lock);
                    let mut inner = state.inner.lock().await;
                    inner.decompiler_loaded = true;
                }
            }
            Err(e) => {
                error!(error = %e, "failed to initialize decompiler");
            }
        }
    }

    // Store the binary and reset user state
    let mut inner = state.inner.lock().await;
    inner.loaded_binary = Some(binary_arc);
    inner.comments.clear();
    inner.renamed_functions.clear();
    inner.bookmarks.clear();

    Ok(info)
}

/// Get all functions from the loaded binary.
#[tauri::command]
pub async fn get_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let functions = functions_to_dtos(binary, &inner.renamed_functions);

    Ok(functions)
}

/// Get info about the currently loaded binary.
#[tauri::command]
pub async fn get_binary_info(state: State<'_, AppState>) -> CmdResult<Option<BinaryInfo>> {
    let inner = state.inner.lock().await;
    let info = inner.loaded_binary.as_ref().map(|b| binary_to_info(b));
    Ok(info)
}

// ============================================================================
// Decompile / Assembly
// ============================================================================

/// Decompile a function at the given address.
#[tauri::command]
pub async fn decompile_function(
    address: u64,
    state: State<'_, AppState>,
) -> CmdResult<DecompileResult> {
    // Step 1: Grab func_name from the inner lock, then DROP it immediately
    let func_name = {
        let inner = state.inner.lock().await;
        inner
            .renamed_functions
            .get(&address)
            .cloned()
            .or_else(|| {
                inner
                    .loaded_binary
                    .as_ref()
                    .and_then(|b| b.function_at(address))
                    .map(|f| f.name.clone())
            })
            .unwrap_or_else(|| format!("sub_{:x}", address))
    };
    // inner lock is now released — other commands can proceed

    // Step 2: Decompile using the SEPARATE decompiler Mutex
    #[cfg(feature = "native_decomp")]
    {
        let mut decomp_lock = state.decompiler.lock().await;
        let decomp_result = decomp_lock
            .as_mut()
            .ok_or_else(|| CmdError::other("Decompiler not initialized"))
            .and_then(|decomp| decomp.decompile(address).map_err(|e| CmdError::other(format!("{e}"))));
        drop(decomp_lock);

        return match decomp_result {
            Ok(code) => Ok(DecompileResult {
                code,
                function_name: func_name,
                address: format!("0x{:x}", address),
            }),
            Err(e) => Ok(DecompileResult {
                code: format!("// Decompilation failed: {}\n// Function: {}\n// Address: 0x{:x}\n", e, func_name, address),
                function_name: func_name,
                address: format!("0x{:x}", address),
            }),
        };
    }

    #[cfg(not(feature = "native_decomp"))]
    {
        Ok(DecompileResult {
            code: format!(
                "// Native decompiler not available\n// Function: {}\n// Address: 0x{:x}\n",
                func_name, address
            ),
            function_name: func_name,
            address: format!("0x{:x}", address),
        })
    }
}

/// Get disassembled instructions at an address.
#[tauri::command]
pub async fn get_assembly(
    address: u64,
    count: usize,
    state: State<'_, AppState>,
) -> CmdResult<Vec<AsmInstructionDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let byte_count = count * 15;
    let bytes = binary
        .get_bytes(address, byte_count)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", address)))?;

    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};

    let bitness = if binary.is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, &bytes, address, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut output = String::new();
    let mut instructions = Vec::with_capacity(count);

    let mut i = 0;
    while decoder.can_decode() && i < count {
        let insn = decoder.decode();
        output.clear();
        formatter.format(&insn, &mut output);

        let start = (insn.ip() - address) as usize;
        let end = start + insn.len();
        let hex_bytes: String = bytes[start..end]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        let parts: Vec<&str> = output.splitn(2, ' ').collect();
        let mnemonic = parts.first().unwrap_or(&"").to_string();
        let operands = parts.get(1).unwrap_or(&"").to_string();

        let comment = inner.comments.get(&insn.ip()).cloned();

        instructions.push(AsmInstructionDto {
            address: format!("0x{:x}", insn.ip()),
            bytes: hex_bytes,
            mnemonic,
            operands,
            comment,
        });

        i += 1;
    }

    Ok(instructions)
}

// ============================================================================
// Strings / Imports / Sections
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
    let min_len = 4;

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
                if let (Some(start), Ok(s)) =
                    (current_start, std::str::from_utf8(&current_str))
                {
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
                while i + 1 < data.len()
                    && data[i] >= 0x20
                    && data[i] < 0x7f
                    && data[i + 1] == 0x00
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
    strings.sort_by(|a, b| a.offset.len().cmp(&b.offset.len()).then(a.offset.cmp(&b.offset)));

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
            if s.is_executable { flags.push("X"); }
            if s.is_writable { flags.push("W"); }
            if s.is_readable { flags.push("R"); }

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

// ============================================================================
// Rename / Comment / Bookmark
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
    } else {
        inner.renamed_functions.insert(address, new_name.trim().to_string());
    }

    Ok(())
}

/// Add or update a comment at the given address.
#[tauri::command]
pub async fn add_comment(
    address: u64,
    text: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
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

// ============================================================================
// Navigation
// ============================================================================

/// Resolve a goto input (hex address or symbol name) to a concrete address.
#[tauri::command]
pub async fn goto_address(
    input: String,
    state: State<'_, AppState>,
) -> CmdResult<GotoResult> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let trimmed = input.trim();

    // Try parsing as hex address
    if let Some(addr) = parse_address(trimmed) {
        let func_name = binary
            .function_at(addr)
            .map(|f| {
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

// ============================================================================
// Hex View / Search / Xrefs (Phase 2)
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
    let bytes = binary
        .get_bytes(address, actual_len)
        .unwrap_or_default();

    let mut rows = Vec::new();
    for chunk_start in (0..bytes.len()).step_by(16) {
        let chunk_end = (chunk_start + 16).min(bytes.len());
        let chunk = &bytes[chunk_start..chunk_end];

        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if b >= 0x20 && b < 0x7f { b as char } else { '.' })
            .collect();

        rows.push(HexRow {
            offset: format!("0x{:08x}", address + chunk_start as u64),
            hex,
            ascii,
        });
    }

    Ok(HexViewData { rows, total_size })
}

// ============================================================================
// Hex Patch (Phase 8)
// ============================================================================

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

    let original = binary
        .patch_bytes_va(address, &bytes)
        .ok_or_else(|| CmdError::other(format!("Patch failed: address 0x{:x} out of range", address)))?;

    inner.loaded_binary = Some(std::sync::Arc::new(binary));
    Ok(original)
}

/// Save the (potentially patched) binary to a new file path.
#[tauri::command]
pub async fn save_patched_binary(
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    binary
        .save_as(&path)
        .map_err(|e| CmdError::other(format!("Save failed: {e}")))
}

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
            if results.len() >= limit { break; }
        }
    }

    // Search strings in readable sections (using view_bytes)
    if results.len() < limit {
        let min_len = 4;
        for section in &binary.sections {
            if results.len() >= limit { break; }
            // Skip empty or tiny sections
            if section.virtual_size < min_len as u64 { continue; }
            // Limit per-section scan to MAX_SCAN_PER_SECTION
            let scan_len = (section.virtual_size as usize).min(MAX_SCAN_PER_SECTION);
            if let Some(data) = binary.view_bytes(section.virtual_address, scan_len) {
                let mut current_start: Option<usize> = None;
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
                                if s.to_lowercase().contains(&q) {
                                    let addr = section.virtual_address + start as u64;
                                    results.push(SearchResultDto {
                                        address: format!("0x{:x}", addr),
                                        name: s.to_string(),
                                        result_type: "string".to_string(),
                                        context: format!("section: {}, len: {}", section.name, s.len()),
                                    });
                                    if results.len() >= limit { break; }
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
            let func_name = binary
                .function_at(addr)
                .map(|f| {
                    inner.renamed_functions
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

/// Get cross-references for an address.
/// Scans all executable sections with iced-x86 to find CALL/JMP/Jcc instructions
/// that target `address`, and returns both incoming (callers) and outgoing refs.
#[tauri::command]
pub async fn get_xrefs(
    address: u64,
    state: State<'_, AppState>,
) -> CmdResult<Vec<XrefDto>> {
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
        else { continue };

        let mut decoder = Decoder::with_ip(
            bitness, &bytes, section.virtual_address, DecoderOptions::NONE
        );

        while decoder.can_decode() {
            let insn = decoder.decode();
            if insn.is_invalid() { break; }

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
            let from_function = binary.functions.iter().find(|f| {
                from_addr >= f.address
                    && from_addr < f.address.saturating_add(f.size.max(1))
            }).map(|f| {
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
        let size = if func.size > 0 { func.size as usize } else { 256 };
        if let Some(bytes) = binary.get_bytes(address, size.min(MAX_XREF_DECODE)) {
            let mut decoder = Decoder::with_ip(bitness, &bytes, address, DecoderOptions::NONE);
            while decoder.can_decode() {
                let insn = decoder.decode();
                if insn.is_invalid() { break; }
                let target: u64 = match insn.flow_control() {
                    FlowControl::Call
                    | FlowControl::UnconditionalBranch
                    | FlowControl::ConditionalBranch => insn.near_branch_target(),
                    _ => 0,
                };
                if target == 0 || target == address { continue; }

                let to_function = binary.functions.iter().find(|f| f.address == target).map(|f| {
                    inner.renamed_functions.get(&f.address).cloned().unwrap_or_else(|| f.name.clone())
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

                if results.len() >= MAX_XREF_OUTGOING { break; }
            }
        }
    }

    Ok(results)
}

// ============================================================================
// Listing View
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

    let first_addr = exec_sections.iter().map(|s| s.virtual_address).min().unwrap_or(binary.image_base);
    let last_addr = exec_sections.iter().map(|s| s.virtual_address + s.virtual_size).max().unwrap_or(first_addr);
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
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
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
        .ok_or_else(|| CmdError::other(format!("No executable section covers 0x{:x}", start_address)))?;

    let effective_start = start_address.max(section.virtual_address);
    let section_end = section.virtual_address + section.virtual_size;
    let decode_size = (section_end - effective_start) as usize;

    let bytes = binary
        .get_bytes(effective_start, decode_size)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", effective_start)))?;

    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, &bytes, effective_start, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();

    let max_count = count.min(500); // safety cap
    let mut rows: Vec<ListingRow> = Vec::with_capacity(max_count + 10);
    let mut insn_count = 0;

    while decoder.can_decode() && insn_count < max_count {
        let insn = decoder.decode();
        if insn.is_invalid() { break; }

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

        rows.push(ListingRow {
            address: format!("0x{:x}", ip),
            bytes: hex_bytes,
            mnemonic,
            operands,
            label: None,
            comment,
            row_type: "instruction".to_string(),
        });

        insn_count += 1;
    }

    Ok(rows)
}

// ============================================================================
// Project Save / Load
// ============================================================================

/// Save the current project (user annotations) to a `.fprj` JSON file.
#[tauri::command]
pub async fn save_project(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    // Convert u64 comment keys → hex address strings
    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(addr, text)| (format!("0x{:x}", addr), text.clone()))
        .collect();

    let renames: std::collections::HashMap<String, String> = inner
        .renamed_functions
        .iter()
        .map(|(addr, name)| (format!("0x{:x}", addr), name.clone()))
        .collect();

    let project = crate::dto::FissionProject {
        version: 1,
        binary_path: binary.path.clone(),
        comments,
        renames,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&project)
        .map_err(|e| CmdError::other(format!("Serialise failed: {e}")))?;

    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write failed: {e}")))?;

    Ok(())
}

/// Load a `.fprj` project file.  Restores user annotations from the file.
/// The binary itself must already be (or will be) loaded separately via `open_file`.
/// Returns the recorded binary path so the frontend can reload it if needed.
#[tauri::command]
pub async fn load_project(path: String, state: State<'_, AppState>) -> CmdResult<crate::dto::FissionProject> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CmdError::other(format!("Read failed: {e}")))?;

    let project: crate::dto::FissionProject = serde_json::from_str(&json)
        .map_err(|e| CmdError::other(format!("Parse failed: {e}")))?;

    // Apply user annotations to current state
    let mut inner = state.inner.lock().await;

    // Convert hex address strings back to u64 keys
    inner.comments = project
        .comments
        .iter()
        .filter_map(|(addr_str, text)| {
            parse_address(addr_str).map(|a| (a, text.clone()))
        })
        .collect();

    inner.renamed_functions = project
        .renames
        .iter()
        .filter_map(|(addr_str, name)| {
            parse_address(addr_str).map(|a| (a, name.clone()))
        })
        .collect();

    inner.bookmarks = project.bookmarks.clone();

    Ok(project)
}

// ============================================================================
// Settings
// ============================================================================

/// Path to the settings file inside the OS app-data directory.
fn settings_path(app_handle: &tauri::AppHandle) -> CmdResult<std::path::PathBuf> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| CmdError::other(format!("Cannot resolve app-data dir: {e}")))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| CmdError::other(format!("Cannot create app-data dir: {e}")))?;
    Ok(data_dir.join("settings.json"))
}

/// Load persisted application settings (or defaults if none saved yet).
#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> CmdResult<crate::dto::AppSettings> {
    let path = settings_path(&app_handle)?;
    if !path.exists() {
        return Ok(crate::dto::AppSettings::default());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CmdError::other(format!("Read settings failed: {e}")))?;
    // If schema is corrupt or outdated, fall back to defaults silently
    Ok(serde_json::from_str(&json).unwrap_or_else(|_| {
        warn!(file = SETTINGS_FILENAME, "settings invalid or schema changed, using defaults");
        crate::dto::AppSettings::default()
    }))
}

/// Persist application settings.
#[tauri::command]
pub async fn save_settings(
    settings: crate::dto::AppSettings,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let path = settings_path(&app_handle)?;
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| CmdError::other(format!("Serialise settings failed: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write settings failed: {e}")))?;
    Ok(())
}

/// Clear the in-memory decompiler cache (forces re-decompilation on next request).
/// The actual decompile/asm cache is managed on the frontend; this command
/// serves as a hook for any future server-side cache that may be added.
#[tauri::command]
pub async fn clear_decompiler_cache(_state: State<'_, AppState>) -> CmdResult<()> {
    // Currently the decompile result cache lives in front-end React state.
    // This command is intentionally a no-op on the backend so the front-end
    // can call it and then clear its own cache in response.
    Ok(())
}

// ============================================================================
// CFG Analysis
// ============================================================================

/// Build a Control Flow Graph for the function at `address`.
///
/// Uses iced-x86 to decode the function body, finds basic-block leaders from
/// branch targets and fall-through boundaries, then constructs nodes + edges.
#[tauri::command]
pub async fn get_cfg(
    address: String,
    state: State<'_, AppState>,
) -> CmdResult<CfgDto> {
    use iced_x86::{Decoder, DecoderOptions, FlowControl, Formatter, IntelFormatter};
    use std::collections::{HashMap, HashSet};

    let address = parse_address(&address)
        .ok_or_else(|| CmdError::other(format!("Invalid address: {}", address)))?;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let func = binary
        .functions
        .iter()
        .find(|f| f.address == address)
        .ok_or_else(|| CmdError::other(format!("Function at 0x{:x} not found", address)))?;

    let func_name = inner
        .renamed_functions
        .get(&address)
        .cloned()
        .unwrap_or_else(|| func.name.clone());

    let decode_size = if func.size > 0 { (func.size as usize).min(MAX_XREF_DECODE) } else { MAX_HEX_READ };
    let bytes = binary
        .get_bytes(address, decode_size)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", address)))?;

    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };
    let end_addr = address + bytes.len() as u64;

    // --- Raw instruction record ---
    #[derive(Clone)]
    struct RawInsn {
        ip: u64,
        len: usize,
        text: String,
        flow: FlowControl,
        target: u64,
    }

    let mut formatter = IntelFormatter::new();
    let mut insns: Vec<RawInsn> = Vec::new();
    let mut leaders: HashSet<u64> = HashSet::new();
    leaders.insert(address); // entry is always a leader

    // Pass 1: decode & collect leaders
    {
        let mut decoder = Decoder::with_ip(bitness, &bytes, address, DecoderOptions::NONE);
        while decoder.can_decode() {
            let insn = decoder.decode();
            if insn.is_invalid() { break; }

            let mut out = String::new();
            formatter.format(&insn, &mut out);

            let flow = insn.flow_control();
            let target = insn.near_branch_target();
            let in_range = target >= address && target < end_addr;
            let next_ip = insn.ip() + insn.len() as u64;

            // Branch target is a new leader (if inside the function)
            if in_range && target != 0 {
                leaders.insert(target);
            }

            // The instruction after an unconditional branch / return is a leader
            match flow {
                FlowControl::UnconditionalBranch
                | FlowControl::Return
                | FlowControl::Exception
                | FlowControl::ConditionalBranch => {
                    if next_ip < end_addr {
                        leaders.insert(next_ip);
                    }
                }
                _ => {}
            }

            insns.push(RawInsn { ip: insn.ip(), len: insn.len(), text: out, flow, target });
        }
    }

    // Sorted list of leader addresses
    let mut sorted_leaders: Vec<u64> = leaders.into_iter().filter(|&a| a < end_addr).collect();
    sorted_leaders.sort_unstable();

    // Leader address → block id
    let addr_to_block: HashMap<u64, usize> = sorted_leaders
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i))
        .collect();

    // Initialise nodes
    let mut nodes: Vec<CfgNode> = sorted_leaders
        .iter()
        .enumerate()
        .map(|(i, &leader)| CfgNode {
            id: i,
            start_address: format!("0x{:x}", leader),
            end_address: format!("0x{:x}", leader),
            instructions: Vec::new(),
            is_entry: leader == address,
            is_exit: false,
        })
        .collect();

    let mut edges: Vec<CfgEdge> = Vec::new();
    let mut cur_block: Option<usize> = None;

    // Pass 2: assign instructions → blocks & build edges
    for ri in &insns {
        // Switch block when we hit a new leader
        if let Some(&bid) = addr_to_block.get(&ri.ip) {
            cur_block = Some(bid);
        }
        let Some(bid) = cur_block else { continue };

        let node = &mut nodes[bid];
        let next_ip = ri.ip + ri.len as u64;
        node.end_address = format!("0x{:x}", next_ip);
        node.instructions.push(ri.text.clone());

        match ri.flow {
            FlowControl::Return | FlowControl::Exception => {
                node.is_exit = true;
            }
            FlowControl::UnconditionalBranch => {
                if ri.target >= address && ri.target < end_addr {
                    if let Some(&tid) = addr_to_block.get(&ri.target) {
                        edges.push(CfgEdge { from: bid, to: tid, kind: "unconditional".into() });
                    }
                }
                cur_block = None;
            }
            FlowControl::ConditionalBranch => {
                // True branch (taken)
                if ri.target >= address && ri.target < end_addr {
                    if let Some(&tid) = addr_to_block.get(&ri.target) {
                        edges.push(CfgEdge { from: bid, to: tid, kind: "true".into() });
                    }
                }
                // False branch (fall-through)
                if let Some(&nid) = addr_to_block.get(&next_ip) {
                    edges.push(CfgEdge { from: bid, to: nid, kind: "false".into() });
                }
                cur_block = None;
            }
            _ => {
                // Fall-through: if next ip is a new leader, add implicit edge
                if addr_to_block.contains_key(&next_ip) {
                    if let Some(&nid) = addr_to_block.get(&next_ip) {
                        edges.push(CfgEdge { from: bid, to: nid, kind: "unconditional".into() });
                    }
                    cur_block = None;
                }
            }
        }
    }

    // De-duplicate edges (same from+to+kind can appear from fall-through logic)
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);

    let block_count = nodes.len();
    let edge_count = edges.len();
    // McCabe cyclomatic complexity: V(G) = E – N + 2
    let cyclomatic_complexity = edge_count.saturating_sub(block_count) + 2;

    Ok(CfgDto {
        function_name: func_name,
        function_address: format!("0x{:x}", address),
        nodes,
        edges,
        block_count,
        edge_count,
        cyclomatic_complexity,
    })
}

/// Export the CFG of `address` as a Graphviz DOT string (copied to clipboard on the frontend).
#[tauri::command]
pub async fn export_cfg_dot(
    address: String,
    state: State<'_, AppState>,
) -> CmdResult<String> {
    let cfg = get_cfg(address, state).await?;

    let mut dot = format!(
        "digraph \"{}\" {{\n  rankdir=TB;\n  node [shape=box fontname=\"Courier\" fontsize=10];\n",
        cfg.function_name.replace('"', "'")
    );

    for node in &cfg.nodes {
        let lines: Vec<String> = node
            .instructions
            .iter()
            .map(|i| {
                i.replace('\\', "\\\\").replace('"', "'").replace('<', "\\<").replace('>', "\\>")
            })
            .collect();
        let label = lines.join("\\l");
        let header = node.start_address.replace('"', "'");
        let color = if node.is_entry {
            "lightblue"
        } else if node.is_exit {
            "lightyellow"
        } else {
            "white"
        };
        dot.push_str(&format!(
            "  B{id} [label=\"{h}:\\l{lbl}\\l\" style=filled fillcolor={c}];\n",
            id = node.id,
            h = header,
            lbl = label,
            c = color
        ));
    }

    for edge in &cfg.edges {
        let color = match edge.kind.as_str() {
            "true" => "green",
            "false" => "red",
            _ => "black",
        };
        dot.push_str(&format!(
            "  B{f} -> B{t} [color={c} label=\"{k}\"];\n",
            f = edge.from,
            t = edge.to,
            c = color,
            k = edge.kind
        ));
    }

    dot.push_str("}\n");
    Ok(dot)
}

// ============================================================================
// Helpers
// ============================================================================
// parse_address, format_addr, and find_sla_dir are provided by fission_core.

/// Build a [`BinaryInfo`] DTO from a [`LoadedBinary`].
fn binary_to_info(binary: &fission_loader::loader::LoadedBinary) -> BinaryInfo {
    BinaryInfo {
        name: std::path::Path::new(&binary.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: binary.path.clone(),
        arch: binary.arch_spec.clone(),
        format: binary.format.clone(),
        entry_point: format_addr(binary.entry_point),
        section_count: binary.sections.len(),
        function_count: binary.functions.len(),
        image_base: format_addr(binary.image_base),
    }
}

/// Return the category string for a function.
fn function_category(f: &fission_loader::loader::FunctionInfo) -> &'static str {
    if f.is_import {
        "import"
    } else if f.is_export {
        "export"
    } else {
        "internal"
    }
}

/// Map every function in `binary` to a [`FunctionDto`], applying any renames.
fn functions_to_dtos(
    binary: &fission_loader::loader::LoadedBinary,
    renames: &std::collections::HashMap<u64, String>,
) -> Vec<FunctionDto> {
    binary
        .functions
        .iter()
        .map(|f| {
            let display_name = renames
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            FunctionDto {
                address: format_addr(f.address),
                name: display_name,
                size: f.size,
                category: function_category(f).to_string(),
            }
        })
        .collect()
}

// ============================================================================
// Debug
// ============================================================================

// ── Windows-only: event-drain helper ────────────────────────────────────────

/// Drain all pending OS debug events from the crossbeam channel and apply
/// them to `debug_state`.  Acquires the debugger lock first (for register
/// reads) then releases it before acquiring debug_state, so we never hold
/// both locks at the same time.
#[cfg(target_os = "windows")]
async fn drain_events_into_state(state: &AppState) {
    use fission_analysis::debug::types::DebugEvent;

    // Step 1: non-blocking drain into a local Vec (very short lock)
    let events: Vec<DebugEvent> = {
        match state.debug_event_rx.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
                .unwrap_or_default(),
            Err(_) => return,
        }
        // std::sync::MutexGuard dropped here
    };

    if events.is_empty() {
        return;
    }

    // Step 2: fetch registers for stop-events (debugger lock, brief)
    let mut reg_cache: std::collections::HashMap<u32, RegisterStateDto> =
        std::collections::HashMap::new();
    {
        let mut dbg = state.debugger.lock().await;
        if let Some(ref mut d) = *dbg {
            for evt in &events {
                let tid = match evt {
                    DebugEvent::BreakpointHit { thread_id, .. } => Some(*thread_id),
                    DebugEvent::SingleStep { thread_id } => Some(*thread_id),
                    _ => None,
                };
                if let Some(tid) = tid {
                    if let Ok(regs) = d.fetch_registers(tid) {
                        reg_cache.insert(tid, RegisterStateDto::from(regs));
                    }
                }
            }
        }
        // debugger lock dropped here
    }

    // Step 2b: record TTD steps if the timeline is actively recording.
    {
        let mut timeline = state.timeline.lock().await;
        if timeline.is_recording() {
            for evt in &events {
                if let DebugEvent::SingleStep { thread_id } = evt {
                    if let Some(regs) = reg_cache.get(thread_id) {
                        use fission_analysis::debug::types::RegisterState;
                        let rs = RegisterState {
                            rax: regs.rax, rbx: regs.rbx, rcx: regs.rcx, rdx: regs.rdx,
                            rsi: regs.rsi, rdi: regs.rdi, rbp: regs.rbp, rsp: regs.rsp,
                            r8:  regs.r8,  r9:  regs.r9,  r10: regs.r10, r11: regs.r11,
                            r12: regs.r12, r13: regs.r13, r14: regs.r14, r15: regs.r15,
                            rip: regs.rip, rflags: regs.rflags,
                        };
                        timeline.record_step_internal(rs, *thread_id);
                    }
                }
            }
        }
        // timeline lock dropped here
    }

    // Step 3: apply events to DTO (debug_state lock)
    let mut ds = state.debug_state.lock().await;
    // Trim log to avoid unbounded growth
    if ds.events.len() + events.len() > 500 {
        let keep = ds.events.len().saturating_sub(events.len());
        ds.events.drain(..ds.events.len() - keep.min(ds.events.len()));
    }
    for evt in events {
        match evt {
            DebugEvent::BreakpointHit { address, thread_id } => {
                ds.status = DebugStatusDto::Suspended;
                let msg = format!("[bp hit] 0x{:x} (tid {})", address, thread_id);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.registers = reg_cache.remove(&thread_id);
            }
            DebugEvent::SingleStep { thread_id } => {
                ds.status = DebugStatusDto::Suspended;
                let msg = format!("[step] tid {}", thread_id);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.registers = reg_cache.remove(&thread_id);
            }
            DebugEvent::ProcessExited { exit_code } => {
                ds.status = DebugStatusDto::Terminated;
                let msg = format!("[exit] Process exited (code {})", exit_code);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.attached_pid = None;
                ds.registers = None;
            }
            DebugEvent::ProcessCreated { pid, main_thread_id } => {
                let msg = format!("[create] PID {} main_tid {}", pid, main_thread_id);
                ds.events.push(msg.clone());
                ds.last_event = Some(msg);
            }
            DebugEvent::ThreadCreated { thread_id } => {
                ds.events.push(format!("[thread+] tid {}", thread_id));
            }
            DebugEvent::ThreadExited { thread_id } => {
                ds.events.push(format!("[thread-] tid {}", thread_id));
            }
            DebugEvent::DllLoaded { name, base_address } => {
                ds.events.push(format!("[dll] {} @ 0x{:x}", name, base_address));
            }
            DebugEvent::Exception { code, address, first_chance } => {
                let chance = if first_chance { "1st" } else { "2nd" };
                let msg = format!("[exc] 0x{:x} @ 0x{:x} ({})", code, address, chance);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
            }
        }
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Return the current debug session state (always safe to call).
/// On Windows, drains any pending OS debug events before returning.
#[tauri::command]
pub async fn debug_get_state(state: State<'_, AppState>) -> CmdResult<DebugStateDto> {
    #[cfg(target_os = "windows")]
    drain_events_into_state(&state).await;
    let ds = state.debug_state.lock().await;
    Ok(ds.clone())
}

/// Attach to a running process by PID.
/// On non-Windows builds returns an error immediately.
#[tauri::command]
pub async fn debug_attach(pid: u32, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::{traits::Debugger, windows::{WindowsDebugger, start_event_loop}};

        // Guard: already attached?
        {
            let ds = state.debug_state.lock().await;
            if ds.attached_pid.is_some() {
                return Err(CmdError::other("Already attached to a process"));
            }
        }

        // Create & attach debugger
        {
            let mut dbg = state.debugger.lock().await;
            let d = dbg.get_or_insert_with(WindowsDebugger::new);
            d.attach(pid).map_err(CmdError::from)?;
        }

        // Wire up background event loop
        let (tx_events, rx_events) =
            crossbeam_channel::unbounded::<fission_analysis::debug::types::DebugEvent>();
        let (tx_stop, rx_stop) = crossbeam_channel::bounded::<()>(1);
        start_event_loop(pid, tx_events, rx_stop);
        *state.debug_event_rx.lock().unwrap() = Some(rx_events);
        *state.debug_stop_tx.lock().unwrap() = Some(tx_stop);

        // Update DTO
        let mut ds = state.debug_state.lock().await;
        ds.status = DebugStatusDto::Running;
        ds.attached_pid = Some(pid);
        ds.events.push(format!("[attach] Attached to PID {}", pid));
        ds.last_event = Some(format!("Attached to PID {}", pid));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pid, state);
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

/// Detach from the currently attached process.
#[tauri::command]
pub async fn debug_detach(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        // Signal event loop to stop, then clear channel handles
        {
            if let Ok(guard) = state.debug_stop_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(());
                }
            }
        }
        *state.debug_stop_tx.lock().unwrap() = None;
        *state.debug_event_rx.lock().unwrap() = None;

        // Detach debugger
        {
            let mut dbg = state.debugger.lock().await;
            if let Some(ref mut d) = *dbg {
                d.detach().map_err(CmdError::from)?;
            }
            *dbg = None;
        }

        // Update DTO
        let mut ds = state.debug_state.lock().await;
        let pid = ds.attached_pid.unwrap_or_default();
        ds.events.push(format!("[detach] Detached from PID {}", pid));
        ds.status = DebugStatusDto::Detached;
        ds.attached_pid = None;
        ds.registers = None;
        ds.last_event = Some("Detached".to_string());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

/// Resume execution of a suspended process.
#[tauri::command]
pub async fn debug_continue(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.continue_execution().map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        ds.status = DebugStatusDto::Running;
        ds.last_event = Some("Continued".to_string());
        ds.events.push("[continue] Resumed execution".to_string());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

/// Single-step the suspended process.
#[tauri::command]
pub async fn debug_step(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.single_step().map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        ds.last_event = Some("Single-step issued".to_string());
        ds.events.push("[step] Single-step issued (waiting for SINGLE_STEP event)".to_string());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

/// Add a software breakpoint at `address`.
#[tauri::command]
pub async fn debug_add_breakpoint(address: u64, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.set_sw_breakpoint(address).map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        let addr_str = format!("0x{:x}", address);
        if !ds.breakpoints.iter().any(|bp| bp.address == addr_str) {
            ds.breakpoints.push(BreakpointInfoDto { address: addr_str.clone(), enabled: true });
        }
        ds.events.push(format!("[bp+] Breakpoint at {}", addr_str));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (address, state);
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

/// Remove a software breakpoint at `address`.
#[tauri::command]
pub async fn debug_remove_breakpoint(address: u64, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.remove_sw_breakpoint(address).map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        let addr_str = format!("0x{:x}", address);
        ds.breakpoints.retain(|bp| bp.address != addr_str);
        ds.events.push(format!("[bp-] Removed breakpoint at {}", addr_str));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (address, state);
        Err(CmdError::other("Dynamic debugging is only supported on Windows"))
    }
}

// ============================================================================
// Phase 5: String XRefs
// ============================================================================

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
        let Some(bytes) =
            binary.get_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            continue;
        };

        // ASCII scan
        let mut c_start: Option<usize> = None;
        let mut acc: Vec<u8> = Vec::new();
        for (i, &byte) in bytes.iter().enumerate() {
            if byte >= 0x20 && byte < 0x7f {
                if c_start.is_none() {
                    c_start = Some(i);
                }
                acc.push(byte);
            } else {
                if acc.len() >= min_len {
                    if let (Some(off), Ok(s)) = (c_start, std::str::from_utf8(&acc)) {
                        let va = section.virtual_address + off as u64;
                        let content = s.to_string();
                        if search_lc.is_empty()
                            || content.to_lowercase().contains(&search_lc)
                        {
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
                        if search_lc.is_empty()
                            || content.to_lowercase().contains(&search_lc)
                        {
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
        let Some(bytes) =
            binary.get_bytes(section.virtual_address, section.virtual_size as usize)
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
                    let func_name = binary
                        .function_at(ip)
                        .map(|f| {
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

// ============================================================================
// Phase 6: Analyze Functions / Deep Scan
// ============================================================================

/// Discover internal functions by scanning for CALL targets in executable sections.
/// Updates the loaded binary in-place and returns the full (updated) function list.
#[tauri::command]
pub async fn analyze_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let mut inner = state.inner.lock().await;

    // Clone the LoadedBinary so we can mutate it
    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    let before = binary.functions.len();
    binary.discover_internal_functions();
    let found = binary.functions.len().saturating_sub(before);

    // Snapshot renames before we move the binary back into the Arc
    let renames = inner.renamed_functions.clone();

    // Replace the Arc with the updated binary
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());
    let _ = found; // delta surfaced to the frontend via the returned slice length

    let functions = functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

/// Discover functions by scanning for common prologue byte patterns (push rbp / push ebp etc.).
/// This is a deeper heuristic scan that can find obfuscated or tail-call functions missed by
/// `analyze_functions`.  Returns the full updated function list.
#[tauri::command]
pub async fn deep_scan_functions(state: State<'_, AppState>) -> CmdResult<Vec<FunctionDto>> {
    let mut inner = state.inner.lock().await;

    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?
        .as_ref()
        .clone();

    binary.discover_functions_by_prologue();

    let renames = inner.renamed_functions.clone();
    let binary_arc = std::sync::Arc::new(binary);
    inner.loaded_binary = Some(binary_arc.clone());

    let functions = functions_to_dtos(&binary_arc, &renames);

    Ok(functions)
}

// ============================================================================
// Plugin System
// ============================================================================

/// Convert fission-core PluginInfo → PluginInfoDto.
fn plugin_info_to_dto(info: &fission_analysis::plugin::api::PluginInfo) -> PluginInfoDto {
    use fission_analysis::plugin::api::PluginType;
    PluginInfoDto {
        id: info.id.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        author: info.author.clone(),
        description: info.description.clone(),
        plugin_type: match info.plugin_type {
            PluginType::Native => PluginTypeDto::Native,
            _ => PluginTypeDto::Unknown,
        },
        enabled: info.enabled,
    }
}

/// Load a Rust native plugin (.so / .dylib / .dll) from disk.
/// Returns the plugin metadata on success.
#[tauri::command]
pub async fn load_plugin(path: String, state: State<'_, AppState>) -> CmdResult<PluginInfoDto> {
    let mut mgr = state.plugin_manager.lock().await;
    let plugin_id = mgr.load_plugin(&path)?;
    let info = mgr
        .get_plugin(&plugin_id)
        .ok_or_else(|| CmdError::other(format!("Plugin '{}' not found after load", plugin_id)))?;
    Ok(plugin_info_to_dto(info))
}

/// Unload a plugin by its ID.
#[tauri::command]
pub async fn unload_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.unload_plugin(&plugin_id).map_err(CmdError::other)
}

/// List all currently loaded plugins.
#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> CmdResult<Vec<PluginInfoDto>> {
    let mgr = state.plugin_manager.lock().await;
    let mut plugins: Vec<PluginInfoDto> = mgr
        .list_plugins()
        .into_iter()
        .map(plugin_info_to_dto)
        .collect();
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plugins)
}

/// Enable a loaded plugin.
#[tauri::command]
pub async fn enable_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.enable_plugin(&plugin_id).map_err(CmdError::other)
}

/// Disable a loaded plugin (keeps it in memory but marks it inactive).
#[tauri::command]
pub async fn disable_plugin(plugin_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut mgr = state.plugin_manager.lock().await;
    mgr.disable_plugin(&plugin_id).map_err(CmdError::other)
}

// ============================================================================
// Snapshot (lightweight annotation backup — no binary required)
// ============================================================================

/// Save current annotations (comments, renames, bookmarks) to a snapshot file.
/// Unlike save_project, this does NOT require a binary to be loaded.
#[tauri::command]
pub async fn save_snapshot(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let inner = state.inner.lock().await;

    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(addr, text)| (format!("0x{:x}", addr), text.clone()))
        .collect();
    let renames: std::collections::HashMap<String, String> = inner
        .renamed_functions
        .iter()
        .map(|(addr, name)| (format!("0x{:x}", addr), name.clone()))
        .collect();

    let snapshot = FissionProject {
        version: 1,
        binary_path: inner
            .loaded_binary
            .as_ref()
            .map(|b| b.path.clone())
            .unwrap_or_default(),
        comments,
        renames,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| CmdError::other(format!("Serialise failed: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write failed: {e}")))?;
    Ok(())
}

/// Load a snapshot file and restore annotations.
/// Returns the binary_path stored in the snapshot so the frontend can reload it.
#[tauri::command]
pub async fn load_snapshot(path: String, state: State<'_, AppState>) -> CmdResult<FissionProject> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CmdError::other(format!("Read failed: {e}")))?;
    let snapshot: FissionProject = serde_json::from_str(&json)
        .map_err(|e| CmdError::other(format!("Parse failed: {e}")))?;

    let mut inner = state.inner.lock().await;
    inner.comments = snapshot
        .comments
        .iter()
        .filter_map(|(addr_str, text)| {
            parse_address(addr_str).map(|a| (a, text.clone()))
        })
        .collect();
    inner.renamed_functions = snapshot
        .renames
        .iter()
        .filter_map(|(addr_str, name)| {
            parse_address(addr_str).map(|a| (a, name.clone()))
        })
        .collect();
    inner.bookmarks = snapshot.bookmarks.clone();
    Ok(snapshot)
}

// ============================================================================
// System / VCS utilities
// ============================================================================

/// Return the current git branch name, or "—" if outside a git repo.
#[tauri::command]
pub async fn get_git_branch() -> String {
    tokio::task::spawn_blocking(|| {
        std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "HEAD")
            .unwrap_or_else(|| "\u{2014}".to_string())
    })
    .await
    .unwrap_or_else(|_| "\u{2014}".to_string())
}

// ============================================================================
// FID — Function Identification
// ============================================================================

/// Scan all known functions in the loaded binary against the built-in MSVC/CRT
/// signature database and rename any that match.
///
/// Returns a [`FidResultDto`] with the count of matched functions and their
/// names so the frontend can refresh the function list.
#[tauri::command]
pub async fn run_fid(state: State<'_, AppState>) -> CmdResult<FidResultDto> {
    use fission_signatures::SignatureDatabase;

    let mut inner = state.inner.lock().await;

    // Collect everything we need from `binary` inside a block so the immutable
    // borrow of `inner` ends before we mutably update `renamed_functions`.
    let (data, image_base, func_list, prev_names) = {
        let binary = inner
            .loaded_binary
            .as_ref()
            .ok_or_else(|| CmdError::other("No binary loaded"))?;

        let mut prev_names: std::collections::HashMap<u64, String> =
            std::collections::HashMap::new();
        let func_list: Vec<(u64, String)> = binary
            .functions
            .iter()
            .map(|f| {
                let current = inner
                    .renamed_functions
                    .get(&f.address)
                    .cloned()
                    .unwrap_or_else(|| f.name.clone());
                prev_names.insert(f.address, current.clone());
                (f.address, current)
            })
            .collect();

        let data: Vec<u8> = binary.inner().data.as_slice().to_vec();
        let image_base = binary.image_base;

        (data, image_base, func_list, prev_names)
    }; // `binary` (and immutable borrow of `inner`) dropped here

    let total_scanned = func_list.len();

    // Run identification in a blocking thread (CPU-bound).
    let identified = tokio::task::spawn_blocking(move || {
        let db = SignatureDatabase::new();
        db.identify_functions_in_binary(&data, &func_list, image_base)
    })
    .await
    .map_err(|e| CmdError::other(format!("FID task failed: {e}")))?;

    // Apply renames to the state and collect match details.
    let mut matches = Vec::new();
    for (addr, new_name) in &identified {
        let prev_name = prev_names.get(addr).cloned().unwrap_or_default();
        inner.renamed_functions.insert(*addr, new_name.clone());
        matches.push(FidMatchDto {
            address: format!("0x{:x}", addr),
            name: new_name.clone(),
            previous_name: prev_name,
        });
    }

    let matched = matches.len();
    Ok(FidResultDto { matched, total_scanned, matches })
}

// ============================================================================
// Phase 8: Export Analysis JSON
// ============================================================================

/// Export all analysis artefacts (functions, comments, bookmarks) to a JSON
/// file at `path`.  The caller (frontend) opens a save-file dialog and passes
/// the chosen path.
#[tauri::command]
pub async fn export_analysis_json(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    // Collect functions with current names applied.
    let functions: Vec<ExportedFunctionDto> = binary
        .functions
        .iter()
        .map(|f| {
            let name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            let is_renamed = inner.renamed_functions.contains_key(&f.address);
            ExportedFunctionDto {
                address: format!("0x{:x}", f.address),
                name,
                is_renamed,
            }
        })
        .collect();

    // Collect comments keyed by hex address.
    let comments: std::collections::HashMap<String, String> = inner
        .comments
        .iter()
        .map(|(k, v)| (format!("0x{:x}", k), v.clone()))
        .collect();

    let binary_name = std::path::Path::new(&binary.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let binary_fingerprint =
        format!("bytes:{}", binary.inner().data.as_slice().len());

    let exported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let export = AnalysisExportDto {
        version: 1,
        exported_at,
        binary_name,
        binary_path: binary.path.clone(),
        binary_fingerprint,
        functions,
        comments,
        bookmarks: inner.bookmarks.clone(),
    };

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| CmdError::other(format!("JSON serialisation failed: {e}")))?;

    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write failed: {e}")))?;

    Ok(())
}

// ============================================================================
// Phase 4: Debug Memory Dump
// ============================================================================

/// Read `size` bytes from the attached process starting at `address` (hex
/// string, e.g. `"0x401000"`) and return a formatted hex dump.
///
/// On non-Windows platforms this command always returns an error because there
/// is no live-debugging backend.
#[tauri::command]
pub async fn debug_read_memory(
    address: String,
    size: usize,
    state: State<'_, AppState>,
) -> CmdResult<String> {
    let addr = u64::from_str_radix(address.trim_start_matches("0x"), 16)
        .map_err(|_| CmdError::other(format!("Invalid address: {address}")))?;

    if size == 0 || size > 4096 {
        return Err(CmdError::other("Size must be 1–4096 bytes"));
    }

    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;
        let mut dbg = state.debugger.lock().await;
        let d = dbg
            .as_mut()
            .ok_or_else(|| CmdError::other("No process attached"))?;

        let bytes = d
            .read_memory(addr, size)
            .map_err(|e| CmdError::other(format!("ReadProcessMemory failed: {e}")))?;

        // Format as classic hex dump: 16 bytes per line.
        let mut out = String::new();
        for (chunk_idx, chunk) in bytes.chunks(16).enumerate() {
            let line_addr = addr + (chunk_idx as u64) * 16;
            let hex_part: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            let ascii_part: String = chunk
                .iter()
                .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                .collect();
            out.push_str(&format!(
                "0x{:016x}  {:<47}  {}\n",
                line_addr,
                hex_part.join(" "),
                ascii_part
            ));
        }
        return Ok(out);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (addr, state);
        Err(CmdError::other(
            "Memory dump is only supported on Windows (live debugger required)",
        ))
    }
}

// ============================================================================
// Phase 5: TTD (Time Travel Debugging) commands
// ============================================================================

fn snapshot_to_dto(s: &fission_analysis::debug::ttd::ExecutionSnapshot) -> TtdSnapshotDto {
    TtdSnapshotDto {
        step: s.step_index,
        thread_id: s.thread_id,
        rip: format!("0x{:x}", s.registers.rip),
        rax: s.registers.rax,
        rbx: s.registers.rbx,
        rcx: s.registers.rcx,
        rdx: s.registers.rdx,
        rsp: s.registers.rsp,
        rbp: s.registers.rbp,
        rsi: s.registers.rsi,
        rdi: s.registers.rdi,
        rflags: s.registers.rflags,
    }
}

fn timeline_to_state_dto(tl: &fission_analysis::debug::ttd::Timeline) -> TtdStateDto {
    let stats = tl.stats();
    let step_range = tl.step_range().map(|(a, b)| [a, b]);
    let current_step = tl.current_position();
    let current_snapshot = tl.current_snapshot().map(snapshot_to_dto);
    TtdStateDto {
        is_recording: tl.is_recording(),
        snapshot_count: stats.count as usize,
        step_range,
        current_step,
        current_snapshot,
    }
}

/// Start TTD recording. While recording, every debugger `SingleStep` event is
/// captured automatically (Windows only; on other platforms the timeline simply
/// accumulates no snapshots).
#[tauri::command]
pub async fn ttd_start(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    tl.start_recording();
    Ok(timeline_to_state_dto(&tl))
}

/// Stop TTD recording and enter replay mode so the timeline can be seeked.
#[tauri::command]
pub async fn ttd_stop(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    tl.stop_recording();
    tl.enter_replay_mode();
    Ok(timeline_to_state_dto(&tl))
}

/// Return the current TTD timeline state without modifying it.
#[tauri::command]
pub async fn ttd_status(state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let tl = state.timeline.lock().await;
    Ok(timeline_to_state_dto(&tl))
}

/// Seek to a specific step index.  Returns the updated timeline state including
/// the register snapshot at that step (if found).
#[tauri::command]
pub async fn ttd_seek(step: u64, state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    let _ = tl.seek_to(step);
    Ok(timeline_to_state_dto(&tl))
}

/// Step one position in the given `direction` (`"forward"` or `"rewind"`).
#[tauri::command]
pub async fn ttd_step(direction: String, state: State<'_, AppState>) -> CmdResult<TtdStateDto> {
    let mut tl = state.timeline.lock().await;
    if direction == "rewind" {
        let _ = tl.rewind(1);
    } else {
        let _ = tl.forward(1);
    }
    Ok(timeline_to_state_dto(&tl))
}
