//! Message and command handlers.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::analysis::decomp::client::GhidraClient;
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
    ghidra_client: Arc<Mutex<Option<GhidraClient>>>,
    #[cfg(target_os = "windows")]
    dbg_event_rx: &Option<std::sync::mpsc::Receiver<crate::debug::types::DebugEvent>>,
) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AsyncMessage::BinaryLoaded(Ok(binary)) => {
                state.log(format!("[✓] Loaded: {}", binary.path));
                state.log(format!("    {} {} | Entry: 0x{:x}", 
                    if binary.is_64bit { "64-bit" } else { "32-bit" },
                    binary.format,
                    binary.entry_point));
                state.log(format!("    {} functions found", binary.functions.len()));
                state.loaded_binary = Some(binary);
                file_ops::preload_server_binary(state, ghidra_client.clone());
            }
            AsyncMessage::BinaryLoaded(Err(e)) => {
                state.log(format!("[✗] Failed to load binary: {}", e));
            }
            AsyncMessage::DecompileResult { address, c_code } => {
                decompiler::cache_decompile_result(state, address, c_code.clone());
                state.log(format!("[✓] Decompiled 0x{:x} (cached)", address));
            }
            AsyncMessage::DecompileError { address: _, error } => {
                state.decompiled_code = format!("// Error: {}", error);
                state.decompiling = false;
                state.log(format!("[✗] Decompile error: {}", error));
                
                // Check if this is a connection error
                if error.contains("transport") || error.contains("connection") {
                    let _ = tx.send(AsyncMessage::ServerDisconnected);
                }
            }
            AsyncMessage::ServerStatus(connected) => {
                state.server_connected = connected;
            }
            AsyncMessage::FileSelected(Some(path)) => {
                file_ops::load_binary(state, tx.clone(), &path);
            }
            AsyncMessage::FileSelected(None) => {
                // User cancelled
            }
            AsyncMessage::ServerDisconnected => {
                state.server_connected = false;
                state.log("[!] Server disconnected. Attempting recovery...");
                file_ops::attempt_server_recovery(state, tx.clone());
            }
            AsyncMessage::ServerRecovered => {
                state.server_connected = true;
                state.recovering = false;
                state.log("[✓] Server reconnected successfully");
                
                // Reload binary if we had one loaded
                if let Some(path) = state.last_binary_path.clone() {
                    state.log("[*] Reloading binary...");
                    file_ops::load_binary(state, tx.clone(), &path);
                }
            }
            AsyncMessage::RecoveryFailed(reason) => {
                state.recovering = false;
                state.log(format!("[✗] Server recovery failed: {}", reason));
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
            if let Some(ref binary) = state.loaded_binary {
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

