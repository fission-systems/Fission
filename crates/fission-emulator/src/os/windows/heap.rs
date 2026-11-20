use std::collections::BTreeMap;

/// How far the heap may grow from its base. Enough for any program this
/// emulator runs, and small enough that a 32-bit process's heap stays clear
/// of the structures the loader places near the top of its address space.
pub const HEAP_SPAN: u64 = 0x4000_0000;

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

    /// A 16-byte-aligned block of `size` bytes, or 0 -- `NULL` -- when the
    /// heap cannot hold it. The arithmetic used to be unchecked, so a guest
    /// `malloc((size_t)-1)` overflowed (a panic in a debug build).
    pub fn alloc(&mut self, size: usize) -> u64 {
        let addr = self.next_addr;
        let Some(alloc_size) = (size as u64).checked_add(15).map(|n| n & !15) else {
            return 0;
        };
        let Some(next) = addr.checked_add(alloc_size) else {
            return 0;
        };
        if next - self.base_addr > HEAP_SPAN {
            return 0;
        }
        self.next_addr = next;
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
