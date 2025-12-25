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
