//! Decompiler operations - Function decompilation using native FFI.

use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
// use std::time::Instant;

use super::decomp_worker::DecompileRequest;
use crate::analysis::disasm::DisasmEngine;
use crate::core::config::CONFIG;
// use crate::ui::gui::core::domain::CachedDecompile;
use crate::ui::gui::core::state::AppState;
use fission_loader::loader::FunctionInfo;

/// Decompile a function (sends request to worker thread)
pub fn decompile_function(
    state: &mut AppState,
    decomp_tx: &Sender<DecompileRequest>,
    latest_request_id: &Arc<AtomicU64>,
    func: &FunctionInfo,
) {
    // Skip import functions
    if func.is_import {
        state.log(format!(
            "[!] {} is an import function (no code to decompile)",
            func.name
        ));
        state.analysis.domain.decompiled_code = format!(
            "// {} is an imported function\n// Address: 0x{:x}\n// No code available - this is a stub pointing to external library",
            func.name, func.address
        );
        return;
    }

    // Persistant caching is now handled at the analysis engine level
    // in CachingDecompiler. We no longer need to check cache here.

    // CRITICAL: Check if binary is loaded AND decompiler context is ready
    if state.analysis.domain.loaded_binary.as_ref().is_none() {
        state.log("[!] No binary loaded".to_string());
        state.analysis.domain.decompiled_code =
            "// No binary loaded\n// Use File → Open to load a binary".to_string();
        return;
    }

    // Extra safety: Check decompiler context is loaded (prevents race conditions)
    if !state.analysis.domain.decompiler_context_loaded {
        state.log("[!] Decompiler context not ready".to_string());
        state.analysis.domain.decompiled_code =
            "// Decompiler initializing...\n// Please wait a moment and try again".to_string();
        return;
    }

    let address = func.address;
    let (bytes, is_64bit, binary_hash) = {
        let binary = match state.analysis.domain.loaded_binary.as_ref() {
            Some(b) => b,
            None => {
                state.analysis.domain.decompiled_code = "// Error: No binary loaded".to_string();
                return;
            }
        };

        // Get function bytes (use config default if size is unknown)
        let mut func_size = if func.size > 0 {
            func.size as usize
        } else {
            CONFIG.decompiler.default_function_size
        };

        // Limit function size to not exceed section bounds
        for section in &binary.sections {
            if section.is_executable
                && address >= section.virtual_address
                && address < section.virtual_address + section.virtual_size
            {
                let max_size = (section.virtual_address + section.virtual_size - address) as usize;
                func_size = func_size.min(max_size);
                break;
            }
        }

        // Clamp to configured min/max sizes
        func_size = func_size
            .max(CONFIG.decompiler.min_function_size)
            .min(CONFIG.decompiler.max_function_size);

        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        (bytes, binary.is_64bit, binary.hash.clone())
    };

    // Disassemble bytes (synchronous, fast)
    match DisasmEngine::new(is_64bit) {
        Ok(engine) => match engine.disassemble(&bytes, address) {
            Ok(insns) => {
                state.analysis.domain.asm_instructions = insns;
            }
            Err(e) => {
                state.log(format!("[!] Disassembly error: {}", e));
                state.analysis.domain.asm_instructions.clear();
            }
        },
        Err(e) => {
            state.log(format!("[!] Failed to initialize disassembler: {}", e));
            state.analysis.domain.asm_instructions.clear();
        }
    }

    state.analysis.domain.decompiling = true;
    state.analysis.domain.decompiled_code = format!("// Decompiling 0x{:x}...", address);
    state.log(format!(
        "[*] Decompiling 0x{:x} ({} bytes)",
        address,
        bytes.len()
    ));

    // Generate new request ID (for debouncing)
    let request_id = latest_request_id.fetch_add(1, Ordering::SeqCst) + 1;
    latest_request_id.store(request_id, Ordering::SeqCst);

    // Send request to worker thread (non-blocking)
    // Send request to worker thread (non-blocking)
    // Optimization: If decompiler context is loaded, send empty bytes to use persistent memory
    let request_bytes = if state.analysis.domain.decompiler_context_loaded {
        Vec::new()
    } else {
        bytes.to_vec()
    };

    let request = DecompileRequest {
        request_id,
        bytes: request_bytes,
        address,
        is_64bit,
        is_prefetch: false,
        is_binary_load: false,
        image_base: 0,
        iat_symbols: std::collections::HashMap::new(),
        global_symbols: std::collections::HashMap::new(),
        functions: Vec::new(),
        gdt_json_path: None,
        sections: Vec::new(),
        binary_hash,
        is_cfg_request: false,
        is_clear_cache: false,
    };

    if let Err(e) = decomp_tx.send(request) {
        state.log(format!("[!] Failed to send decompile request: {}", e));
        state.analysis.domain.decompiling = false;
    }
}

/// Store decompile result in UI state
pub fn cache_decompile_result(state: &mut AppState, _address: u64, c_code: String) {
    // Apply IAT symbol replacement if binary is loaded
    let processed_code = if let Some(ref binary) = state.analysis.domain.loaded_binary {
        apply_iat_symbols(&c_code, &binary.iat_symbols)
    } else {
        c_code.clone()
    };

    state.analysis.domain.decompiled_code = processed_code;
    state.analysis.domain.decompiling = false;
}

/// Replace pcRamXXXXXXXX and func_0xXXXXXXXX patterns with actual IAT symbol names
/// Uses combined regex for single-pass O(N) complexity
fn apply_iat_symbols(code: &str, iat_symbols: &std::collections::HashMap<u64, String>) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    if iat_symbols.is_empty() {
        return code.to_string();
    }

    // Combined regex pattern for both pcRam and func_0x patterns
    // Matches: pcRam00403050 or func_0x00403050
    static COMBINED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:pcRam|func_0x)([0-9a-fA-F]{8})").unwrap());

    // Single pass replacement for both patterns
    let result = COMBINED_RE.replace_all(code, |caps: &regex::Captures| {
        let addr_str = &caps[1];
        if let Ok(addr) = u64::from_str_radix(addr_str, 16)
            && let Some(name) = iat_symbols.get(&addr)
        {
            return name.clone();
        }
        caps[0].to_string()
    });

    result.into_owned()
}
