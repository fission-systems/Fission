use fission_core::prelude::*;
use fission_core::{APP_DIR_NAME, DECOMP_CACHE_DIR_NAME, DEFAULT_L1_CACHE_SIZE};
use lru::LruCache;
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tracing::{debug, info};

/// Decompiler Cache - Manages In-memory (L1) and Disk (L2) caching
pub struct DecompilerCache {
    /// L1 Cache: In-memory LRU
    l1: LruCache<u64, String>,
    /// L2 Cache: Disk-based storage path
    l2_root: PathBuf,
    /// Binary identifier (hash)
    binary_hash: String,
}

impl DecompilerCache {
    /// Create a new decompiler cache for a specific binary
    pub fn new(binary_hash: &str, l1_size: usize) -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| FissionError::other("Could not determine cache directory"))?
            .join(APP_DIR_NAME)
            .join(DECOMP_CACHE_DIR_NAME)
            .join(binary_hash);

        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).map_err(|e| {
                FissionError::other(format!("Failed to create cache directory: {}", e))
            })?;
        }

        info!("[*] Initialized decompiler cache at {:?}", cache_dir);

        let default_l1 = match NonZeroUsize::new(DEFAULT_L1_CACHE_SIZE) {
            Some(v) => v,
            None => NonZeroUsize::MIN,
        };
        let l1_cap = NonZeroUsize::new(l1_size).unwrap_or(default_l1);

        Ok(Self {
            l1: LruCache::new(l1_cap),
            l2_root: cache_dir,
            binary_hash: binary_hash.to_string(),
        })
    }

    /// Try to get decompiled code from cache
    pub fn get(&mut self, address: u64) -> Option<String> {
        // 1. Check L1 (Memory)
        if let Some(code) = self.l1.get(&address) {
            debug!("L1 cache hit for 0x{:x}", address);
            return Some(code.clone());
        }

        // 2. Check L2 (Disk)
        let file_path = self.get_file_path(address);
        if file_path.exists() {
            if let Ok(code) = fs::read_to_string(&file_path) {
                debug!("L2 cache hit for 0x{:x}", address);
                // Backfill L1
                self.l1.put(address, code.clone());
                return Some(code);
            }
        }

        None
    }

    /// Store decompiled code in cache
    pub fn put(&mut self, address: u64, code: String) {
        // 1. Update L1
        self.l1.put(address, code.clone());

        // 2. Update L2 (Disk)
        let file_path = self.get_file_path(address);
        if let Err(e) = fs::write(&file_path, code) {
            tracing::error!("Failed to write decompiler cache to {:?}: {}", file_path, e);
        } else {
            debug!("Stored decompiler result for 0x{:x} to L2", address);
        }
    }

    /// Clear all cache for this binary
    pub fn clear(&mut self) {
        self.l1.clear();
        if let Err(e) = fs::remove_dir_all(&self.l2_root) {
            tracing::warn!("Failed to clear L2 cache at {:?}: {}", self.l2_root, e);
        }
        // Re-create directory
        let _ = fs::create_dir_all(&self.l2_root);
    }

    /// Get path to cached file for a given address
    fn get_file_path(&self, address: u64) -> PathBuf {
        self.l2_root.join(format!("0x{:x}.c", address))
    }

    /// Get the binary hash associated with this cache
    pub fn binary_hash(&self) -> &str {
        &self.binary_hash
    }
}
