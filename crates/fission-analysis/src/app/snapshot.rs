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
    let mut deserializer = rkyv::Infallible;
    let inner: LoadedBinaryInner = archived
        .deserialize(&mut deserializer)
        .map_err(|e| FissionError::other(format!("Snapshot deserialize failed: {:?}", e)))?;

    // Wrap in LoadedBinary for Arc-based COW semantics
    let binary = LoadedBinary::from_inner(inner);

    crate::core::logging::info(&format!("Loaded snapshot from {:?}", path));
    Ok(binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::types::{DataBuffer, FunctionInfo, LoadedBinaryInner};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_dummy_inner() -> LoadedBinaryInner {
        LoadedBinaryInner {
            path: "test.bin".into(),
            hash: "dummy_hash".into(),
            data: Arc::new(DataBuffer::Heap(vec![0x90, 0x90, 0xC3])),
            arch_spec: "x86:LE:64:default".into(),
            entry_point: 0x1000,
            image_base: 0x0,
            functions: vec![FunctionInfo {
                name: "main".into(),
                address: 0x1000,
                size: 3,
                is_export: true,
                is_import: false,
            }],
            sections: vec![],
            is_64bit: true,
            format: "ELF".into(),
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            function_addr_index: HashMap::new(),
            function_name_index: HashMap::new(),
            functions_sorted: true,
            inferred_types: vec![],
        }
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let inner = create_dummy_inner();
        let binary = LoadedBinary::from_inner(inner.clone());

        // Use tempfile crate if available, or just a temporary path
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("fission_test_snapshot.rkyv");

        // Save
        assert!(save_snapshot(&binary, &path).is_ok());

        // Load
        let Ok(loaded) = load_snapshot(&path) else {
            panic!("Failed to load snapshot")
        };

        // Verify key fields
        assert_eq!(loaded.inner().path, inner.path);
        assert_eq!(loaded.inner().hash, inner.hash);
        assert_eq!(loaded.inner().data.as_slice(), inner.data.as_slice());
        assert_eq!(loaded.inner().functions.len(), inner.functions.len());
        assert_eq!(loaded.inner().functions[0].name, inner.functions[0].name);

        // Clean up
        let _ = std::fs::remove_file(path);
    }
}
