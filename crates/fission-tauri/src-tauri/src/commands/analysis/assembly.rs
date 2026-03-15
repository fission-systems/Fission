//! Disassembly and decompilation commands.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
#[cfg(feature = "native_decomp")]
use fission_loader::loader::LoadedBinary;
#[cfg(feature = "native_decomp")]
use fission_static::analysis::decomp::{
    rescue_preview_output, select_preview_output, PreviewEngineMode, PreviewSelection,
};
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

#[cfg(feature = "native_decomp")]
struct DecompileOutcome {
    code: String,
    engine_used: DecompilerEngineMode,
    fell_back: bool,
    fallback_reason: Option<String>,
}

#[cfg(feature = "native_decomp")]
fn decompile_with_engine(
    decomp: &mut fission_static::analysis::decomp::CachingDecompiler,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    engine_mode: DecompilerEngineMode,
) -> Result<DecompileOutcome, CmdError> {
    let preview_mode = match engine_mode {
        DecompilerEngineMode::Legacy => PreviewEngineMode::Legacy,
        DecompilerEngineMode::MlilPreview => PreviewEngineMode::MlilPreview,
        DecompilerEngineMode::Auto => PreviewEngineMode::Auto,
    };
    let preview = select_preview_output(decomp, binary, address, name, preview_mode)
        .map_err(CmdError::other)?;
    if let Some(code) = preview.preview_code {
        return Ok(DecompileOutcome {
            code,
            engine_used: DecompilerEngineMode::MlilPreview,
            fell_back: false,
            fallback_reason: None,
        });
    }
    match decomp.decompile(address) {
        Ok(code) => Ok(outcome_from_preview_selection(code, preview)),
        Err(e) => {
            let error_text = e.to_string();
            if !matches!(engine_mode, DecompilerEngineMode::Legacy) {
                if let Some(selection) =
                    rescue_preview_output(decomp, binary, address, name, &error_text)
                        .map_err(CmdError::other)?
                {
                    if let Some(code) = selection.preview_code {
                        return Ok(DecompileOutcome {
                            code,
                            engine_used: DecompilerEngineMode::MlilPreview,
                            fell_back: true,
                            fallback_reason: selection.fallback_reason,
                        });
                    }
                }
            }
            Err(CmdError::other(format!("{e}")))
        }
    }
}

#[cfg(feature = "native_decomp")]
fn outcome_from_preview_selection(code: String, selection: PreviewSelection) -> DecompileOutcome {
    let engine_used = match selection.engine_used {
        PreviewEngineMode::Legacy => DecompilerEngineMode::Legacy,
        PreviewEngineMode::MlilPreview => DecompilerEngineMode::MlilPreview,
        PreviewEngineMode::Auto => DecompilerEngineMode::Auto,
    };
    DecompileOutcome {
        code,
        engine_used,
        fell_back: selection.fell_back,
        fallback_reason: selection.fallback_reason,
    }
}

/// Decompile a function at the given address.
#[tauri::command]
pub async fn decompile_function(
    address: u64,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<DecompileResult> {
    // Step 1: Grab func_name and binary from the inner lock, then DROP it immediately
    let (func_name, binary) = {
        let inner = state.inner.lock().await;
        let func_name = inner
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
            .unwrap_or_else(|| format!("sub_{:x}", address));
        let binary = inner.loaded_binary.clone();
        (func_name, binary)
    };
    let binary = binary.ok_or_else(|| CmdError::other("No binary loaded"))?;
    let engine_mode = crate::commands::workspace::settings::get_settings(app_handle)
        .await
        .ok()
        .and_then(|settings| settings.decompiler_options)
        .unwrap_or_default()
        .engine_mode;

    // Step 2: Decompile using the SEPARATE decompiler Mutex
    #[cfg(feature = "native_decomp")]
    {
        let mut decomp_lock = state.decompiler.lock().await;
        let decomp_result = decomp_lock
            .as_mut()
            .ok_or_else(|| CmdError::other("Decompiler not initialized"))
            .and_then(|decomp| {
                decompile_with_engine(decomp, binary.as_ref(), address, &func_name, engine_mode)
            });
        drop(decomp_lock);

        match decomp_result {
            Ok(result) => Ok(DecompileResult {
                code: result.code,
                function_name: func_name,
                address: format!("0x{:x}", address),
                engine_used: result.engine_used,
                fell_back: result.fell_back,
                fallback_reason: result.fallback_reason,
            }),
            Err(e) => Ok(DecompileResult {
                code: format!(
                    "// Decompilation failed: {}\n// Function: {}\n// Address: 0x{:x}\n",
                    e, func_name, address
                ),
                function_name: func_name,
                address: format!("0x{:x}", address),
                engine_used: DecompilerEngineMode::Legacy,
                fell_back: false,
                fallback_reason: Some(e.to_string()),
            }),
        }
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
            engine_used: DecompilerEngineMode::Legacy,
            fell_back: false,
            fallback_reason: Some("native decompiler not available".to_string()),
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

/// Clear the in-memory decompiler cache (forces re-decompilation on next request).
///
/// The actual decompile/asm cache is managed on the frontend; this command
/// serves as a hook for any future server-side cache that may be added.
#[tauri::command]
pub async fn clear_decompiler_cache(_state: State<'_, AppState>) -> CmdResult<()> {
    // Currently the decompile result cache lives in front-end React state.
    // This command is intentionally a no-op on the backend so the front-end
    // can call it and then clear its own cache in response.
    Ok(())
}
