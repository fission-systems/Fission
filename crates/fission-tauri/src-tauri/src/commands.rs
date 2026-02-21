//! Fission Tauri — Tauri command handlers.
//!
//! Each `#[tauri::command]` function bridges the frontend (React) to the
//! backend (fission-loader / fission-analysis).

use crate::dto::*;
use crate::state::AppState;
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use tauri::State;
use tauri::Manager as _;

// ============================================================================
// File / Binary Operations
// ============================================================================

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> Result<BinaryInfo, String> {
    let binary = tokio::task::spawn_blocking(move || {
        let mut binary = LoadedBinary::from_file(&path)
            .map_err(|e| format!("Failed to load binary: {e}"))?;
        // Automatic multi-pass function discovery (runs in the worker thread)
        binary.discover_internal_functions();    // Pass 1: CALL target scan
        binary.discover_functions_by_prologue(); // Pass 2: prologue pattern scan
        Ok::<LoadedBinary, String>(binary)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
    ?;

    let info = BinaryInfo {
        name: std::path::Path::new(&binary.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: binary.path.clone(),
        arch: binary.arch_spec.clone(),
        format: binary.format.clone(),
        entry_point: format!("0x{:x}", binary.entry_point),
        section_count: binary.sections.len(),
        function_count: binary.functions.len(),
        image_base: format!("0x{:x}", binary.image_base),
    };

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
                    eprintln!("[!] Failed to load binary into decompiler: {e}");
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
                        .functions
                        .iter()
                        .filter(|f| f.is_import)
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
                eprintln!("[!] Failed to initialize decompiler: {e}");
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
pub async fn get_functions(state: State<'_, AppState>) -> Result<Vec<FunctionDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let functions: Vec<FunctionDto> = binary
        .functions
        .iter()
        .map(|f| {
            let display_name = inner
                .renamed_functions
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());

            let category = if f.is_import {
                "import"
            } else if f.is_export {
                "export"
            } else {
                "internal"
            };

            FunctionDto {
                address: format!("0x{:x}", f.address),
                name: display_name,
                size: f.size,
                category: category.to_string(),
            }
        })
        .collect();

    Ok(functions)
}

/// Get info about the currently loaded binary.
#[tauri::command]
pub async fn get_binary_info(state: State<'_, AppState>) -> Result<Option<BinaryInfo>, String> {
    let inner = state.inner.lock().await;
    let info = inner.loaded_binary.as_ref().map(|binary| BinaryInfo {
        name: std::path::Path::new(&binary.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: binary.path.clone(),
        arch: binary.arch_spec.clone(),
        format: binary.format.clone(),
        entry_point: format!("0x{:x}", binary.entry_point),
        section_count: binary.sections.len(),
        function_count: binary.functions.len(),
        image_base: format!("0x{:x}", binary.image_base),
    });
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
) -> Result<DecompileResult, String> {
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
            .ok_or_else(|| "Decompiler not initialized".to_string())
            .and_then(|decomp| decomp.decompile(address).map_err(|e| format!("{e}")));
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
) -> Result<Vec<AsmInstructionDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let byte_count = count * 15;
    let bytes = binary
        .get_bytes(address, byte_count)
        .ok_or_else(|| format!("Cannot read bytes at 0x{:x}", address))?;

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
pub async fn get_strings(state: State<'_, AppState>) -> Result<Vec<StringDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let data = binary.inner().data.as_slice();
    let mut strings = Vec::new();
    let min_len = 4;

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

    Ok(strings)
}

/// Get import table entries.
#[tauri::command]
pub async fn get_imports(state: State<'_, AppState>) -> Result<Vec<ImportDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let imports: Vec<ImportDto> = binary
        .functions
        .iter()
        .filter(|f| f.is_import)
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
                ("unknown".to_string(), display_name)
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

/// Get section information.
#[tauri::command]
pub async fn get_sections(state: State<'_, AppState>) -> Result<Vec<SectionDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
) -> Result<(), String> {
    let mut inner = state.inner.lock().await;

    // Verify the binary is loaded
    if inner.loaded_binary.is_none() {
        return Err("No binary loaded".to_string());
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
) -> Result<(), String> {
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
) -> Result<std::collections::HashMap<String, String>, String> {
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
) -> Result<bool, String> {
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
pub async fn get_bookmarks(state: State<'_, AppState>) -> Result<Vec<BookmarkDto>, String> {
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
) -> Result<GotoResult, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
) -> Result<HexViewData, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let total_size = binary
        .sections
        .iter()
        .map(|s| s.virtual_address + s.virtual_size)
        .max()
        .unwrap_or(0);

    // Clamp length to avoid excessive memory
    let actual_len = length.min(4096);
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
) -> Result<Vec<u8>, String> {
    let mut inner = state.inner.lock().await;
    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?
        .as_ref()
        .clone();

    let original = binary
        .patch_bytes_va(address, &bytes)
        .ok_or_else(|| format!("Patch failed: address 0x{:x} out of range", address))?;

    inner.loaded_binary = Some(std::sync::Arc::new(binary));
    Ok(original)
}

/// Save the (potentially patched) binary to a new file path.
#[tauri::command]
pub async fn save_patched_binary(
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    binary
        .save_as(&path)
        .map_err(|e| format!("Save failed: {e}"))
}

/// Search functions, strings, and addresses.
#[tauri::command]
pub async fn search_binary(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResultDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
            // Limit per-section scan to 256KB
            let scan_len = (section.virtual_size as usize).min(256 * 1024);
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
) -> Result<Vec<XrefDto>, String> {
    use iced_x86::{Decoder, DecoderOptions, FlowControl};

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let bitness: u32 = if binary.is_64bit { 64 } else { 32 };
    let mut results: Vec<XrefDto> = Vec::new();

    for section in binary.sections.iter().filter(|s| s.is_executable) {
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

            if results.len() >= 2000 {
                break;
            }
        }
    }

    // Also find outgoing refs FROM this address (what does this function call?)
    let func = binary.functions.iter().find(|f| f.address == address);
    if let Some(func) = func {
        let size = if func.size > 0 { func.size as usize } else { 256 };
        if let Some(bytes) = binary.get_bytes(address, size.min(65536)) {
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

                if results.len() >= 4000 { break; }
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
pub async fn get_listing_info(state: State<'_, AppState>) -> Result<ListingInfo, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let exec_sections: Vec<_> = binary.sections.iter().filter(|s| s.is_executable).collect();
    if exec_sections.is_empty() {
        return Err("No executable sections found".to_string());
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
) -> Result<Vec<ListingRow>, String> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
    use std::collections::HashMap;

    let start_address = parse_address(&start_address)
        .ok_or_else(|| format!("Invalid address: {}", start_address))?;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
        .ok_or_else(|| format!("No executable section covers 0x{:x}", start_address))?;

    let effective_start = start_address.max(section.virtual_address);
    let section_end = section.virtual_address + section.virtual_size;
    let decode_size = (section_end - effective_start) as usize;

    let bytes = binary
        .get_bytes(effective_start, decode_size)
        .ok_or_else(|| format!("Cannot read bytes at 0x{:x}", effective_start))?;

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
pub async fn save_project(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
        .map_err(|e| format!("Serialise failed: {e}"))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Write failed: {e}"))?;

    Ok(())
}

/// Load a `.fprj` project file.  Restores user annotations from the file.
/// The binary itself must already be (or will be) loaded separately via `open_file`.
/// Returns the recorded binary path so the frontend can reload it if needed.
#[tauri::command]
pub async fn load_project(path: String, state: State<'_, AppState>) -> Result<crate::dto::FissionProject, String> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read failed: {e}"))?;

    let project: crate::dto::FissionProject = serde_json::from_str(&json)
        .map_err(|e| format!("Parse failed: {e}"))?;

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
fn settings_path(app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app-data dir: {e}"))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| format!("Cannot create app-data dir: {e}"))?;
    Ok(data_dir.join("settings.json"))
}

/// Load persisted application settings (or defaults if none saved yet).
#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> Result<crate::dto::AppSettings, String> {
    let path = settings_path(&app_handle)?;
    if !path.exists() {
        return Ok(crate::dto::AppSettings::default());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read settings failed: {e}"))?;
    // If schema is corrupt or outdated, fall back to defaults silently
    Ok(serde_json::from_str(&json).unwrap_or_else(|_| {
        eprintln!("[!] settings.json invalid or schema changed, using defaults");
        crate::dto::AppSettings::default()
    }))
}

/// Persist application settings.
#[tauri::command]
pub async fn save_settings(
    settings: crate::dto::AppSettings,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let path = settings_path(&app_handle)?;
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Serialise settings failed: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Write settings failed: {e}"))?;
    Ok(())
}

/// Clear the in-memory decompiler cache (forces re-decompilation on next request).
/// The actual decompile/asm cache is managed on the frontend; this command
/// serves as a hook for any future server-side cache that may be added.
#[tauri::command]
pub async fn clear_decompiler_cache(_state: State<'_, AppState>) -> Result<(), String> {
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
) -> Result<CfgDto, String> {
    use iced_x86::{Decoder, DecoderOptions, FlowControl, Formatter, IntelFormatter};
    use std::collections::{HashMap, HashSet};

    let address = parse_address(&address)
        .ok_or_else(|| format!("Invalid address: {}", address))?;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let func = binary
        .functions
        .iter()
        .find(|f| f.address == address)
        .ok_or_else(|| format!("Function at 0x{:x} not found", address))?;

    let func_name = inner
        .renamed_functions
        .get(&address)
        .cloned()
        .unwrap_or_else(|| func.name.clone());

    let decode_size = if func.size > 0 { (func.size as usize).min(65536) } else { 4096 };
    let bytes = binary
        .get_bytes(address, decode_size)
        .ok_or_else(|| format!("Cannot read bytes at 0x{:x}", address))?;

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

    Ok(CfgDto {
        function_name: func_name,
        function_address: format!("0x{:x}", address),
        nodes,
        edges,
    })
}

/// Export the CFG of `address` as a Graphviz DOT string (copied to clipboard on the frontend).
#[tauri::command]
pub async fn export_cfg_dot(
    address: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
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

/// Parse a hex or decimal address string.
fn parse_address(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if trimmed.chars().all(|c| c.is_ascii_hexdigit()) && trimmed.len() >= 4 {
        // Looks like a hex address without prefix
        u64::from_str_radix(trimmed, 16).ok()
    } else {
        trimmed.parse::<u64>().ok()
    }
}

/// Find the Sleigh specification directory.
#[cfg(feature = "native_decomp")]
fn find_sla_dir() -> String {
    let candidates = [
        "ghidra_decompiler/languages",
        "../ghidra_decompiler/languages",
        "../../ghidra_decompiler/languages",
        "../../../ghidra_decompiler/languages",
        "../../../../ghidra_decompiler/languages",
    ];

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    for candidate in &candidates {
        let path = std::path::Path::new(candidate);
        if path.is_dir() {
            return path.to_string_lossy().to_string();
        }

        if let Some(ref exe) = exe_dir {
            let path = exe.join(candidate);
            if path.is_dir() {
                return path.to_string_lossy().to_string();
            }
        }
    }

    "ghidra_decompiler/languages".to_string()
}

// ============================================================================
// Debug
// ============================================================================

/// Return the current debug session state (always safe to call).
#[tauri::command]
pub async fn debug_get_state(state: State<'_, AppState>) -> Result<DebugStateDto, String> {
    let ds = state.debug_state.lock().await;
    Ok(ds.clone())
}

/// Attach to a running process by PID.
/// On non-Windows builds this currently returns an error.
#[tauri::command]
pub async fn debug_attach(pid: u32, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut ds = state.debug_state.lock().await;
        if ds.attached_pid.is_some() {
            return Err("Already attached to a process".to_string());
        }
        ds.status = DebugStatusDto::Attaching;
        ds.attached_pid = Some(pid);
        ds.last_event = Some(format!("Attaching to PID {}", pid));
        ds.events.push(format!("[attach] PID {}", pid));
        // TODO: wire up fission_analysis::debug Windows backend
        ds.status = DebugStatusDto::Running;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pid, state);
        Err("Dynamic debugging is only supported on Windows".to_string())
    }
}

/// Detach from the currently attached process.
#[tauri::command]
pub async fn debug_detach(state: State<'_, AppState>) -> Result<(), String> {
    let mut ds = state.debug_state.lock().await;
    if ds.attached_pid.is_none() {
        return Err("Not attached to any process".to_string());
    }
    let pid = ds.attached_pid.unwrap_or(0);
    ds.events.push(format!("[detach] Detached from PID {}", pid));
    ds.status = DebugStatusDto::Detached;
    ds.attached_pid = None;
    ds.registers = None;
    ds.last_event = Some("Detached".to_string());
    Ok(())
}

/// Resume execution of a suspended process.
#[tauri::command]
pub async fn debug_continue(state: State<'_, AppState>) -> Result<(), String> {
    let mut ds = state.debug_state.lock().await;
    if ds.attached_pid.is_none() {
        return Err("Not attached".to_string());
    }
    if ds.status != DebugStatusDto::Suspended {
        return Err("Process is not suspended".to_string());
    }
    ds.status = DebugStatusDto::Running;
    ds.last_event = Some("Continued".to_string());
    ds.events.push("[continue] Resumed execution".to_string());
    Ok(())
}

/// Single-step the suspended process.
#[tauri::command]
pub async fn debug_step(state: State<'_, AppState>) -> Result<(), String> {
    let mut ds = state.debug_state.lock().await;
    if ds.attached_pid.is_none() {
        return Err("Not attached".to_string());
    }
    if ds.status != DebugStatusDto::Suspended {
        return Err("Process is not suspended".to_string());
    }
    ds.last_event = Some("Step".to_string());
    ds.events.push("[step] Single-stepped".to_string());
    Ok(())
}

/// Add a software breakpoint at `address` (hex string or decimal).
#[tauri::command]
pub async fn debug_add_breakpoint(address: u64, state: State<'_, AppState>) -> Result<(), String> {
    let mut ds = state.debug_state.lock().await;
    let addr_str = format!("0x{:x}", address);
    if ds.breakpoints.iter().any(|bp| bp.address == addr_str) {
        return Err(format!("Breakpoint already exists at {}", addr_str));
    }
    ds.breakpoints.push(BreakpointInfoDto {
        address: addr_str.clone(),
        enabled: true,
    });
    ds.events.push(format!("[bp+] Added breakpoint at {}", addr_str));
    Ok(())
}

/// Remove a breakpoint at `address`.
#[tauri::command]
pub async fn debug_remove_breakpoint(address: u64, state: State<'_, AppState>) -> Result<(), String> {
    let mut ds = state.debug_state.lock().await;
    let addr_str = format!("0x{:x}", address);
    let before = ds.breakpoints.len();
    ds.breakpoints.retain(|bp| bp.address != addr_str);
    if ds.breakpoints.len() == before {
        return Err(format!("No breakpoint at {}", addr_str));
    }
    ds.events.push(format!("[bp-] Removed breakpoint at {}", addr_str));
    Ok(())
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
) -> Result<Vec<StringXrefDto>, String> {
    use iced_x86::{Decoder, DecoderOptions, OpKind};
    use std::collections::HashMap;

    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

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
pub async fn analyze_functions(state: State<'_, AppState>) -> Result<Vec<FunctionDto>, String> {
    let mut inner = state.inner.lock().await;

    // Clone the LoadedBinary so we can mutate it
    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?
        .as_ref()
        .clone();

    let before = binary.functions.len();
    binary.discover_internal_functions();
    let found = binary.functions.len().saturating_sub(before);

    // Snapshot renames before we move the binary back into the Arc
    let renames = inner.renamed_functions.clone();

    // Replace the Arc with the updated binary
    inner.loaded_binary = Some(std::sync::Arc::new(binary));
    let _ = found; // delta surfaced to the frontend via the returned slice length

    let functions: Vec<FunctionDto> = inner
        .loaded_binary
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .map(|f| {
            let display_name = renames
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            let category = if f.is_import {
                "import"
            } else if f.is_export {
                "export"
            } else {
                "internal"
            };
            FunctionDto {
                address: format!("0x{:x}", f.address),
                name: display_name,
                size: f.size,
                category: category.to_string(),
            }
        })
        .collect();

    Ok(functions)
}

/// Discover functions by scanning for common prologue byte patterns (push rbp / push ebp etc.).
/// This is a deeper heuristic scan that can find obfuscated or tail-call functions missed by
/// `analyze_functions`.  Returns the full updated function list.
#[tauri::command]
pub async fn deep_scan_functions(state: State<'_, AppState>) -> Result<Vec<FunctionDto>, String> {
    let mut inner = state.inner.lock().await;

    let mut binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?
        .as_ref()
        .clone();

    binary.discover_functions_by_prologue();

    let renames = inner.renamed_functions.clone();
    inner.loaded_binary = Some(std::sync::Arc::new(binary));

    let functions: Vec<FunctionDto> = inner
        .loaded_binary
        .as_ref()
        .unwrap()
        .functions
        .iter()
        .map(|f| {
            let display_name = renames
                .get(&f.address)
                .cloned()
                .unwrap_or_else(|| f.name.clone());
            let category = if f.is_import {
                "import"
            } else if f.is_export {
                "export"
            } else {
                "internal"
            };
            FunctionDto {
                address: format!("0x{:x}", f.address),
                name: display_name,
                size: f.size,
                category: category.to_string(),
            }
        })
        .collect();

    Ok(functions)
}
