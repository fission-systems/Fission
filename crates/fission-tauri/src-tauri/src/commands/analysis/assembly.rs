//! Disassembly and decompilation commands.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_decompiler_core::{decompile_with_rust_sleigh, RustSleighDecompileConfig};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::fallback_reason_with_kind;
use std::time::Duration;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

struct DecompileOutcome {
    code: String,
    engine_used: DecompilerEngineMode,
    fell_back: bool,
    fallback_reason: Option<String>,
}

fn decompile_rust_only(
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    decompiler_options: crate::dto::DecompilerOptions,
) -> Result<DecompileOutcome, CmdError> {
    let mut config = RustSleighDecompileConfig::tauri_defaults();
    config.nir_mode = fission_decompiler_core::NirEngineMode::Nir;

    let result = decompile_with_rust_sleigh(
        binary,
        address,
        name,
        &config,
        Some(decompiler_options.performance.max_function_size),
        Some(decompiler_options.performance.max_instructions),
    )
    .map_err(CmdError::other)?;

    Ok(DecompileOutcome {
        code: result.code,
        engine_used: DecompilerEngineMode::Nir,
        fell_back: result.fell_back,
        fallback_reason: result.fallback_reason,
    })
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
            .fact_store
            .as_ref()
            .and_then(|store| store.resolved_name(address).map(ToString::to_string))
            .or_else(|| inner.renamed_functions.get(&address).cloned())
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
    let decompiler_options = crate::commands::workspace::settings::get_settings(app_handle)
        .await
        .ok()
        .and_then(|settings| settings.decompiler_options)
        .unwrap_or_default();

    let timeout_ms = decompiler_options.performance.timeout_ms.max(1);
    let binary_for_job = binary.clone();
    let func_name_for_job = func_name.clone();
    let options_for_job = decompiler_options.clone();

    let job = tokio::task::spawn_blocking(move || {
        decompile_rust_only(binary_for_job.as_ref(), address, &func_name_for_job, options_for_job)
    });

    let decomp_result = match tokio::time::timeout(Duration::from_millis(timeout_ms), job).await {
        Ok(joined) => match joined {
            Ok(result) => result,
            Err(e) => Err(CmdError::other(format!("Decompile task failed: {e}"))),
        },
        Err(_) => {
            return Ok(DecompileResult {
                code: format!(
                    "// Decompilation timed out\n// Function: {}\n// Address: 0x{:x}\n",
                    func_name, address
                ),
                function_name: func_name,
                address: format!("0x{:x}", address),
                engine_used: DecompilerEngineMode::Nir,
                fell_back: true,
                fallback_reason: Some(fallback_reason_with_kind(
                    "preview_timeout",
                    format!("decompilation exceeded {timeout_ms}ms"),
                )),
            });
        }
    };

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
            engine_used: DecompilerEngineMode::Nir,
            fell_back: true,
            fallback_reason: Some(fallback_reason_with_kind("rust_decomp_failure", e.to_string())),
        }),
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
