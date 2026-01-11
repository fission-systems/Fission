//! File operations - Binary loading, native decompiler initialization.

use crossbeam_channel::Sender;
use std::path::Path;
use std::sync::Arc;

use crate::ui::gui::core::messages::AsyncMessage;
use crate::ui::gui::core::state::AppState;
use fission_loader::loader::LoadedBinary;

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

/// Open native folder dialog to select a folder
pub fn open_folder_dialog(tx: Sender<AsyncMessage>) {
    std::thread::spawn(move || {
        let folder = rfd::FileDialog::new()
            .set_title("Open Folder")
            .pick_folder();

        let path = folder.map(|p| p.to_string_lossy().to_string());
        let _ = tx.send(AsyncMessage::FolderSelected(path));
    });
}

/// Load a binary file
pub fn load_binary(state: &mut AppState, tx: Sender<AsyncMessage>, path: &str) {
    let path = path.to_string();

    // Save path
    state.analysis.domain.last_binary_path = Some(path.clone());

    state.log(format!("[*] Loading {}...", path));

    std::thread::spawn(move || match LoadedBinary::from_file(&path) {
        Ok(binary) => {
            let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(std::sync::Arc::new(binary))));
        }
        Err(e) => {
            let _ = tx.send(AsyncMessage::BinaryLoaded(Err(e.to_string())));
        }
    });
}
/// Open native file dialog to save a snapshot
pub fn save_snapshot_dialog(tx: Sender<AsyncMessage>) {
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .set_title("Save Snapshot")
            .add_filter("Fission Snapshot", &["fiss"])
            .save_file();

        if let Some(path) = file {
            let _ = tx.send(AsyncMessage::SaveSnapshot(
                path.to_string_lossy().to_string(),
            ));
        }
    });
}

/// Open native file dialog to load a snapshot
pub fn load_snapshot_dialog(tx: Sender<AsyncMessage>) {
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .set_title("Load Snapshot")
            .add_filter("Fission Snapshot", &["fiss"])
            .pick_file();

        if let Some(path) = file {
            let _ = tx.send(AsyncMessage::LoadSnapshot(
                path.to_string_lossy().to_string(),
            ));
        }
    });
}

/// Load all binaries from a folder
pub fn load_folder(state: &mut AppState, tx: Sender<AsyncMessage>, folder_path: &str) {
    let folder_path = folder_path.to_string();

    state.log(format!("[*] Scanning folder: {}...", folder_path));

    std::thread::spawn(move || match scan_folder_for_binaries(&folder_path) {
        Ok(binary_paths) => {
            if binary_paths.is_empty() {
                let _ = tx.send(AsyncMessage::ProjectLoaded {
                    path: folder_path,
                    binaries: Vec::new(),
                });
                return;
            }

            let mut binaries = Vec::new();
            for path in binary_paths {
                match LoadedBinary::from_file(&path) {
                    Ok(binary) => {
                        binaries.push(Arc::new(binary));
                    }
                    Err(e) => {
                        eprintln!("[!] Failed to load {}: {}", path, e);
                    }
                }
            }

            let _ = tx.send(AsyncMessage::ProjectLoaded {
                path: folder_path,
                binaries,
            });
        }
        Err(e) => {
            eprintln!("[!] Failed to scan folder: {}", e);
            let _ = tx.send(AsyncMessage::ProjectLoaded {
                path: folder_path,
                binaries: Vec::new(),
            });
        }
    });
}

/// Scan a folder recursively for binary files
fn scan_folder_for_binaries(folder_path: &str) -> Result<Vec<String>, std::io::Error> {
    let mut binaries = Vec::new();
    let path = Path::new(folder_path);

    if !path.is_dir() {
        return Ok(binaries);
    }

    scan_dir_recursive(path, &mut binaries, 0)?;

    Ok(binaries)
}

/// Recursively scan directory for binary files (max depth: 10)
fn scan_dir_recursive(
    dir: &Path,
    binaries: &mut Vec<String>,
    depth: usize,
) -> Result<(), std::io::Error> {
    const MAX_DEPTH: usize = 10;

    if depth > MAX_DEPTH {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            scan_dir_recursive(&path, binaries, depth + 1)?;
        } else if path.is_file() {
            if is_binary_file(&path) {
                if let Some(path_str) = path.to_str() {
                    binaries.push(path_str.to_string());
                }
            }
        }
    }

    Ok(())
}

/// Check if a file is likely a binary executable
fn is_binary_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        matches!(
            ext_str.as_str(),
            "exe" | "dll" | "so" | "dylib" | "bin" | "o" | "obj" | "a" | "lib"
        )
    } else {
        // Check for executable permission on Unix-like systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(path) {
                let permissions = metadata.permissions();
                return permissions.mode() & 0o111 != 0;
            }
        }
        false
    }
}

/// Open save dialog for exporting results
pub fn export_results_dialog(_tx: Sender<AsyncMessage>) {
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .set_title("Export Results")
            .add_filter("JSON", &["json"])
            .add_filter("CSV", &["csv"])
            .save_file();

        if let Some(path) = file {
            export_results(path.to_string_lossy().to_string());
        }
    });
}

/// Export decompilation results to JSON or CSV
fn export_results(path: String) {
    // This is a placeholder - actual implementation would serialize
    // the decompile cache and project info to the selected format
    println!("[*] Exporting results to: {}", path);

    // TODO: Implement actual export logic
    // - Iterate through decompile_cache
    // - For each cached function, export address, name, and C code
    // - Include project metadata (folder, binary list)
    // - Write to JSON or CSV based on file extension
}
