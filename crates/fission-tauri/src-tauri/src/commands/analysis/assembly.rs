//! Disassembly and decompilation commands.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
#[cfg(feature = "native_decomp")]
use fission_loader::loader::LoadedBinary;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

#[cfg(feature = "native_decomp")]
fn pcode_total_ops(pcode: &fission_pcode::PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

#[cfg(feature = "native_decomp")]
fn max_multiequal_fanin(pcode: &fission_pcode::PcodeFunction) -> usize {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .filter(|op| op.opcode == fission_pcode::PcodeOpcode::MultiEqual)
        .map(|op| op.inputs.len())
        .max()
        .unwrap_or(0)
}

#[cfg(feature = "native_decomp")]
fn contains_indirect_control_flow(pcode: &fission_pcode::PcodeFunction) -> bool {
    pcode.blocks.iter().flat_map(|block| block.ops.iter()).any(|op| {
        matches!(
            op.opcode,
            fission_pcode::PcodeOpcode::CallInd | fission_pcode::PcodeOpcode::BranchInd
        )
    })
}

#[cfg(feature = "native_decomp")]
fn auto_mlil_eligible(binary: &LoadedBinary, pcode: &fission_pcode::PcodeFunction) -> bool {
    binary.is_64bit
        && binary.format.eq_ignore_ascii_case("PE")
        && pcode.blocks.len() <= 8
        && pcode_total_ops(pcode) <= 400
        && !contains_indirect_control_flow(pcode)
        && max_multiequal_fanin(pcode) <= 4
}

#[cfg(feature = "native_decomp")]
struct DecompileOutcome {
    code: String,
    engine_used: DecompilerEngineMode,
    fell_back: bool,
    fallback_reason: Option<String>,
}

#[cfg(feature = "native_decomp")]
fn try_render_mlil_preview(
    decomp: &mut fission_analysis::analysis::decomp::CachingDecompiler,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
) -> Result<Option<String>, CmdError> {
    let pcode_json = decomp
        .inner_mut()
        .get_pcode(address)
        .map_err(|e| CmdError::other(format!("{e}")))?;
    let mut pcode = fission_pcode::PcodeFunction::from_json(&pcode_json)
        .map_err(|e| CmdError::other(format!("mlil-preview pcode parse failed: {e}")))?;
    if enforce_auto_gate && !auto_mlil_eligible(binary, &pcode) {
        return Ok(None);
    }
    let mut optimizer =
        fission_pcode::PcodeOptimizer::new(fission_pcode::PcodeOptimizerConfig::default());
    let _ = optimizer.optimize(&mut pcode);
    let options = fission_pcode::MlilPreviewOptions::from_loaded_binary(binary);
    let code = fission_pcode::render_mlil_preview(&pcode, name, address, &options)
        .map_err(|e| CmdError::other(format!("mlil-preview unavailable: {e}")))?;
    Ok(Some(code))
}

#[cfg(feature = "native_decomp")]
fn decompile_with_engine(
    decomp: &mut fission_analysis::analysis::decomp::CachingDecompiler,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    engine_mode: DecompilerEngineMode,
) -> Result<DecompileOutcome, CmdError> {
    match engine_mode {
        DecompilerEngineMode::Legacy => decomp
            .decompile(address)
            .map(|code| DecompileOutcome {
                code,
                engine_used: DecompilerEngineMode::Legacy,
                fell_back: false,
                fallback_reason: None,
            })
            .map_err(|e| CmdError::other(format!("{e}"))),
        DecompilerEngineMode::MlilPreview => match try_render_mlil_preview(
            decomp, binary, address, name, false,
        ) {
            Ok(Some(code)) => Ok(DecompileOutcome {
                code,
                engine_used: DecompilerEngineMode::MlilPreview,
                fell_back: false,
                fallback_reason: None,
            }),
            Ok(None) => decomp
                .decompile(address)
                .map(|code| DecompileOutcome {
                    code,
                    engine_used: DecompilerEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(
                        "mlil-preview skipped: function not supported by preview builder"
                            .to_string(),
                    ),
                })
                .map_err(|e| CmdError::other(format!("{e}"))),
            Err(preview_err) => decomp
                .decompile(address)
                .map(|code| DecompileOutcome {
                    code,
                    engine_used: DecompilerEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(preview_err.to_string()),
                })
                .map_err(|e| CmdError::other(format!("{e}"))),
        },
        DecompilerEngineMode::Auto => match try_render_mlil_preview(
            decomp, binary, address, name, true,
        ) {
            Ok(Some(code)) => Ok(DecompileOutcome {
                code,
                engine_used: DecompilerEngineMode::MlilPreview,
                fell_back: false,
                fallback_reason: None,
            }),
            Ok(None) => decomp
                .decompile(address)
                .map(|code| DecompileOutcome {
                    code,
                    engine_used: DecompilerEngineMode::Legacy,
                    fell_back: false,
                    fallback_reason: None,
                })
                .map_err(|e| CmdError::other(format!("{e}"))),
            Err(preview_err) => decomp
                .decompile(address)
                .map(|code| DecompileOutcome {
                    code,
                    engine_used: DecompilerEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(preview_err.to_string()),
                })
                .map_err(|e| CmdError::other(format!("{e}"))),
        },
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
            .and_then(|decomp| decompile_with_engine(decomp, binary.as_ref(), address, &func_name, engine_mode));
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
