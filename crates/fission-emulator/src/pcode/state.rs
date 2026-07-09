use anyhow::{Result, bail};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use crate::pcode::page_map::PageMap;
use crate::pcode::spaces::SpaceLayout;

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
    ArrayTheory {
        array_id: u32,
    },
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
        if !self.pages.contains_key(&page_addr) {
            self.pages.insert(page_addr, MemoryPage::new_concrete(ps));
        }
        self.pages.get_mut(&page_addr).unwrap()
    }

    fn get_page(&self, addr: u64) -> Option<&MemoryPage> {
        let page_addr = addr & !(self.page_size - 1);
        self.pages.get(&page_addr)
    }

    pub fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(size);
        for i in 0..size as u64 {
            let current_addr = addr + i;
            let offset = (current_addr & (self.page_size - 1)) as usize;
            let byte = match self.get_page(current_addr) {
                Some(MemoryPage::Concrete(data)) => data[offset],
                Some(MemoryPage::Symbolic { concrete, .. }) => concrete[offset],
                Some(MemoryPage::ArrayTheory { .. }) => 0, // Fallback for pure concrete read
                None => 0, // Uninitialized memory reads as 0
            };
            result.push(byte);
        }
        Ok(result)
    }

    pub fn write(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        for (i, &byte) in data.iter().enumerate() {
            let current_addr = addr + i as u64;
            let offset = (current_addr & (self.page_size - 1)) as usize;
            let page = self.get_page_mut(current_addr);
            match page {
                MemoryPage::Concrete(page_data) => {
                    Arc::make_mut(page_data)[offset] = byte;
                }
                MemoryPage::Symbolic { concrete, shadow } => {
                    Arc::make_mut(concrete)[offset] = byte;
                    Arc::make_mut(shadow)[offset] = None;
                }
                MemoryPage::ArrayTheory { .. } => {
                    // For now, if we do a concrete write to an ArrayTheory page, we might just ignore the array part
                    // or convert it back. We will handle ArrayTheory separately later.
                }
            }
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
    pub trace_mem_writes: Vec<(u64, Vec<u8>, Vec<u8>)>,  // (addr, old_bytes, new_bytes)

    /// Shadow register mapping: (register_offset) -> SymNodeId.
    /// We can treat register space as just another address space, but usually
    /// registers are accessed by name/offset, so a separate map or just using shadow_memory with space_id=2 works.
    /// Let's use shadow_memory with space_id=2 for registers, so we don't need a separate field!

    #[serde(skip)]
    pub trace_shadow_writes: Vec<(u64, u64, Option<u32>, Option<u32>)>, // (space_id, address, old_node, new_node)

    /// Persistent register-space cache (offset → u64) across TBs.
    /// Reduces page-map walk cost for hot GPRs; invalidated on bulk restore.
    #[serde(skip)]
    pub reg_cache: std::collections::HashMap<u64, u64>,
    /// Hit/miss counters for telemetry.
    #[serde(skip)]
    pub reg_cache_hits: u64,
    #[serde(skip)]
    pub reg_cache_misses: u64,
}

impl fission_solver::solver::MemoryOracle for MachineState {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8> {
        self.read_space_readonly(space_id, addr, 1).ok().map(|v| v[0])
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
            reg_cache: std::collections::HashMap::new(),
            reg_cache_hits: 0,
            reg_cache_misses: 0,
        }
    }

    /// Drop persistent register cache (TTD restore / snapshot).
    pub fn invalidate_reg_cache(&mut self) {
        self.reg_cache.clear();
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
            self.spaces.insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        space.theory_array_id = Some(id);
    }

    pub fn read_space(&mut self, space_id: u64, addr: u64, size: usize) -> Result<Vec<u8>> {
        if space_id == 0 {
            // const space: we shouldn't really read from it this way, but just in case
            bail!("Attempted to read from const space via memory read");
        }
        // Hot path: 1–8 byte register reads via persistent cache.
        if space_id == self.spaces_layout.register && (1..=8).contains(&size) && addr % 8 == 0 {
            let key = addr;
            if let Some(&cached) = self.reg_cache.get(&key) {
                self.reg_cache_hits = self.reg_cache_hits.saturating_add(1);
                let mut out = vec![0u8; size];
                for i in 0..size {
                    out[i] = ((cached >> (i * 8)) & 0xff) as u8;
                }
                return Ok(out);
            }
            self.reg_cache_misses = self.reg_cache_misses.saturating_add(1);
        }
        if self.enforce_page_faults && space_id == self.spaces_layout.ram {
            use crate::pcode::page_map::AccessKind;
            self.page_map
                .check_range(addr, size, AccessKind::Read)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        if !self.spaces.contains_key(&space_id) {
            self.spaces.insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        let data = space.read(addr, size)?;

        if space_id == self.spaces_layout.register && (1..=8).contains(&size) && addr % 8 == 0 {
            let mut val = 0u64;
            for (i, &b) in data.iter().enumerate() {
                val |= (b as u64) << (i * 8);
            }
            // Cache full 8-byte window (partial reads still seed the slot).
            if size == 8 {
                self.reg_cache.insert(addr, val);
            }
        }
        
        if self.tracing_memory && space_id == self.spaces_layout.ram {
            self.trace_mem_reads.push((addr, data.clone()));
        }
        
        Ok(data)
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
                space.read(addr, data.len()).unwrap_or_else(|_| vec![0; data.len()])
            } else {
                vec![0; data.len()]
            };
            self.trace_mem_writes.push((addr, old, data.to_vec()));
        }

        if !self.spaces.contains_key(&space_id) {
            self.spaces.insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        space.write(addr, data)?;

        // Keep persistent register cache coherent with writes.
        if space_id == self.spaces_layout.register {
            if data.len() == 8 && addr % 8 == 0 {
                let mut val = 0u64;
                for (i, &b) in data.iter().enumerate() {
                    val |= (b as u64) << (i * 8);
                }
                self.reg_cache.insert(addr, val);
            } else {
                // Partial/unaligned write: drop any overlapping 8-byte slots.
                let start = addr & !7;
                let end = addr.saturating_add(data.len() as u64);
                let mut k = start;
                while k < end {
                    self.reg_cache.remove(&k);
                    k = k.saturating_add(8);
                }
            }
        }

        // When writing concrete bytes, clear their shadow memory taint.
        for i in 0..data.len() {
            let curr_addr = addr + i as u64;
            let old_node = space.clear_shadow(curr_addr);
            if self.tracing_memory && old_node.is_some() {
                self.trace_shadow_writes.push((space_id, curr_addr, old_node, None));
            }
        }

        Ok(())
    }

    pub fn set_shadow_memory(&mut self, space_id: u64, addr: u64, node: u32) {
        if !self.spaces.contains_key(&space_id) {
            self.spaces.insert(space_id, AddressSpace::new(format!("space_{}", space_id)));
        }
        let space = self.spaces.get_mut(&space_id).unwrap();
        let old_node = space.set_shadow(addr, node);
        if self.tracing_memory {
            self.trace_shadow_writes.push((space_id, addr, old_node, Some(node)));
        }
    }

    pub fn get_shadow_memory(&self, space_id: u64, addr: u64) -> Option<u32> {
        self.spaces.get(&space_id).and_then(|s| s.get_shadow(addr))
    }

    pub fn clear_shadow_memory(&mut self, space_id: u64, addr: u64) {
        if let Some(space) = self.spaces.get_mut(&space_id) {
            let old_node = space.clear_shadow(addr);
            if self.tracing_memory && old_node.is_some() {
                self.trace_shadow_writes.push((space_id, addr, old_node, None));
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
        state.write_space(ram, 0x1000, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
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
