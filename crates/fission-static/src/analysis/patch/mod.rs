//! Binary Patching Module
//!
//! Provides functionality to modify bytes in loaded binaries and save patched versions.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::core::errors::Result;
pub use fission_core::models::QuickPatch;

/// A single patch applied to the binary
#[derive(Debug, Clone)]
pub struct Patch {
    /// File offset where the patch is applied
    pub offset: u64,
    /// Original bytes before patching
    pub original: Vec<u8>,
    /// New bytes after patching
    pub patched: Vec<u8>,
    /// Description of what this patch does
    pub description: String,
    /// Is this patch currently applied?
    pub applied: bool,
}

impl Patch {
    /// Create a new patch
    pub fn new(
        offset: u64,
        original: Vec<u8>,
        patched: Vec<u8>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            offset,
            original,
            patched,
            description: description.into(),
            applied: false,
        }
    }

    /// Size of the patch in bytes
    pub fn size(&self) -> usize {
        self.patched.len()
    }
}

/// Manager for binary patches
#[derive(Debug, Clone, Default)]
pub struct PatchManager {
    /// All patches indexed by offset
    patches: HashMap<u64, Patch>,
}

impl PatchManager {
    /// Create a new patch manager
    pub fn new() -> Self {
        Self {
            patches: HashMap::new(),
        }
    }

    /// Add a patch at the given offset
    pub fn add_patch(
        &mut self,
        offset: u64,
        original: Vec<u8>,
        patched: Vec<u8>,
        description: impl Into<String>,
    ) {
        let patch = Patch::new(offset, original, patched, description);
        self.patches.insert(offset, patch);
    }

    /// Remove a patch at the given offset
    pub fn remove_patch(&mut self, offset: u64) -> Option<Patch> {
        self.patches.remove(&offset)
    }

    /// Get a patch by offset
    pub fn get_patch(&self, offset: u64) -> Option<&Patch> {
        self.patches.get(&offset)
    }

    /// Get all patches
    pub fn all_patches(&self) -> Vec<&Patch> {
        self.patches.values().collect()
    }

    /// Get number of patches
    pub fn count(&self) -> usize {
        self.patches.len()
    }

    /// Apply all patches to a data buffer
    pub fn apply_all(&mut self, data: &mut [u8]) {
        for patch in self.patches.values_mut() {
            let Ok(offset) = usize::try_from(patch.offset) else {
                continue;
            };
            let Some(end) = offset.checked_add(patch.patched.len()) else {
                continue;
            };

            if end <= data.len() {
                data[offset..end].copy_from_slice(&patch.patched);
                patch.applied = true;
            }
        }
    }

    /// Revert all patches in a data buffer
    pub fn revert_all(&mut self, data: &mut [u8]) {
        for patch in self.patches.values_mut() {
            if !patch.applied {
                continue;
            }
            let Ok(offset) = usize::try_from(patch.offset) else {
                continue;
            };
            let Some(end) = offset.checked_add(patch.original.len()) else {
                continue;
            };

            if end <= data.len() {
                data[offset..end].copy_from_slice(&patch.original);
                patch.applied = false;
            }
        }
    }

    /// Clear all patches
    pub fn clear(&mut self) {
        self.patches.clear();
    }
}

/// Save patched data to a file
pub fn save_patched_file(data: &[u8], path: &Path) -> Result<()> {
    fs::write(path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_manager() {
        let mut data = vec![0x74, 0x10, 0x90, 0x90]; // JE +16, NOP, NOP
        let mut pm = PatchManager::new();

        // Patch JE to JNE
        pm.add_patch(0, vec![0x74], vec![0x75], "Invert jump");

        assert_eq!(pm.count(), 1);

        // Apply patches
        pm.apply_all(&mut data);
        assert_eq!(data[0], 0x75); // Now JNE

        // Revert
        pm.revert_all(&mut data);
        assert_eq!(data[0], 0x74); // Back to JE
    }

    #[test]
    fn overflow_offset_and_unapplied_revert_are_safe() {
        let mut data = vec![0x90, 0x91, 0x92, 0x93];
        let mut pm = PatchManager::new();
        pm.add_patch(
            u64::MAX - 1,
            vec![0xaa, 0xbb, 0xcc],
            vec![0x11, 0x22, 0x33],
            "oob",
        );
        pm.add_patch(0, vec![0xff, 0xfe], vec![0x01, 0x02], "unapplied");

        // Unapplied patch must not clobber buffer on revert.
        pm.revert_all(&mut data);
        assert_eq!(data, vec![0x90, 0x91, 0x92, 0x93]);

        // Overflowing u64::MAX - 1 + 3 must not wrap into range or panic.
        pm.apply_all(&mut data);
        assert!(!pm.get_patch(u64::MAX - 1).unwrap().applied);
        assert_eq!(data[0..2], [0x01, 0x02]);
    }
}
