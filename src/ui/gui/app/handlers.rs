//! Message and command handlers.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::ui::gui::state::AppState;
use crate::ui::gui::messages::AsyncMessage;

use super::debug_ops;
use super::file_ops;
use super::decompiler;

/// Process pending async messages from background threads
pub fn process_messages(
    state: &mut AppState,
    rx: &Receiver<AsyncMessage>,
    tx: &Sender<AsyncMessage>,
    native_decompiler: Arc<Mutex<Option<crate::analysis::decomp::NativeDecompiler>>>,
    #[cfg(target_os = "windows")]
    dbg_event_rx: &Option<std::sync::mpsc::Receiver<crate::debug::types::DebugEvent>>,
) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AsyncMessage::BinaryLoaded(Ok(binary)) => {
                // Note: Internal function discovery now disabled for fast loading
                // Can be triggered separately via "Analyze" button
                
                state.log(format!("[✓] Loaded: {}", binary.path));
                state.log(format!("    {} {} | Entry: 0x{:x}", 
                    if binary.is_64bit { "64-bit" } else { "32-bit" },
                    binary.format,
                    binary.entry_point));
                state.log(format!("    {} functions found", binary.functions.len()));
                state.analysis.loaded_binary = Some(binary);
                file_ops::preload_server_binary(state, native_decompiler.clone());
            }
            AsyncMessage::BinaryLoaded(Err(e)) => {
                state.log(format!("[✗] Failed to load binary: {}", e));
                state.log("    → Ensure the file is a valid PE/ELF/Mach-O executable".to_string());
            }
            AsyncMessage::DecompileResult { address, c_code } => {
                decompiler::cache_decompile_result(state, address, c_code.clone());
                state.log(format!("[✓] Decompiled 0x{:x} (cached)", address));
            }
            AsyncMessage::DecompileError { address: _, error } => {
                state.analysis.decompiled_code = format!("// Decompilation failed\n// Error: {}\n\n// Possible causes:\n// - Function may not exist at this address\n// - fission_decomp CLI may not be built\n// - Try running: cd ghidra_decompiler/build && cmake .. && make", error);
                state.analysis.decompiling = false;
                state.log(format!("[✗] Decompile error: {}", error));
                state.log("    → Check if ghidra_decompiler/build/fission_decomp exists".to_string());
            }
            AsyncMessage::FileSelected(Some(path)) => {
                file_ops::load_binary(state, tx.clone(), &path);
            }
            AsyncMessage::FileSelected(None) => {
                // User cancelled
            }
            AsyncMessage::DebugEvent(evt) => {
                debug_ops::handle_debug_event(state, evt);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut pending = Vec::new();
        if let Some(rx) = dbg_event_rx {
            while let Ok(evt) = rx.try_recv() {
                pending.push(evt);
            }
        }
        for evt in pending {
            debug_ops::handle_debug_event(state, evt);
        }
    }
}

/// Process a CLI command
pub fn process_command(
    state: &mut AppState,
    tx: Sender<AsyncMessage>,
    cmd: &str,
) {
    match cmd {
        "help" | "?" => {
            state.log("Available commands:");
            state.log("  load <path>  : Load a binary for analysis");
            state.log("  funcs        : List functions");
            state.log("  clear        : Clear console");
            state.log("  exit         : Quit Fission");
        }
        "funcs" | "functions" => {
            if let Some(ref binary) = state.analysis.loaded_binary {
                let funcs: Vec<_> = binary.functions.iter()
                    .map(|f| (f.address, f.name.clone()))
                    .collect();
                state.log(format!("[*] {} functions:", funcs.len()));
                for (addr, name) in funcs {
                    state.log(format!("  0x{:08x} {}", addr, name));
                }
            } else {
                state.log("[!] No binary loaded");
            }
        }
        "clear" => {
            state.clear_logs();
            state.log("[*] Console cleared");
        }
        "exit" | "quit" => {
            std::process::exit(0);
        }
        _ if cmd.starts_with("load ") => {
            let path = cmd.trim_start_matches("load ").trim();
            file_ops::load_binary(state, tx, path);
        }
        _ => {
            state.log(format!("[!] Unknown command: {}", cmd));
        }
    }
}

