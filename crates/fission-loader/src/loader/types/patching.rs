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
        let offset = usize::try_from(offset).ok()?;
        let end = offset.checked_add(new_bytes.len())?;
        let original = self.data.as_slice().get(offset..end)?.to_vec();

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
        let offset = usize::try_from(offset).ok()?;
        let end = offset.checked_add(size)?;
        Some(self.data.as_slice().get(offset..end)?.to_vec())
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

#[cfg(test)]
mod tests {
    use crate::loader::{DataBuffer, LoadedBinaryBuilder};

    /// `offset + len` near `u64::MAX` wrapped in release builds, passed the
    /// `end > len` test with `end < offset`, and panicked on the slice.
    #[test]
    fn patching_at_an_offset_whose_end_overflows_is_refused() {
        let mut binary =
            LoadedBinaryBuilder::new("patch".to_string(), DataBuffer::Heap(vec![1, 2, 3, 4]))
                .build()
                .expect("binary");
        assert_eq!(binary.patch_bytes(u64::MAX, &[9, 9]), None);
        assert_eq!(binary.patch_bytes(u64::MAX - 1, &[9, 9]), None);
        assert_eq!(binary.patch_bytes(3, &[9, 9]), None);
        assert_eq!(binary.get_bytes_at_offset(u64::MAX, 2), None);
        assert_eq!(binary.get_bytes_at_offset(3, usize::MAX), None);
        // Nothing above touched the data; an in-range patch still works.
        assert_eq!(binary.get_bytes_at_offset(0, 4), Some(vec![1, 2, 3, 4]));
        assert_eq!(binary.patch_bytes(2, &[9, 9]), Some(vec![3, 4]));
        assert_eq!(binary.get_bytes_at_offset(0, 4), Some(vec![1, 2, 9, 9]));
    }
}
