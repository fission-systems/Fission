//! JIT translation-block cache with page invalidation and hard-chain slots.
//!
//! Hard chaining (QEMU-inspired): a **global** table maps guest entry PC →
//! host function pointer (`AtomicUsize`). Any TB exit to that PC can hard-chain
//! (fallthrough **or** absolute branch/call), without a soft lookup once linked.

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

unsafe impl Send for JitBlock {}
unsafe impl Sync for JitBlock {}

/// Guest PC → host block map with page-level invalidation and global hard-chain table.
pub struct JitCache {
    pub blocks: RwLock<HashMap<u64, Arc<JitBlock>>>,
    /// Page base → list of TB entry PCs intersecting that page.
    pub page_to_blocks: RwLock<HashMap<u64, Vec<u64>>>,
    /// Guest entry PC → hard-chain slot (host `*const u8` as usize, 0 = unresolved).
    /// Shared by fallthrough and absolute exits targeting this PC.
    pub chain_table: RwLock<HashMap<u64, Arc<AtomicUsize>>>,
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
        self.blocks.read().unwrap().get(&pc).cloned()
    }

    /// Get or create the hard-chain slot for a guest PC.
    pub fn chain_slot(&self, pc: u64) -> Arc<AtomicUsize> {
        let mut table = self.chain_table.write().unwrap();
        table
            .entry(pc)
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
            .clone()
    }

    /// Load host function for hard chain, if resolved.
    pub fn hard_chain_host(&self, pc: u64) -> Option<*const u8> {
        let table = self.chain_table.read().unwrap();
        let slot = table.get(&pc)?;
        let host = slot.load(Ordering::Acquire);
        if host == 0 {
            None
        } else {
            Some(host as *const u8)
        }
    }

    /// Insert a TB and publish its host entry into the global chain table.
    pub fn insert(&self, pc: u64, block: Arc<JitBlock>) {
        let pages = block.pages.clone();
        let host = block.host_func_ptr as usize;

        // Publish hard-chain target for this entry PC (absolute + fallthrough inbound).
        self.chain_slot(pc).store(host, Ordering::Release);
        tracing::debug!("JIT hard-chain: publish TB@0x{:X} host={:p}", pc, block.host_func_ptr);

        self.blocks.write().unwrap().insert(pc, block);
        let mut ptb = self.page_to_blocks.write().unwrap();
        for page in pages {
            ptb.entry(page).or_default().push(pc);
        }
    }

    pub fn invalidate_page(&self, page_addr: u64) {
        let mut ptb = self.page_to_blocks.write().unwrap();
        let mut blks = self.blocks.write().unwrap();
        let table = self.chain_table.write().unwrap();

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
                        tracing::debug!(
                            "JIT hard-chain: cleared stale slot for 0x{:X}",
                            entry_pc
                        );
                    }
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.read().unwrap().len()
    }
}

impl Default for JitCache {
    fn default() -> Self {
        Self::new()
    }
}
