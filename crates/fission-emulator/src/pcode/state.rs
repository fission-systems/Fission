use crate::pcode::page_map::PageMap;
use crate::pcode::spaces::SpaceLayout;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
pub enum MemoryPage {
    /// Pure concrete page (e.g. .text or un-tainted RAM). Length is always page_size.
    Concrete(Arc<Vec<u8>>),
    /// Page containing symbolic values at concrete offsets.
    /// The `shadow` vector stores the SymNodeId for each tainted byte.
    Symbolic {
        concrete: Arc<Vec<u8>>,
        shadow: Arc<Vec<Option<u32>>>,
    },
    /// A full fallback to SMT Array theory when a symbolic pointer is written.
    /// `array_id` is the SymNodeId of the current ArrayStore AST node.
    ArrayTheory { array_id: u32 },
}

impl MemoryPage {
    pub fn new_concrete(page_size: usize) -> Self {
        Self::Concrete(Arc::new(vec![0; page_size]))
    }

    pub fn make_symbolic(&mut self) {
        if let Self::Concrete(data) = self {
            let len = data.len();
            *self = Self::Symbolic {
                concrete: data.clone(), // COW
                shadow: Arc::new(vec![None; len]),
            };
        }
    }
}

/// Represents a single address space in the emulated machine (e.g. ram, register, unique).
#[derive(Clone, Serialize, Deserialize)]
pub struct AddressSpace {
    pub name: String,
    // Hybrid Page-based memory allocation (4KB pages)
    pub pages: im::HashMap<u64, MemoryPage>,
    pub page_size: u64,
    /// Root symbolic array representing this address space in SMT Array Theory
    pub theory_array_id: Option<u32>,
}

impl AddressSpace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pages: im::HashMap::new(),
            page_size: 0x1000,
            theory_array_id: None,
        }
    }

    fn get_page_mut(&mut self, addr: u64) -> &mut MemoryPage {
        let page_addr = addr & !(self.page_size - 1);
        let ps = self.page_size as usize;
        // One lookup, not two. `pages` is a persistent HAMT keyed with SipHash
        // -- chosen so a snapshot is a cheap clone -- so `contains_key` then
        // `get_mut` hashed the same address twice on every access.
        self.pages
            .entry(page_addr)
            .or_insert_with(|| MemoryPage::new_concrete(ps))
    }

    fn get_page(&self, addr: u64) -> Option<&MemoryPage> {
        let page_addr = addr & !(self.page_size - 1);
        self.pages.get(&page_addr)
    }

    /// Read into a caller's buffer.
    ///
    /// `read` allocates a `Vec` for its result, and the JIT's memory callback
    /// wants eight bytes at most -- so the allocator was showing up as the
    /// single largest cost in a loop that reads one register slot. This is the
    /// primitive; `read` is a thin wrapper that allocates once at the boundary.
    pub fn read_into(&self, addr: u64, buf: &mut [u8]) {
        buf.fill(0);
        let ps = self.page_size as usize;
        let mut done = 0usize;
        while done < buf.len() {
            let current = addr.wrapping_add(done as u64);
            let offset = (current & (self.page_size - 1)) as usize;
            let take = (buf.len() - done).min(ps - offset);
            match self.get_page(current) {
                Some(MemoryPage::Concrete(data)) => {
                    buf[done..done + take].copy_from_slice(&data[offset..offset + take]);
                }
                Some(MemoryPage::Symbolic { concrete, .. }) => {
                    buf[done..done + take].copy_from_slice(&concrete[offset..offset + take]);
                }
                Some(MemoryPage::ArrayTheory { .. }) | None => {}
            }
            done += take;
        }
    }

    /// Read `size` bytes, a page span at a time.
    ///
    /// Not byte at a time: each byte used to cost its own page lookup, and a
    /// lookup is a SipHash into a persistent HAMT, so an eight-byte guest load
    /// hashed the same page address eight times. Accesses that stay inside one
    /// page -- nearly all of them -- now hash once.
    pub fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>> {
        let mut result = vec![0u8; size];
        self.read_into(addr, &mut result);
        Ok(result)
    }

    /// Write `data`, a page span at a time.
    ///
    /// Same reason as [`AddressSpace::read`], plus one more: `Arc::make_mut`
    /// is an atomic refcount check, and the byte loop ran it once per byte of
    /// every store.
    pub fn write(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        let ps = self.page_size as usize;
        let page_mask = self.page_size - 1;
        let mut done = 0usize;
        while done < data.len() {
            let current = addr.wrapping_add(done as u64);
            let offset = (current & page_mask) as usize;
            let take = (data.len() - done).min(ps - offset);
            let chunk = &data[done..done + take];
            match self.get_page_mut(current) {
                MemoryPage::Concrete(page_data) => {
                    Arc::make_mut(page_data)[offset..offset + take].copy_from_slice(chunk);
                }
                MemoryPage::Symbolic { concrete, shadow } => {
                    Arc::make_mut(concrete)[offset..offset + take].copy_from_slice(chunk);
                    // Concrete bytes replace whatever the shadow said about
                    // them, byte for byte.
                    Arc::make_mut(shadow)[offset..offset + take].fill(None);
                }
                // A concrete write to an array-theory page is not modelled;
                // it was ignored before this and still is.
                MemoryPage::ArrayTheory { .. } => {}
            }
            done += take;
        }
        Ok(())
    }

    pub fn get_shadow(&self, addr: u64) -> Option<u32> {
        let current_addr = addr;
        let offset = (current_addr & (self.page_size - 1)) as usize;
        match self.get_page(current_addr) {
            Some(MemoryPage::Symbolic { shadow, .. }) => shadow[offset],
            _ => None,
        }
    }

    pub fn set_shadow(&mut self, addr: u64, node: u32) -> Option<u32> {
        let current_addr = addr;
        let offset = (current_addr & (self.page_size - 1)) as usize;
        let page = self.get_page_mut(current_addr);

        // Ensure the page is symbolic
        page.make_symbolic();

        if let MemoryPage::Symbolic { shadow, .. } = page {
            let shadow_mut = Arc::make_mut(shadow);
            let old = shadow_mut[offset];
            shadow_mut[offset] = Some(node);
            old
        } else {
            None
        }
    }

    pub fn clear_shadow(&mut self, addr: u64) -> Option<u32> {
        let current_addr = addr;
        let offset = (current_addr & (self.page_size - 1)) as usize;
        let page_addr = addr & !(self.page_size - 1);
        if let Some(MemoryPage::Symbolic { shadow, .. }) = self.pages.get_mut(&page_addr) {
            let shadow_mut = Arc::make_mut(shadow);
            let old = shadow_mut[offset];
            shadow_mut[offset] = None;
            old
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AccessKind {
    Read,
    Write,
    Execute,
}

#[derive(Clone)]
pub struct MemoryAccess {
    pub kind: AccessKind,
    pub space_id: u64,
    pub addr: u64,
    pub size: usize,
    /// For small writes or reads, we can optionally provide the value.
    pub data: Option<Vec<u8>>,
}

pub type MemoryAccessHook = std::sync::Arc<dyn Fn(&MemoryAccess) + Send + Sync>;

/// Holds the complete state of the emulated machine.
#[derive(Clone, Serialize, Deserialize)]
pub struct MachineState {
    pub spaces: im::HashMap<u64, AddressSpace>,

    /// Guest virtual memory map + protections (user-mode).
    /// Cleanroom design inspired by QEMU linux-user page flags; no vendor dependency.
    pub page_map: PageMap,

    /// SLA-native address space indices (ram / register / unique / …).
    pub spaces_layout: SpaceLayout,

    /// When true, RAM accesses must hit a mapped page with matching R/W prot.
    /// Unmapped or wrong-prot accesses return [`crate::pcode::page_map::PageFault`].
    pub enforce_page_faults: bool,

    #[serde(skip)]
    pub hooks: Vec<MemoryAccessHook>,

    #[serde(skip)]
    pub tracing_memory: bool,
    #[serde(skip)]
    pub trace_mem_reads: Vec<(u64, Vec<u8>)>,
    #[serde(skip)]
    pub trace_mem_writes: Vec<(u64, Vec<u8>, Vec<u8>)>, // (addr, old_bytes, new_bytes)

    /// Shadow register mapping: (register_offset) -> SymNodeId.
    /// We can treat register space as just another address space, but usually
    /// registers are accessed by name/offset, so a separate map or just using shadow_memory with space_id=2 works.
    /// Let's use shadow_memory with space_id=2 for registers, so we don't need a separate field!

    #[serde(skip)]
    pub trace_shadow_writes: Vec<(u64, u64, Option<u32>, Option<u32>)>, // (space_id, address, old_node, new_node)

    /// Vestigial: the register slot cache is gone (`host_reg_file` is the
    /// register file, so a cache in front of it cost a SipHash to save an
    /// eight-byte copy). Kept at zero so the metrics shape does not change.
    #[serde(skip)]
    pub reg_cache_hits: u64,
    #[serde(skip)]
    pub reg_cache_misses: u64,

    /// Contiguous host-side register file for zero-callout JIT loads/stores.
    /// Mirrors the low `HOST_REG_FILE_SIZE` bytes of register space.
    #[serde(skip)]
    pub host_reg_file: Box<[u8]>,
}

/// Bytes of register space mirrored for zero-callout JIT access.
pub const HOST_REG_FILE_SIZE: usize = 0x2000;

impl fission_solver::solver::MemoryOracle for MachineState {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8> {
        self.read_space_readonly(space_id, addr, 1)
            .ok()
            .map(|v| v[0])
    }
}

impl MachineState {
    pub fn new() -> Self {
        Self::with_layout(SpaceLayout::fallback())
    }

    pub fn with_layout(layout: SpaceLayout) -> Self {
        let mut spaces = im::HashMap::new();
        spaces.insert(layout.unique, AddressSpace::new("unique"));
        spaces.insert(layout.register, AddressSpace::new("register"));
        spaces.insert(layout.ram, AddressSpace::new("ram"));
        // Also materialize any other named spaces from the SLA table.
        for (name, &idx) in &layout.by_name {
            if !spaces.contains_key(&idx) {
                spaces.insert(idx, AddressSpace::new(name.clone()));
            }
        }
        Self {
            spaces,
            page_map: PageMap::new(),
            spaces_layout: layout,
            enforce_page_faults: false,
            hooks: Vec::new(),
            tracing_memory: false,
            trace_mem_reads: Vec::new(),
            trace_mem_writes: Vec::new(),
            trace_shadow_writes: Vec::new(),
            reg_cache_hits: 0,
            reg_cache_misses: 0,
            host_reg_file: vec![0u8; HOST_REG_FILE_SIZE].into_boxed_slice(),
        }
    }

    /// Clear the host register file (TTD restore / snapshot).
    pub fn invalidate_reg_cache(&mut self) {
        self.host_reg_file.fill(0);
    }

    /// Stable host pointer for JIT register-file loads (valid for emulator lifetime).
    #[inline]
    pub fn host_reg_file_ptr(&mut self) -> *mut u8 {
        self.host_reg_file.as_mut_ptr()
    }

    #[inline]
    pub fn host_reg_in_range(&self, offset: u64, size: usize) -> bool {
        let end = offset as usize + size;
        end <= HOST_REG_FILE_SIZE && offset as usize + size >= size
    }

    /// Enable PageFault checks on the RAM space (user-mode).
    pub fn with_page_faults(mut self, enabled: bool) -> Self {
        self.enforce_page_faults = enabled;
        self
    }

    #[inline]
    pub fn ram_space(&self) -> u64 {
        self.spaces_layout.ram
    }

    #[inline]
    pub fn register_space(&self) -> u64 {
        self.spaces_layout.register
    }

    #[inline]
    pub fn unique_space(&self) -> u64 {
        self.spaces_layout.unique
    }

    pub fn get_theory_array_id(&self, space_id: u64) -> Option<u32> {
        self.spaces.get(&space_id).and_then(|s| s.theory_array_id)
    }

    pub fn set_theory_array_id(&mut self, space_id: u64, id: u32) {
        if !self.spaces.contains_key(&space_id) {
            self.spaces
                .insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        space.theory_array_id = Some(id);
    }

    /// Read into a caller's buffer, without allocating.
    ///
    /// The JIT's memory callback wants at most eight bytes and used to get a
    /// freshly allocated `Vec` for every one, which put `malloc`/`free` at the
    /// top of the profile in a loop that reads a single register slot.
    ///
    /// The register slot cache went at the same time. `host_reg_file` *is*
    /// register space -- a flat array -- so the cache was paying a SipHash to
    /// avoid an eight-byte copy, and every block exit then paid another
    /// callback to invalidate it.
    pub fn read_into(&mut self, space_id: u64, addr: u64, buf: &mut [u8]) -> Result<()> {
        if space_id == 0 {
            bail!("Attempted to read from const space via memory read");
        }
        let size = buf.len();
        // Hot path: register space is the host register file, directly.
        if space_id == self.spaces_layout.register && self.host_reg_in_range(addr, size) {
            let start = addr as usize;
            buf.copy_from_slice(&self.host_reg_file[start..start + size]);
            return Ok(());
        }
        if self.enforce_page_faults && space_id == self.spaces_layout.ram {
            use crate::pcode::page_map::AccessKind;
            self.page_map
                .check_range(addr, size, AccessKind::Read)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        // One lookup, not `contains_key` then `get_mut`: `spaces` is a HAMT
        // too, so the pair hashed the space id twice on every access.
        let space = self
            .spaces
            .entry(space_id)
            .or_insert_with(|| AddressSpace::new(format!("space_{space_id}")));
        space.read_into(addr, buf);

        if self.tracing_memory && space_id == self.spaces_layout.ram {
            self.trace_mem_reads.push((addr, buf.to_vec()));
        }
        Ok(())
    }

    pub fn read_space(&mut self, space_id: u64, addr: u64, size: usize) -> Result<Vec<u8>> {
        let mut out = vec![0u8; size];
        self.read_into(space_id, addr, &mut out)?;
        Ok(out)
    }

    pub fn read_space_readonly(&self, space_id: u64, addr: u64, size: usize) -> Result<Vec<u8>> {
        if space_id == 0 {
            bail!("Attempted to read from const space via memory read");
        }
        if let Some(space) = self.spaces.get(&space_id) {
            space.read(addr, size)
        } else {
            Ok(vec![0; size])
        }
    }

    pub fn write_space(&mut self, space_id: u64, addr: u64, data: &[u8]) -> Result<()> {
        if space_id == 0 {
            bail!("Attempted to write to const space");
        }

        if self.enforce_page_faults && space_id == self.spaces_layout.ram {
            use crate::pcode::page_map::AccessKind;
            self.page_map
                .check_range(addr, data.len(), AccessKind::Write)
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        if self.tracing_memory && space_id == self.spaces_layout.ram {
            // Read the old value before overwriting so TTD can reconstruct undo deltas.
            let old = if let Some(space) = self.spaces.get(&space_id) {
                space
                    .read(addr, data.len())
                    .unwrap_or_else(|_| vec![0; data.len()])
            } else {
                vec![0; data.len()]
            };
            self.trace_mem_writes.push((addr, old, data.to_vec()));
        }

        if !self.spaces.contains_key(&space_id) {
            self.spaces
                .insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let is_reg = space_id == self.spaces_layout.register;
        let host_ok = is_reg && self.host_reg_in_range(addr, data.len());
        {
            let space = self.spaces.get_mut(&space_id).unwrap();
            space.write(addr, data)?;

            // When writing concrete bytes, clear their shadow memory taint.
            for i in 0..data.len() {
                let curr_addr = addr + i as u64;
                let old_node = space.clear_shadow(curr_addr);
                if self.tracing_memory && old_node.is_some() {
                    self.trace_shadow_writes
                        .push((space_id, curr_addr, old_node, None));
                }
            }
        }

        // The host register file *is* register space, so keeping it current is
        // the whole of the bookkeeping -- there is no cache in front of it to
        // invalidate any more.
        if is_reg && host_ok {
            let start = addr as usize;
            self.host_reg_file[start..start + data.len()].copy_from_slice(data);
        }

        Ok(())
    }

    pub fn set_shadow_memory(&mut self, space_id: u64, addr: u64, node: u32) {
        if !self.spaces.contains_key(&space_id) {
            self.spaces
                .insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        let old_node = space.set_shadow(addr, node);
        if self.tracing_memory {
            self.trace_shadow_writes
                .push((space_id, addr, old_node, Some(node)));
        }
    }

    pub fn get_shadow_memory(&self, space_id: u64, addr: u64) -> Option<u32> {
        self.spaces.get(&space_id).and_then(|s| s.get_shadow(addr))
    }

    pub fn clear_shadow_memory(&mut self, space_id: u64, addr: u64) {
        if let Some(space) = self.spaces.get_mut(&space_id) {
            let old_node = space.clear_shadow(addr);
            if self.tracing_memory && old_node.is_some() {
                self.trace_shadow_writes
                    .push((space_id, addr, old_node, None));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_memory_model() {
        let mut state = MachineState::new();
        let ram = state.ram_space();
        // Read unitialized (concrete 0)
        let data = state.read_space(ram, 0x1000, 4).unwrap();
        assert_eq!(data, vec![0, 0, 0, 0]);

        // Write concrete
        state
            .write_space(ram, 0x1000, &[0xDE, 0xAD, 0xBE, 0xEF])
            .unwrap();
        let data = state.read_space(ram, 0x1000, 4).unwrap();
        assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        // Shadow memory starts empty
        assert_eq!(state.get_shadow_memory(ram, 0x1000), None);

        // Set shadow memory on first two bytes
        state.set_shadow_memory(ram, 0x1000, 42);
        state.set_shadow_memory(ram, 0x1001, 43);

        assert_eq!(state.get_shadow_memory(ram, 0x1000), Some(42));
        assert_eq!(state.get_shadow_memory(ram, 0x1001), Some(43));
        assert_eq!(state.get_shadow_memory(ram, 0x1002), None);

        // Read concrete after shadow is set
        let data = state.read_space(ram, 0x1000, 4).unwrap();
        assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        // Write concrete to partially clear shadow memory
        state.write_space(ram, 0x1001, &[0xCC, 0xDD]).unwrap();
        assert_eq!(state.get_shadow_memory(ram, 0x1000), Some(42)); // Unaffected
        assert_eq!(state.get_shadow_memory(ram, 0x1001), None); // Cleared
        assert_eq!(state.get_shadow_memory(ram, 0x1002), None); // Cleared

        let data = state.read_space(ram, 0x1000, 4).unwrap();
        assert_eq!(data, vec![0xDE, 0xCC, 0xDD, 0xEF]);
    }
}
