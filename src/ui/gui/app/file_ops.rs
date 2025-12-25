//! File operations - Binary loading, native decompiler initialization.

use std::fs;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::analysis::loader::LoadedBinary;
use crate::ui::gui::state::AppState;
use crate::ui::gui::messages::AsyncMessage;

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
    state.analysis.decompile_cache.clear();
    // Save path
    state.analysis.last_binary_path = Some(path.clone());
    
    state.log(format!("[*] Loading {}...", path));
    
    std::thread::spawn(move || {
        match LoadedBinary::from_file(&path) {
            Ok(binary) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(std::sync::Arc::new(binary)))); }
            Err(e) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Err(e.to_string()))); }
        }
    });
}

/// Ensure native decompiler is initialized.
pub fn preload_server_binary(
    state: &mut AppState,
    native_decompiler: Arc<Mutex<Option<crate::analysis::decomp::NativeDecompiler>>>,
) {
    let Some(binary) = state.analysis.loaded_binary.as_ref() else {
        return;
    };

    // Initialize the native decompiler library
    {
        let mut native_guard = match native_decompiler.lock() {
            Ok(guard) => guard,
            Err(e) => {
                state.log(format!("[!] Native decompiler lock poisoned: {}", e));
                e.into_inner()
            }
        };
        
        if native_guard.is_none() {
            if let Some(lib_path) = crate::analysis::decomp::native::find_library() {
                // Determine SLA directory (usually same as library or relative to it)
                // For now, assume it's in the ghidra_decompiler directory in current path
                let sla_dir = std::env::current_dir()
                    .map(|p| p.join("ghidra_decompiler").to_string_lossy().into_owned())
                    .unwrap_or_else(|_| ".".to_string());
                
                match crate::analysis::decomp::NativeDecompiler::new(lib_path, &sla_dir) {
                    Ok(nd) => {
                        state.log("[✓] Native decompiler initialized");
                        *native_guard = Some(nd);
                    }
                    Err(e) => {
                        state.log(format!("[✗] Native decompiler init failed: {}", e));
                        state.log("    → Try: cd ghidra_decompiler/build && cmake .. && make".to_string());
                    }
                }
            } else {
                state.log("[!] Native decompiler not found".to_string());
                state.log("    → Build with: cd ghidra_decompiler/build && cmake .. && make".to_string());
                state.log("    → Expected: fission_decomp executable".to_string());
            }
        }
    }
}
