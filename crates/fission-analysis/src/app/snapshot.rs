//! Analysis Snapshot Serialization using Rkyv (Zero-Copy)

use crate::prelude::*;
use fission_loader::loader::{LoadedBinary, LoadedBinaryInner};
use rkyv::Deserialize;
use std::fs;
use std::path::Path;

/// Save the loaded binary and analysis state to a snapshot file
pub fn save_snapshot(binary: &LoadedBinary, path: &Path) -> Result<()> {
    // Serialize the inner data using rkyv
    // AlignedVec is required for zero-copy deserialization
    let bytes = rkyv::to_bytes::<_, 1024>(binary.inner())
        .map_err(|e| FissionError::other(format!("Serialization failed: {}", e)))?;

    // Write to disk
    fs::write(path, bytes).map_err(|e| FissionError::Io(e))?;

    crate::core::logging::info(&format!("Saved snapshot to {:?}", path));
    Ok(())
}

/// Load a snapshot from a file
pub fn load_snapshot(path: &Path) -> Result<LoadedBinary> {
    let data = fs::read(path).map_err(|e| FissionError::Io(e))?;

    // Validate the archive (now against LoadedBinaryInner)
    let archived = rkyv::check_archived_root::<LoadedBinaryInner>(&data)
        .map_err(|e| FissionError::other(format!("Snapshot validation failed: {}", e)))?;

    // Deserialize fully into LoadedBinaryInner (deep copy)
    let inner: LoadedBinaryInner = archived
        .deserialize(&mut rkyv::Infallible)
        .map_err(|e| FissionError::other(format!("Deserialization failed: {}", e)))?;

    // Wrap in LoadedBinary for Arc-based COW semantics
    let binary = LoadedBinary::from_inner(inner);

    crate::core::logging::info(&format!("Loaded snapshot from {:?}", path));
    Ok(binary)
}
