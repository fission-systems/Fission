//! File operations - Binary loading, server connection, recovery.

use std::fs;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

use crate::analysis::decomp::client::{GhidraClient, BinaryId};
use crate::analysis::decomp::client::ghidra_service::FunctionMeta;
use crate::analysis::loader::{LoadedBinary, FunctionInfo};
use crate::ui::gui::state::AppState;
use crate::ui::gui::messages::AsyncMessage;

use super::TOKIO_RUNTIME;

/// Open native file dialog to select a binary
pub fn open_file_dialog(tx: Sender<AsyncMessage>) {
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .set_title("Open Binary")
            .add_filter("Executables", &["exe", "dll", "so", "dylib", "bin"])
            .add_filter("All Files", &["*"])
            .pick_file();
        
        let path = file.map(|p| p.to_string_lossy().to_string());
        let _ = tx.send(AsyncMessage::FileSelected(path));
    });
}

/// Load a binary file
pub fn load_binary(state: &mut AppState, tx: Sender<AsyncMessage>, path: &str) {
    let path = path.to_string();
    
    // Clear cache on new binary load
    state.decompile_cache.clear();
    // Save path for recovery reload
    state.last_binary_path = Some(path.clone());
    
    state.log(format!("[*] Loading {}...", path));
    
    std::thread::spawn(move || {
        match LoadedBinary::from_file(&path) {
            Ok(binary) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(binary))); }
            Err(e) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Err(e.to_string()))); }
        }
    });
}

/// Ensure server has the current binary loaded and cache functions from server metadata.
pub fn preload_server_binary(state: &mut AppState, ghidra_client: Arc<Mutex<Option<GhidraClient>>>) {
    let Some(binary) = state.loaded_binary.as_ref() else {
        return;
    };

    let arch = binary.arch_spec.clone();
    let bin_size = binary.data.len() as u64;
    let bin_mtime = fs::metadata(&binary.path).ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let bin_id = BinaryId::new(Some(binary.path.clone()), bin_size, arch.clone(), bin_mtime);
    let bin_bytes = binary.data.clone();
    let bin_base = binary.image_base;

    let shared_client = ghidra_client;
    let funcs = TOKIO_RUNTIME.block_on(async move {
        let mut guard = shared_client.lock().unwrap();
        if guard.is_none() {
            *guard = connect_with_backoff().await;
        }
        let Some(client) = guard.as_mut() else { return None; };
        match client.load_binary_if_needed(bin_bytes, bin_base, &arch, bin_id).await {
            Ok((_, metas)) => Some(metas.to_vec()),
            Err(_) => None,
        }
    });

    if let Some(server_funcs) = funcs {
        if !server_funcs.is_empty() {
            let converted: Vec<FunctionInfo> = server_funcs.into_iter().map(convert_meta).collect();
            state.loaded_binary.as_mut().map(|b| b.functions = converted);
        }
    }
}

/// Convert server FunctionMeta to FunctionInfo
pub fn convert_meta(m: FunctionMeta) -> FunctionInfo {
    FunctionInfo {
        name: m.name,
        address: m.address,
        size: m.size as u64,
        is_export: false,
        is_import: m.is_import,
    }
}

/// Connect with backoff retry
pub async fn connect_with_backoff() -> Option<GhidraClient> {
    let delays = [Duration::from_millis(0), Duration::from_millis(200), Duration::from_millis(500)];
    for d in delays {
        if d.as_millis() > 0 {
            sleep(d).await;
        }
        if let Ok(c) = GhidraClient::connect().await {
            return Some(c);
        }
    }
    None
}

/// Attempt to recover server connection with exponential backoff
pub fn attempt_server_recovery(state: &mut AppState, tx: Sender<AsyncMessage>) {
    if state.recovering {
        return; // Already recovering
    }
    
    state.recovering = true;
    
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Max 3 retries with exponential backoff (1s, 2s, 4s)
            for attempt in 0..3 {
                let wait_time = Duration::from_secs(1 << attempt);
                tokio::time::sleep(wait_time).await;
                
                match GhidraClient::connect().await {
                    Ok(_client) => {
                        let _ = tx.send(AsyncMessage::ServerRecovered);
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(AsyncMessage::ServerStatus(false));
                        if attempt == 2 {
                            let _ = tx.send(AsyncMessage::RecoveryFailed(e.to_string()));
                        }
                    }
                }
            }
        });
    });
}

