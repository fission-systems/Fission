//! Fission Tauri — Tauri command handlers.
//!
//! Each `#[tauri::command]` function bridges the frontend (React) to the
//! backend (fission-loader / fission-analysis).

use crate::dto::*;
use crate::state::AppState;
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use tauri::State;

/// Open and parse a binary file.
#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> Result<BinaryInfo, String> {
    // Load binary on a blocking thread (CPU-heavy parsing)
    let binary = tokio::task::spawn_blocking(move || LoadedBinary::from_file(&path))
        .await
        .map_err(|e| format!("Task failed: {e}"))?
        .map_err(|e| format!("Failed to load binary: {e}"))?;

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
                // Load binary into decompiler context
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
                    let mut inner = state.inner.lock().await;
                    inner.decompiler = Some(decomp);
                    inner.decompiler_loaded = true;
                }
            }
            Err(e) => {
                eprintln!("[!] Failed to initialize decompiler: {e}");
            }
        }
    }

    // Store the binary in state
    let mut inner = state.inner.lock().await;
    inner.loaded_binary = Some(binary_arc);

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
        .map(|f| FunctionDto {
            address: format!("0x{:x}", f.address),
            name: f.name.clone(),
            size: f.size,
        })
        .collect();

    Ok(functions)
}

/// Decompile a function at the given address.
#[tauri::command]
pub async fn decompile_function(
    address: u64,
    state: State<'_, AppState>,
) -> Result<DecompileResult, String> {
    let mut inner = state.inner.lock().await;

    // Get function name
    let func_name = inner
        .loaded_binary
        .as_ref()
        .and_then(|b| b.function_at(address))
        .map(|f| f.name.clone())
        .unwrap_or_else(|| format!("sub_{:x}", address));

    #[cfg(feature = "native_decomp")]
    {
        let decomp = inner
            .decompiler
            .as_mut()
            .ok_or_else(|| "Decompiler not initialized".to_string())?;

        let code = decomp.decompile(address).map_err(|e| format!("{e}"))?;

        return Ok(DecompileResult {
            code,
            function_name: func_name,
            address: format!("0x{:x}", address),
        });
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

    // Get bytes at the address
    let byte_count = count * 15; // max x86 instruction is 15 bytes
    let bytes = binary
        .get_bytes(address, byte_count)
        .ok_or_else(|| format!("Cannot read bytes at 0x{:x}", address))?;

    // Use iced-x86 for disassembly
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

        // Format bytes
        let start = (insn.ip() - address) as usize;
        let end = start + insn.len();
        let hex_bytes: String = bytes[start..end]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        // Split mnemonic and operands
        let parts: Vec<&str> = output.splitn(2, ' ').collect();
        let mnemonic = parts.first().unwrap_or(&"").to_string();
        let operands = parts.get(1).unwrap_or(&"").to_string();

        instructions.push(AsmInstructionDto {
            address: format!("0x{:x}", insn.ip()),
            bytes: hex_bytes,
            mnemonic,
            operands,
        });

        i += 1;
    }

    Ok(instructions)
}

/// Get extracted strings from the loaded binary.
#[tauri::command]
pub async fn get_strings(state: State<'_, AppState>) -> Result<Vec<StringDto>, String> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    // Extract ASCII strings from the binary data
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

        // Cap at 10000 strings for performance
        if strings.len() >= 10000 {
            break;
        }
    }

    Ok(strings)
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

/// Find the Sleigh specification directory.
#[cfg(feature = "native_decomp")]
fn find_sla_dir() -> String {
    // Try several well-known paths
    let candidates = [
        // Relative to executable
        "ghidra_decompiler/languages",
        "../ghidra_decompiler/languages",
        "../../ghidra_decompiler/languages",
        // Relative to workspace root (common during dev)
        "../../../ghidra_decompiler/languages",
        "../../../../ghidra_decompiler/languages",
    ];

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    for candidate in &candidates {
        // Try relative to CWD
        let path = std::path::Path::new(candidate);
        if path.is_dir() {
            return path.to_string_lossy().to_string();
        }

        // Try relative to exe dir
        if let Some(ref exe) = exe_dir {
            let path = exe.join(candidate);
            if path.is_dir() {
                return path.to_string_lossy().to_string();
            }
        }
    }

    // Fallback
    "ghidra_decompiler/languages".to_string()
}
