//! Decompiler operations - Function decompilation using native FFI.

use std::sync::mpsc::Sender;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::analysis::disasm::DisasmEngine;
use crate::analysis::loader::FunctionInfo;
use crate::ui::gui::state::{AppState, CachedDecompile};
use super::decomp_worker::DecompileRequest;

/// Decompile a function (sends request to worker thread)
pub fn decompile_function(
    state: &mut AppState,
    decomp_tx: &Sender<DecompileRequest>,
    latest_request_id: &Arc<AtomicU64>,
    func: &FunctionInfo,
) {
    // Skip import functions
    if func.is_import {
        state.log(format!("[!] {} is an import function (no code to decompile)", func.name));
        state.analysis.decompiled_code = format!(
            "// {} is an imported function\n// Address: 0x{:x}\n// No code available - this is a stub pointing to external library",
            func.name, func.address
        );
        return;
    }
    
    // Check cache first
    let address = func.address;
    if let Some(cached) = state.analysis.decompile_cache.get(&address) {
        let c_code = cached.c_code.clone();
        let asm = cached.asm_instructions.clone();
        state.log(format!("[*] Using cached result for 0x{:x}", address));
        state.analysis.decompiled_code = c_code;
        state.analysis.asm_instructions = asm;
        return;
    }
    
    if state.analysis.loaded_binary.is_none() {
        state.log("[!] No binary loaded");
        return;
    }
    
    let (bytes, is_64bit) = {
        let binary = state.analysis.loaded_binary.as_ref().unwrap();
        
        // Get function bytes (estimate 4KB for function body if size is 0)
        let mut func_size = if func.size > 0 { func.size as usize } else { 4096 };
        
        // Limit function size to not exceed section bounds
        for section in &binary.sections {
            if section.is_executable 
                && address >= section.virtual_address 
                && address < section.virtual_address + section.virtual_size as u64 
            {
                let max_size = (section.virtual_address + section.virtual_size as u64 - address) as usize;
                func_size = func_size.min(max_size);
                break;
            }
        }
        
        // Ensure minimum size of 16 bytes, max of 64KB
        func_size = func_size.max(16).min(65536);
        
        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        (bytes, binary.is_64bit)
    };
    
    // Disassemble bytes (synchronous, fast)
    match DisasmEngine::new(is_64bit) {
        Ok(engine) => {
            match engine.disassemble(&bytes, address) {
                Ok(insns) => {
                    state.analysis.asm_instructions = insns;
                }
                Err(e) => {
                    state.log(format!("[!] Disassembly error: {}", e));
                    state.analysis.asm_instructions.clear();
                }
            }
        }
        Err(e) => {
            state.log(format!("[!] Failed to initialize disassembler: {}", e));
            state.analysis.asm_instructions.clear();
        }
    }

    state.analysis.decompiling = true;
    state.analysis.decompiled_code = format!("// Decompiling 0x{:x}...", address);
    state.log(format!("[*] Decompiling 0x{:x} ({} bytes)", address, bytes.len()));
    
    // Generate new request ID (for debouncing)
    let request_id = latest_request_id.fetch_add(1, Ordering::SeqCst) + 1;
    latest_request_id.store(request_id, Ordering::SeqCst);
    
    // Send request to worker thread (non-blocking)
    let request = DecompileRequest {
        request_id,
        bytes,
        address,
        is_64bit,
    };
    
    if let Err(e) = decomp_tx.send(request) {
        state.log(format!("[!] Failed to send decompile request: {}", e));
        state.analysis.decompiling = false;
    }
}

/// Store decompile result in cache
pub fn cache_decompile_result(state: &mut AppState, address: u64, c_code: String) {
    // Apply IAT symbol replacement if binary is loaded
    let processed_code = if let Some(ref binary) = state.analysis.loaded_binary {
        apply_iat_symbols(&c_code, &binary.iat_symbols)
    } else {
        c_code.clone()
    };
    
    if let Some(func) = &state.analysis.selected_function {
        if func.address == address {
            state.analysis.decompile_cache.insert(address, CachedDecompile {
                c_code: processed_code.clone(),
                asm_instructions: state.analysis.asm_instructions.clone(),
                timestamp: Instant::now(),
            });
        }
    }
    state.analysis.decompiled_code = processed_code;
    state.analysis.decompiling = false;
}

/// Replace pcRamXXXXXXXX patterns with actual IAT symbol names
/// Uses regex for O(N) complexity instead of O(N*M)
fn apply_iat_symbols(code: &str, iat_symbols: &std::collections::HashMap<u64, String>) -> String {
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    if iat_symbols.is_empty() {
        return code.to_string();
    }
    
    // Regex patterns for Ghidra memory references
    // pcRam00403050 or pcRam00403050 (lowercase/uppercase hex)
    static PCRAM_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"pcRam([0-9a-fA-F]{8})").unwrap()
    });
    
    // func_0x00403050 pattern
    static FUNC_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"func_0x([0-9a-fA-F]{8})").unwrap()
    });
    
    // Single pass replacement for pcRam patterns
    let result = PCRAM_RE.replace_all(code, |caps: &regex::Captures| {
        let addr_str = &caps[1];
        if let Ok(addr) = u64::from_str_radix(addr_str, 16) {
            if let Some(name) = iat_symbols.get(&addr) {
                return name.clone();
            }
        }
        caps[0].to_string()
    });
    
    // Single pass replacement for func_ patterns
    let result = FUNC_RE.replace_all(&result, |caps: &regex::Captures| {
        let addr_str = &caps[1];
        if let Ok(addr) = u64::from_str_radix(addr_str, 16) {
            if let Some(name) = iat_symbols.get(&addr) {
                return name.clone();
            }
        }
        caps[0].to_string()
    });
    
    result.into_owned()
}
