//! Decompiler operations - Function decompilation with caching.

use std::fs;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::analysis::decomp::client::{GhidraClient, BinaryId};
use crate::analysis::disasm::DisasmEngine;
use crate::analysis::loader::FunctionInfo;
use crate::ui::gui::state::{AppState, CachedDecompile};
use crate::ui::gui::messages::AsyncMessage;

use super::TOKIO_RUNTIME;
use super::file_ops::connect_with_backoff;

/// Decompile a function
pub fn decompile_function(
    state: &mut AppState,
    tx: Sender<AsyncMessage>,
    ghidra_client: Arc<Mutex<Option<GhidraClient>>>,
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
    
    let _start_time = Instant::now();

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
    
    let (arch, bin_id, bin_bytes, bin_base, bytes, is_64bit) = {
        let binary = state.loaded_binary.as_ref().unwrap();
        let arch = binary.arch_spec.clone();
        let bin_size = binary.data.len() as u64;
        let bin_mtime = fs::metadata(&binary.path).ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let bin_id = BinaryId::new(Some(binary.path.clone()), bin_size, arch.clone(), bin_mtime);
        let bin_bytes = binary.data.clone();
        let bin_base = binary.image_base;
        
        // Get function bytes (estimate 4KB for function body)
        let func_size = if func.size > 0 { func.size as usize } else { 4096 };
        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        (arch, bin_id, bin_bytes, bin_base, bytes, binary.is_64bit)
    };
    
    // Disassemble bytes
    let _disasm_start = Instant::now();
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
    
    // Spawn async task for decompilation
    let shared_client = ghidra_client;
    let bin_bytes_clone = bin_bytes.clone();
    let bin_id_clone = bin_id.clone();
    let arch_clone = arch.clone();
    let handle = TOKIO_RUNTIME.handle().clone();
    std::thread::spawn(move || {
        handle.block_on(async {

            let mut guard = shared_client.lock().unwrap();

            // Try reuse; if missing or failed ensure, reconnect with short backoff
            let mut need_new = guard.is_none();
            if !need_new {
                if let Some(client) = guard.as_mut() {
                    if client.ensure_connected().await.is_err() {
                        need_new = true;
                    }
                }
            }

            if need_new {
                let (prev_id, prev_funcs) = guard
                    .as_ref()
                    .map(|c| c.snapshot_state())
                    .unwrap_or((None, Vec::new()));

                let mut new_client = None;
                let delays = [Duration::from_millis(0), Duration::from_millis(200), Duration::from_millis(500)];
                for d in delays {
                    if d.as_millis() > 0 {
                        sleep(d).await;
                    }
                    match GhidraClient::connect().await {
                        Ok(mut c) => {
                            c.restore_state(prev_id.clone(), prev_funcs.clone());
                            new_client = Some(c);
                            break;
                        }
                        Err(_) => continue,
                    }
                }

                if let Some(c) = new_client {
                    *guard = Some(c);
                } else {
                    let _ = tx.send(AsyncMessage::DecompileError { 
                        address, 
                        error: "Server reconnection failed".to_string() 
                    });
                    return;
                }
            }

            let client = guard.as_mut().unwrap();

            // Load the binary bytes only if needed
            if let Err(e) = match client.load_binary_if_needed(bin_bytes_clone.clone(), bin_base, &arch_clone, bin_id_clone.clone()).await {
                Ok(_) => Ok(()),
                Err(err) => Err(err),
            } {
                let _ = tx.send(AsyncMessage::DecompileError { 
                    address, 
                    error: e.to_string() 
                });
                return;
            }
            
            // Decompile
            match client.decompile_function(address).await {
                Ok(result) => {
                    let _ = tx.send(AsyncMessage::DecompileResult { 
                        address, 
                        c_code: result.c_code 
                    });
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::DecompileError { 
                        address, 
                        error: e.to_string() 
                    });
                }
            }
        });
    });
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

