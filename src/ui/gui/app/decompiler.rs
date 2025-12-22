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
    
    let (_, _, _, _, bytes, is_64bit) = {
        let binary = state.analysis.loaded_binary.as_ref().unwrap();
        
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
    
    // Clone data needed for background thread
    let tx_clone = tx.clone();
    let bytes_clone = bytes.clone();
    let native_decompiler_clone = Arc::clone(&native_decompiler);
    
    // Spawn background thread for decompilation (prevents GUI freezing)
    std::thread::spawn(move || {
        // Initialize decompiler if needed
        let mut native_guard = native_decompiler_clone.lock().unwrap();
        if native_guard.is_none() {
            if let Some(cli_path) = crate::analysis::decomp::native::find_library() {
                let sla_dir = std::env::current_dir()
                    .unwrap()
                    .join("ghidra_decompiler")
                    .to_string_lossy()
                    .into_owned();
                match crate::analysis::decomp::NativeDecompiler::new(cli_path, &sla_dir) {
                    Ok(nd) => {
                        *native_guard = Some(nd);
                    }
                    Err(e) => {
                        let _ = tx_clone.send(AsyncMessage::DecompileError {
                            address,
                            error: format!("Failed to init decompiler: {}", e),
                        });
                        return;
                    }
                }
            } else {
                let _ = tx_clone.send(AsyncMessage::DecompileError {
                    address,
                    error: "Decompiler CLI not found".to_string(),
                });
                return;
            }
        }

        // Perform decompilation
        if let Some(nd) = native_guard.as_ref() {
            match nd.decompile(&bytes_clone, address, is_64bit) {
                Ok(c_code) => {
                    let _ = tx_clone.send(AsyncMessage::DecompileResult {
                        address,
                        c_code,
                    });
                }
                Err(e) => {
                    let _ = tx_clone.send(AsyncMessage::DecompileError {
                        address,
                        error: e.to_string(),
                    });
                }
            }
        }
    });
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
fn apply_iat_symbols(code: &str, iat_symbols: &std::collections::HashMap<u64, String>) -> String {
    use std::collections::HashMap;
    
    if iat_symbols.is_empty() {
        return code.to_string();
    }
    
    let mut result = code.to_string();
    
    // Build replacement map: pcRam00403050 -> MessageBoxA
    let mut replacements: HashMap<String, String> = HashMap::new();
    for (addr, name) in iat_symbols {
        // Ghidra uses pcRamXXXXXXXX format for memory pointers
        let pcram_pattern = format!("pcRam{:08x}", addr);
        replacements.insert(pcram_pattern.clone(), name.clone());
        
        // Also try uppercase variant
        let pcram_upper = format!("pcRam{:08X}", addr);
        replacements.insert(pcram_upper, name.clone());
        
        // Also handle func_0xXXXXXXXX patterns
        let func_pattern = format!("func_0x{:08x}", addr);
        replacements.insert(func_pattern.clone(), name.clone());
    }
    
    // Apply replacements
    for (pattern, replacement) in &replacements {
        result = result.replace(pattern, replacement);
    }
    
    result
}
