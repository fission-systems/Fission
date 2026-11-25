//! JIT translation-block cache with page invalidation and hard-chain slots.
//!
//! Hard chaining (QEMU-inspired): a **global** table maps guest entry PC →
//! host function pointer (`AtomicUsize`). Any TB exit to that PC can hard-chain
//! (fallthrough **or** absolute branch/call), without a soft lookup once linked.
//!
//! The run loop calls whatever host pointer the cache holds, so publishing one
//! is `unsafe` and the tables are private. Safe code cannot make the emulator
//! jump to an arbitrary address:
//!
//! ```compile_fail,E0133
//! use fission_emulator::jit::cache::{JitBlock, JitCache};
//! let block = JitBlock {
//!     guest_pc: 0x1000,
//!     host_func_ptr: 0x10 as *const u8,
//!     block_size: 4,
//!     guest_insns: 1,
//!     next_pc: None,
//!     pages: vec![0x1000],
//!     abs_exit_targets: Vec::new(),
//! };
//! JitCache::new().insert(0x1000, std::sync::Arc::new(block));
//! ```
//!
//! ```compile_fail,E0616
//! use fission_emulator::jit::cache::JitCache;
//! let cache = JitCache::new();
//! assert_eq!(cache.blocks.read().unwrap().len(), 0);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// One compiled host translation block (may cover multiple guest instructions).
pub struct JitBlock {
    /// First guest PC covered by this block.
    pub guest_pc: u64,
    pub host_func_ptr: *const u8,
    /// Total guest bytes covered (sum of insn lengths).
    pub block_size: usize,
    /// Number of guest instructions in the TB.
    pub guest_insns: u32,
    /// Fall-through next PC if the TB ends without an absolute branch (hint).
    pub next_pc: Option<u64>,
    /// Guest pages touched by the TB (for SMC invalidation).
    pub pages: Vec<u64>,
    /// Known absolute exit targets seen while compiling (for documentation/metrics).
    pub abs_exit_targets: Vec<u64>,
}

// SAFETY: `host_func_ptr` points to finalized executable machine code owned by
// the `Emulator`'s `JITModule` for as long as the block stays in `JitCache`; the
// generated code bytes are immutable once published.
unsafe impl Send for JitBlock {}
// SAFETY: Shared references to `JitBlock` only expose immutable metadata and the
// thread-safe finalized code entry point.
unsafe impl Sync for JitBlock {}

/// Guest PC → host block map with page-level invalidation and global hard-chain table.
pub struct JitCache {
    blocks: RwLock<HashMap<u64, Arc<JitBlock>>>,
    /// Page base → list of TB entry PCs intersecting that page.
    page_to_blocks: RwLock<HashMap<u64, Vec<u64>>>,
    /// Guest entry PC → hard-chain slot (host `*const u8` as usize, 0 = unresolved).
    /// Shared by fallthrough and absolute exits targeting this PC.
    chain_table: RwLock<HashMap<u64, Arc<AtomicUsize>>>,
}

impl JitCache {
    pub fn new() -> Self {
        Self {
            blocks: RwLock::new(HashMap::new()),
            page_to_blocks: RwLock::new(HashMap::new()),
            chain_table: RwLock::new(HashMap::new()),
        }
    }

    pub fn lookup(&self, pc: u64) -> Option<Arc<JitBlock>> {
        self.blocks
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pc)
            .cloned()
    }

    /// Get or create the hard-chain slot for a guest PC.
    pub fn chain_slot(&self, pc: u64) -> Arc<AtomicUsize> {
        let mut table = self.chain_table.write().unwrap_or_else(|e| e.into_inner());
        table
            .entry(pc)
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
            .clone()
    }

    /// Load host function for hard chain, if resolved.
    pub fn hard_chain_host(&self, pc: u64) -> Option<*const u8> {
        let table = self.chain_table.read().unwrap_or_else(|e| e.into_inner());
        let slot = table.get(&pc)?;
        let host = slot.load(Ordering::Acquire);
        if host == 0 {
            None
        } else {
            Some(host as *const u8)
        }
    }

    /// Insert a TB and publish its host entry into the global chain table.
    /// Null host function pointers are rejected so they can never be called.
    ///
    /// # Safety
    ///
    /// `block.host_func_ptr` must be the entry of finalized machine code with
    /// the signature `extern "C" fn(*mut Emulator) -> u64`, honouring the
    /// [`crate::jit::callbacks`] contract, and must stay valid for as long as
    /// the block can be looked up or hard-chained to. The emulator meets this
    /// with code from its own `JITModule`, which never frees code memory while
    /// the emulator lives (so invalidating a block that is still on the host
    /// stack cannot leave a dangling return address). The run loop and the
    /// chaining callbacks call the pointer without further checks.
    pub unsafe fn insert(&self, pc: u64, block: Arc<JitBlock>) {
        if block.host_func_ptr.is_null() {
            tracing::error!("JITCache::insert: rejecting null host_func_ptr at guest PC {pc:#x}");
            return;
        }
        let pages = block.pages.clone();
        let host = block.host_func_ptr as usize;

        // Publish hard-chain target for this entry PC (absolute + fallthrough inbound).
        self.chain_slot(pc).store(host, Ordering::Release);
        tracing::debug!(
            "JIT hard-chain: publish TB@0x{:X} host={:p}",
            pc,
            block.host_func_ptr
        );

        self.blocks
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pc, block);
        let mut ptb = self
            .page_to_blocks
            .write()
            .unwrap_or_else(|e| e.into_inner());
        for page in pages {
            ptb.entry(page).or_default().push(pc);
        }
    }

    pub fn invalidate_page(&self, page_addr: u64) {
        let mut ptb = self
            .page_to_blocks
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let mut blks = self.blocks.write().unwrap_or_else(|e| e.into_inner());
        let table = self.chain_table.read().unwrap_or_else(|e| e.into_inner());

        if let Some(pcs) = ptb.remove(&page_addr) {
            let mut removed_hosts = Vec::new();
            for pc in pcs {
                if let Some(block) = blks.remove(&pc) {
                    removed_hosts.push(block.host_func_ptr as usize);
                    // Clear published chain slot for this entry.
                    if let Some(slot) = table.get(&pc) {
                        slot.store(0, Ordering::Release);
                    }
                    for p in &block.pages {
                        if *p != page_addr {
                            if let Some(list) = ptb.get_mut(p) {
                                list.retain(|x| *x != pc);
                            }
                        }
                    }
                    tracing::info!("JIT Cache: Invalidated TB at 0x{:X} due to SMC", pc);
                }
            }
            // Any other slot that still holds a removed host pointer must be cleared.
            // (Should not happen if slots are only set for their own entry PC.)
            if !removed_hosts.is_empty() {
                for (entry_pc, slot) in table.iter() {
                    let cur = slot.load(Ordering::Acquire);
                    if removed_hosts.contains(&cur) {
                        slot.store(0, Ordering::Release);
                        tracing::debug!("JIT hard-chain: cleared stale slot for 0x{:X}", entry_pc);
                    }
                }
            }
        }
    }

    /// Invalidate the blocks on every page `[addr, addr + len)` touches.
    ///
    /// The range is whatever was unmapped or re-protected, and its length is
    /// the guest's: walking it a page at a time made `munmap(p, 1 << 60)` a
    /// loop the run never came back from. So the walk is over whichever is
    /// shorter -- the pages of the range, or the pages that hold code.
    pub fn invalidate_range(&self, addr: u64, len: u64) {
        if len == 0 {
            return;
        }
        const PAGE: u64 = 0x1000;
        let first = addr & !(PAGE - 1);
        let last = addr.saturating_add(len - 1) & !(PAGE - 1);
        let span = (last - first) / PAGE + 1;
        let pages: Vec<u64> = {
            let ptb = self
                .page_to_blocks
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if span <= ptb.len() as u64 {
                (0..span)
                    .map(|i| first + i * PAGE)
                    .filter(|page| ptb.contains_key(page))
                    .collect()
            } else {
                ptb.keys()
                    .copied()
                    .filter(|page| (first..=last).contains(page))
                    .collect()
            }
        };
        for page in pages {
            self.invalidate_page(page);
        }
    }

    /// Drop every compiled block and unpublish every chain slot.
    ///
    /// QEMU's `tb_flush`, and needed for the same reason: instrumentation is
    /// decided when a block is translated, so a block compiled before an
    /// observer subscribed carries no callbacks and would keep running
    /// uninstrumented for as long as it stays cached.
    pub fn flush_all(&self) {
        let mut blks = self.blocks.write().unwrap_or_else(|e| e.into_inner());
        let mut ptb = self
            .page_to_blocks
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let table = self.chain_table.read().unwrap_or_else(|e| e.into_inner());
        for slot in table.values() {
            slot.store(0, Ordering::Release);
        }
        let dropped = blks.len();
        blks.clear();
        ptb.clear();
        if dropped > 0 {
            tracing::info!("JIT cache: flushed {dropped} blocks (instrumentation changed)");
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for JitCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_host_func_ptr_and_clears_chain_slots_on_invalidation() {
        let cache = JitCache::new();
        // SAFETY: the null pointer is rejected; nothing is ever called.
        unsafe {
            cache.insert(
                0x1000,
                Arc::new(JitBlock {
                    guest_pc: 0x1000,
                    host_func_ptr: std::ptr::null(),
                    block_size: 4,
                    guest_insns: 1,
                    next_pc: Some(0x1004),
                    pages: vec![0x1000],
                    abs_exit_targets: Vec::new(),
                }),
            );
        }
        assert!(cache.is_empty());
        assert!(cache.lookup(0x1000).is_none());
        assert!(cache.hard_chain_host(0x1000).is_none());

        let dummy_fn: usize = 0x1234_5678;
        // SAFETY: this test only inspects and invalidates the published
        // pointer; it never calls it.
        unsafe {
            cache.insert(
                0x2000,
                Arc::new(JitBlock {
                    guest_pc: 0x2000,
                    host_func_ptr: dummy_fn as *const u8,
                    block_size: 4,
                    guest_insns: 1,
                    next_pc: Some(0x2004),
                    pages: vec![0x2000],
                    abs_exit_targets: Vec::new(),
                }),
            );
        }
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.hard_chain_host(0x2000), Some(dummy_fn as *const u8));
        cache.invalidate_range(0x2000, 0x1000);
        assert!(cache.is_empty());
        assert!(cache.hard_chain_host(0x2000).is_none());
    }
}
