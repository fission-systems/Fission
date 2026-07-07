use std::collections::HashMap;
use anyhow::{Result, bail};
use serde::{Serialize, Deserialize};

/// Represents a single address space in the emulated machine (e.g. ram, register, unique).
#[derive(Clone, Serialize, Deserialize)]
pub struct AddressSpace {
    pub name: String,
    // Page-based memory allocation (4KB pages) to avoid allocating huge blocks
    pub pages: HashMap<u64, Vec<u8>>,
    pub page_size: u64,
}

impl AddressSpace {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pages: HashMap::new(),
            page_size: 0x1000,
        }
    }

    fn get_page_mut(&mut self, addr: u64) -> &mut [u8] {
        let page_addr = addr & !(self.page_size - 1);
        self.pages
            .entry(page_addr)
            .or_insert_with(|| vec![0; self.page_size as usize])
    }

    fn get_page(&self, addr: u64) -> Option<&[u8]> {
        let page_addr = addr & !(self.page_size - 1);
        self.pages.get(&page_addr).map(|v| v.as_slice())
    }

    pub fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(size);
        for i in 0..size as u64 {
            let current_addr = addr + i;
            let offset = (current_addr & (self.page_size - 1)) as usize;
            let byte = match self.get_page(current_addr) {
                Some(page) => page[offset],
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
            page[offset] = byte;
        }
        Ok(())
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
    pub spaces: HashMap<u64, AddressSpace>,

    #[serde(skip)]
    pub hooks: Vec<MemoryAccessHook>,

    #[serde(skip)]
    pub tracing_memory: bool,
    #[serde(skip)]
    pub trace_mem_reads: Vec<(u64, Vec<u8>)>,
    #[serde(skip)]
    pub trace_mem_writes: Vec<(u64, Vec<u8>, Vec<u8>)>,  // (addr, old_bytes, new_bytes)
}

impl MachineState {
    pub fn new() -> Self {
        let mut spaces = HashMap::new();
        // Conventional space IDs: 0=const, 1=unique, 2=register, 3=ram
        spaces.insert(1, AddressSpace::new("unique"));
        spaces.insert(2, AddressSpace::new("register"));
        spaces.insert(3, AddressSpace::new("ram"));
        Self { 
            spaces, 
            hooks: Vec::new(),
            tracing_memory: false,
            trace_mem_reads: Vec::new(),
            trace_mem_writes: Vec::new(),
        }
    }

    pub fn read_space(&mut self, space_id: u64, addr: u64, size: usize) -> Result<Vec<u8>> {
        if space_id == 0 {
            // const space: we shouldn't really read from it this way, but just in case
            bail!("Attempted to read from const space via memory read");
        }
        let space = self.spaces.entry(space_id).or_insert_with(|| AddressSpace::new(format!("space_{}", space_id)));
        let data = space.read(addr, size)?;
        
        if self.tracing_memory && space_id == 3 {
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
        if self.tracing_memory && space_id == 3 {
            // Read the old value before overwriting so TTD can reconstruct undo deltas.
            let old = if let Some(space) = self.spaces.get(&space_id) {
                space.read(addr, data.len()).unwrap_or_else(|_| vec![0; data.len()])
            } else {
                vec![0; data.len()]
            };
            self.trace_mem_writes.push((addr, old, data.to_vec()));
        }
        
        let space = self.spaces.entry(space_id).or_insert_with(|| AddressSpace::new(format!("space_{}", space_id)));
        space.write(addr, data)
    }
}
