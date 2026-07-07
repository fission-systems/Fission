use std::collections::BTreeMap;

pub struct DummyHeap {
    pub base_addr: u64,
    pub next_addr: u64,
    pub allocations: BTreeMap<u64, usize>,
}

impl DummyHeap {
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            next_addr: base_addr,
            allocations: BTreeMap::new(),
        }
    }

    pub fn alloc(&mut self, size: usize) -> u64 {
        let addr = self.next_addr;
        let alloc_size = (size as u64 + 15) & !15; // 16-byte align
        self.next_addr += alloc_size;
        self.allocations.insert(addr, size);
        addr
    }

    pub fn free(&mut self, addr: u64) -> bool {
        self.allocations.remove(&addr).is_some()
    }
    
    pub fn realloc(&mut self, old_addr: u64, new_size: usize) -> Option<u64> {
        if self.free(old_addr) {
            Some(self.alloc(new_size))
        } else {
            None
        }
    }
}
