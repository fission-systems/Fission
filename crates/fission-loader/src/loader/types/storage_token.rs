//! Weak identity for the loader's copy-on-write byte/metadata storage.

use super::{LoadedBinary, LoadedBinaryInner};
use std::sync::{Arc, Weak};

/// Identity of one COW inner allocation, without retaining its image bytes.
///
/// This is neither a content hash nor a program ID. Mutating through
/// `inner_mut`, `DerefMut` or the patch APIs detaches the token, including when
/// the binary's content hash stays unchanged. Companion debug fields stored
/// outside the inner allocation are not covered by this identity.
#[derive(Clone)]
pub struct LoadedBinaryStorageToken {
    inner: Weak<LoadedBinaryInner>,
}

impl LoadedBinaryStorageToken {
    pub fn matches(&self, binary: &LoadedBinary) -> bool {
        // The weak reference keeps the allocation header from being reused,
        // even after its data has been dropped. No dangling pointer is read.
        std::ptr::eq(self.inner.as_ptr(), Arc::as_ptr(&binary.inner))
    }
}

impl LoadedBinary {
    pub fn storage_token(&self) -> LoadedBinaryStorageToken {
        LoadedBinaryStorageToken {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::loader::{DataBuffer, LoadedBinaryBuilder};

    fn binary() -> super::LoadedBinary {
        LoadedBinaryBuilder::new("storage.bin".into(), DataBuffer::Heap(vec![1; 32]))
            .build()
            .expect("storage fixture")
    }

    #[test]
    fn storage_token_matches_clones_until_cow_metadata_mutation() {
        let original = binary();
        let token = original.storage_token();
        let mut changed = original.clone();
        assert!(token.matches(&original));
        assert!(token.matches(&changed));
        changed.inner_mut().image_base = 0x1000;
        assert!(token.matches(&original));
        assert!(!token.matches(&changed));
        assert!(changed.storage_token().matches(&changed));
    }

    #[test]
    fn storage_token_detaches_on_patch_even_without_another_strong_owner() {
        let mut binary = binary();
        let token = binary.storage_token();
        let hash = binary.hash.clone();
        assert_eq!(binary.patch_bytes(0, &[2, 3]), Some(vec![1, 1]));
        assert_eq!(binary.hash, hash);
        assert!(!token.matches(&binary));
        assert!(binary.storage_token().matches(&binary));
    }

    #[test]
    fn storage_token_does_not_retain_the_image() {
        let binary = binary();
        let token = binary.storage_token();
        assert_eq!(token.inner.strong_count(), 1);
        drop(binary);
        assert_eq!(token.inner.strong_count(), 0);
        assert!(token.inner.upgrade().is_none());
    }
}
