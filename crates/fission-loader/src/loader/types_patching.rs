use super::LoadedBinary;
use crate::prelude::*;
use std::path::Path;
use std::sync::Arc;

impl LoadedBinary {
    /// Patch bytes at a file offset
    /// Returns the original bytes that were replaced
    ///
    /// Uses Copy-on-Write semantics at the LoadedBinary level:
    /// If the LoadedBinary is cloned (via Arc), this modification
    /// will trigger a clone of the entire inner structure.
    pub fn patch_bytes(&mut self, offset: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + new_bytes.len();

        if end > self.data.as_slice().len() {
            return None;
        }

        let original = self.data.as_slice()[offset..end].to_vec();

        let data_mut = Arc::make_mut(&mut self.data);
        let vec = data_mut.to_mut_vec();
        vec[offset..end].copy_from_slice(new_bytes);

        Some(original)
    }

    /// Patch bytes at a virtual address
    /// Converts VA to file offset and applies the patch
    pub fn patch_bytes_va(&mut self, va: u64, new_bytes: &[u8]) -> Option<Vec<u8>> {
        let offset = self.va_to_file_offset(va)?;
        self.patch_bytes(offset as u64, new_bytes)
    }

    /// Get bytes at a file offset (for displaying original)
    pub fn get_bytes_at_offset(&self, offset: u64, size: usize) -> Option<Vec<u8>> {
        let offset = offset as usize;
        let end = offset + size;

        if end > self.data.as_slice().len() {
            return None;
        }

        Some(self.data.as_slice()[offset..end].to_vec())
    }

    /// Save the (potentially patched) binary to a file
    pub fn save_as<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        std::fs::write(path, self.data.as_slice())?;
        Ok(())
    }

    /// Apply a quick patch at a file offset
    pub fn apply_quick_patch(&mut self, offset: u64, patch_type: QuickPatch) -> Option<Vec<u8>> {
        let bytes = patch_type.bytes();
        self.patch_bytes(offset, &bytes)
    }

    /// Apply a quick patch at a virtual address
    pub fn apply_quick_patch_va(&mut self, va: u64, patch_type: QuickPatch) -> Option<Vec<u8>> {
        let bytes = patch_type.bytes();
        self.patch_bytes_va(va, &bytes)
    }
}
