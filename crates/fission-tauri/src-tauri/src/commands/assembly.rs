//! Disassembly and decompilation commands.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use tauri::State;

// ============================================================================
// Commands
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
            .and_then(|decomp| {
                decomp
                    .decompile(address)
                    .map_err(|e| CmdError::other(format!("{e}")))
            });
        drop(decomp_lock);

        return match decomp_result {
            Ok(code) => Ok(DecompileResult {
                code,
                function_name: func_name,
                address: format!("0x{:x}", address),
            }),
            Err(e) => Ok(DecompileResult {
                code: format!(
                    "// Decompilation failed: {}\n// Function: {}\n// Address: 0x{:x}\n",
                    e, func_name, address
                ),
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
