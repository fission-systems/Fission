//! Analysis Snapshot Serialization using Rkyv (Zero-Copy)

use crate::analysis::loader::LoadedBinary;
use crate::core::prelude::*;
use rkyv::Deserialize;
use std::fs;
use std::path::Path;

/// Save the loaded binary and analysis state to a snapshot file
pub fn save_snapshot(binary: &LoadedBinary, path: &Path) -> Result<()> {
    // Serialize using rkyv
    // AlignedVec is required for zero-copy deserialization
    let bytes = rkyv::to_bytes::<_, 1024>(binary)
        .map_err(|e| FissionError::other(format!("Serialization failed: {}", e)))?;

    // Write to disk
    fs::write(path, bytes).map_err(|e| FissionError::Io(e))?;

    crate::core::logging::info(&format!("Saved snapshot to {:?}", path));
    Ok(())
}

/// Load a snapshot from a file
pub fn load_snapshot(path: &Path) -> Result<LoadedBinary> {
    let data = fs::read(path).map_err(|e| FissionError::Io(e))?;

    // Validate the archive
    let archived = rkyv::check_archived_root::<LoadedBinary>(&data)
        .map_err(|e| FissionError::other(format!("Snapshot validation failed: {}", e)))?;

    // Deserialize fully into LoadedBinary (deep copy)
    // For read-only access, we could use the Archived variant directly (True Zero-Copy),
    // but LoadedBinary is used mutably throughout the app, so we deserialize it.
    let binary: LoadedBinary = archived
        .deserialize(&mut rkyv::Infallible)
        .map_err(|e| FissionError::other(format!("Deserialization failed: {}", e)))?;

    crate::core::logging::info(&format!("Loaded snapshot from {:?}", path));
    Ok(binary)
}
