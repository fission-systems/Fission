pub use fission_ui::engine::*;

use std::fs;
use std::path::PathBuf;

/// Desktop only function to load binary from path
pub fn load_binary_blocking(path: &PathBuf) -> Result<LoadResult, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    load_binary_from_bytes_blocking(data, &name)
}
