//! Decompiler operations - Function decompilation using native FFI.

use std::fs;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::analysis::disasm::DisasmEngine;
use crate::analysis::loader::FunctionInfo;
use crate::ui::gui::state::{AppState, CachedDecompile};
use crate::ui::gui::messages::AsyncMessage;

/// Decompile a function
pub fn decompile_function(
    state: &mut AppState,
    tx: Sender<AsyncMessage>,
    native_decompiler: Arc<Mutex<Option<crate::analysis::decomp::NativeDecompiler>>>,
    func: &FunctionInfo,
) {
    // Skip import functions
    if func.is_import {
        state.log(format!("[!] {} is an import function (no code to decompile)", func.name));
        state.decompiled_code = format!(
            "// {} is an imported function\n// Address: 0x{:x}\n// No code available - this is a stub pointing to external library",
            func.name, func.address
        );
        return;
    }
    
    // Check cache first
    let address = func.address;
    if let Some(cached) = state.decompile_cache.get(&address) {
        let c_code = cached.c_code.clone();
        let asm = cached.asm_instructions.clone();
        state.log(format!("[*] Using cached result for 0x{:x}", address));
        state.decompiled_code = c_code;
        state.asm_instructions = asm;
        return;
    }
    
    if state.loaded_binary.is_none() {
        state.log("[!] No binary loaded");
        return;
    }
    
    let (_, _, _, _, bytes, is_64bit) = {
        let binary = state.loaded_binary.as_ref().unwrap();
        
        // Get function bytes (estimate 4KB for function body if size is 0)
        let mut func_size = if func.size > 0 { func.size as usize } else { 4096 };
        
        // Limit function size to not exceed section bounds
        // Find the section containing this address
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
        ("", (), (), (), bytes, binary.is_64bit)
    };
    
    // Disassemble bytes
    match DisasmEngine::new(is_64bit) {
        Ok(engine) => {
            match engine.disassemble(&bytes, address) {
                Ok(insns) => {
                    state.asm_instructions = insns;
                }
                Err(e) => {
                    state.log(format!("[!] Disassembly error: {}", e));
                    state.asm_instructions.clear();
                }
            }
        }
        Err(e) => {
            state.log(format!("[!] Failed to initialize disassembler: {}", e));
            state.asm_instructions.clear();
        }
    }

    state.decompiling = true;
    state.decompiled_code = format!("// Decompiling 0x{:x}...", address);
    state.log(format!("[*] Decompiling 0x{:x} ({} bytes)", address, bytes.len()));
    
    // Use Native FFI
    {
        let mut native_guard = native_decompiler.lock().unwrap();
        if native_guard.is_none() {
            // Attempt to load library if not already initialized
            if let Some(lib_path) = crate::analysis::decomp::native::find_library() {
                let sla_dir = std::env::current_dir().unwrap().join("ghidra_decompiler").to_string_lossy().into_owned();
                match crate::analysis::decomp::NativeDecompiler::new(lib_path, &sla_dir) {
                    Ok(nd) => {
                        state.log("[✓] Native decompiler initialized");
                        *native_guard = Some(nd);
                    }
                    Err(e) => {
                        state.log(format!("[✗] Native decompiler init failed: {}", e));
                    }
                }
            }
        }

        if let Some(nd) = native_guard.as_ref() {
            match nd.decompile(&bytes, address, is_64bit) {
                Ok(c_code) => {
                    cache_decompile_result(state, address, c_code.clone());
                    state.log("[✓] Decompile successful");
                }
                Err(e) => {
                    state.log(format!("[✗] Decompile failed: {}", e));
                    state.decompiled_code = format!("// Decompile error: {}", e);
                    state.decompiling = false;
                }
            }
        } else {
            state.log("[!] Native decompiler not available");
            state.decompiled_code = "// Native decompiler not available".to_string();
            state.decompiling = false;
        }
    }
}

/// Store decompile result in cache
pub fn cache_decompile_result(state: &mut AppState, address: u64, c_code: String) {
    if let Some(func) = &state.selected_function {
        if func.address == address {
            state.decompile_cache.insert(address, CachedDecompile {
                c_code: c_code.clone(),
                asm_instructions: state.asm_instructions.clone(),
                timestamp: Instant::now(),
            });
        }
    }
    state.decompiled_code = c_code;
    state.decompiling = false;
}
