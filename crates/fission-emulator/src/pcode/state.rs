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

/// The most a single [`MachineState::read_space`] returns. Far more than any
/// guest buffer this emulator handles, and the point is only that a length
/// the guest made up is refused as an error instead of being allocated.
pub const MAX_GUEST_BUFFER: usize = 1 << 30;

pub type MemoryAccessHook = std::sync::Arc<dyn Fn(&MemoryAccess) + Send + Sync>;

/// The contents of RAM nobody mapped, byte by byte.
///
/// A harness that runs code against an initial state it defines everywhere
/// -- every byte a pure function of the address -- installs one. An access to
/// an unmapped page then maps it readable and writable, filled from this,
/// rather than faulting. Two evaluators given the same function read the
/// same bytes at every address without either knowing which values are
/// pointers.
pub type LazyMemory = std::sync::Arc<dyn Fn(u64) -> u8 + Send + Sync>;

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

    /// What unmapped RAM holds. `None` (the default) leaves an access to an
    /// unmapped page a fault. See [`LazyMemory`].
    #[serde(skip)]
    pub lazy_memory: Option<LazyMemory>,

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
    ///
    /// Serialised with the rest: the JIT writes registers here and nowhere
    /// else, so a snapshot saved without it lost every register a compiled
    /// block had set -- and loaded back as an empty slice, which the first
    /// register access then indexed past.
    pub host_reg_file: Box<[u8]>,
}

/// Bytes of register space mirrored for zero-callout JIT access.
/// How much of register space is mirrored in the flat host array.
///
/// Large enough for every architecture this emulator runs, which is what makes
/// the mirror worth having: a register outside it takes a call-out on every
/// access instead of a load. aarch64 puts `x0` at 0x4000 and its `V` registers
/// above 0x5000, so the old 0x2000 covered x86 and nothing else -- every
/// aarch64 register access was a call, and worse, a value written to one was
/// not visible in the backing store until the block exited.
pub const HOST_REG_FILE_SIZE: usize = 0x8000;

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
            lazy_memory: None,
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
        usize::try_from(offset)
            .ok()
            .and_then(|start| start.checked_add(size))
            .is_some_and(|end| end <= self.host_reg_file.len())
    }

    /// Give the register mirror the size the JIT compiles against. Compiled
    /// code addresses it as a flat `HOST_REG_FILE_SIZE` array, so one of any
    /// other length -- a state from somewhere that did not carry it -- is
    /// replaced, keeping what it held.
    pub fn normalize_host_reg_file(&mut self) {
        if self.host_reg_file.len() != HOST_REG_FILE_SIZE {
            let mut mirror = vec![0u8; HOST_REG_FILE_SIZE];
            let keep = self.host_reg_file.len().min(HOST_REG_FILE_SIZE);
            mirror[..keep].copy_from_slice(&self.host_reg_file[..keep]);
            self.host_reg_file = mirror.into_boxed_slice();
        }
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
        self.check_ram_access(
            space_id,
            addr,
            size,
            crate::pcode::page_map::AccessKind::Read,
        )?;
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

    /// Map every unmapped page `[addr, addr + size)` touches, filled from
    /// [`Self::lazy_memory`]. A page already mapped is left as it is --
    /// including its protection, so a write to read-only memory still
    /// faults. The fill is the page's initial content, not an access: it is
    /// neither traced nor observed.
    fn map_lazy_pages(&mut self, addr: u64, size: usize) {
        use crate::pcode::page_map::{PAGE_SIZE, page_align_down, prot};
        let Some(fill) = self.lazy_memory.clone() else {
            return;
        };
        let last = addr.wrapping_add(size.max(1) as u64 - 1);
        let mut page = page_align_down(addr);
        loop {
            if !self.page_map.is_mapped(page) {
                self.page_map
                    .map_region(page, PAGE_SIZE, prot::READ | prot::WRITE, true);
                let bytes: Vec<u8> = (0..PAGE_SIZE).map(|i| fill(page.wrapping_add(i))).collect();
                let ram = self.spaces_layout.ram;
                let space = self
                    .spaces
                    .entry(ram)
                    .or_insert_with(|| AddressSpace::new(format!("space_{ram}")));
                let _ = space.write(page, &bytes);
            }
            if page >= page_align_down(last) {
                break;
            }
            page = page.wrapping_add(PAGE_SIZE);
        }
    }

    /// Read `size` bytes into a fresh buffer.
    ///
    /// `size` is very often the guest's own number -- the count of a `write`,
    /// the length of a string it says it has -- so one beyond
    /// [`MAX_GUEST_BUFFER`] is refused before a host buffer of that size is
    /// asked for. Building the buffer first meant a count of `(size_t)-1`
    /// took the host down (`capacity overflow`) on its way to a page fault.
    pub fn read_space(&mut self, space_id: u64, addr: u64, size: usize) -> Result<Vec<u8>> {
        if size > MAX_GUEST_BUFFER {
            // Preserve page faults without populating lazy pages for a
            // request that will be rejected regardless of the mapping.
            if self.enforce_page_faults && space_id == self.spaces_layout.ram {
                self.page_map
                    .check_range(addr, size, crate::pcode::page_map::AccessKind::Read)
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
            bail!("guest read of {size} bytes at 0x{addr:X} is larger than any buffer it could be");
        }
        let mut out = vec![0u8; size];
        self.read_into(space_id, addr, &mut out)?;
        Ok(out)
    }

    /// The page-protection check of a RAM access, when faults are enforced.
    /// Anything else -- another space, or enforcement off -- passes.
    fn check_ram_access(
        &mut self,
        space_id: u64,
        addr: u64,
        size: usize,
        kind: crate::pcode::page_map::AccessKind,
    ) -> Result<()> {
        if !self.enforce_page_faults || space_id != self.spaces_layout.ram {
            return Ok(());
        }
        if self.page_map.check_range(addr, size, kind).is_err() {
            self.map_lazy_pages(addr, size);
            self.page_map
                .check_range(addr, size, kind)
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        Ok(())
    }

    /// Make `[addr, addr + len)` of RAM read as zero, as a kernel does when
    /// it hands out fresh pages.
    ///
    /// Not a guest access: no protection check (a mapping may be created
    /// read-only or with no access at all, and is zero all the same), no
    /// hooks. And not a write of `len` zeros: a page nothing has been stored
    /// in already reads as zero, so the cost is in the pages that exist, and
    /// a page wholly inside the range is dropped rather than overwritten.
    /// `len` is the guest's to choose -- an `mmap` length, a `.bss` size --
    /// and must not become host memory by being asked for.
    ///
    /// Under TTD tracing the bytes and shadow that go are recorded like any
    /// other write, so a seek back restores them.
    pub fn zero_range(&mut self, addr: u64, len: u64) {
        if len == 0 {
            return;
        }
        let ram = self.spaces_layout.ram;
        let Some(space) = self.spaces.get_mut(&ram) else {
            return;
        };
        let page_size = space.page_size;
        let last = addr.saturating_add(len - 1);
        let first_page = addr & !(page_size - 1);
        let last_page = last & !(page_size - 1);
        let span = (last_page - first_page) / page_size + 1;
        // Whichever is shorter: the pages of the range, or the pages there are.
        let mut pages: Vec<u64> = if span <= space.pages.len() as u64 {
            (0..span)
                .map(|i| first_page + i * page_size)
                .filter(|page| space.pages.contains_key(page))
                .collect()
        } else {
            space
                .pages
                .keys()
                .copied()
                .filter(|page| (first_page..=last_page).contains(page))
                .collect()
        };
        pages.sort_unstable();

        for page in pages {
            let from = addr.max(page);
            let to = last.min(page + (page_size - 1));
            let n = (to - from + 1) as usize;
            if self.tracing_memory {
                let mut old = vec![0u8; n];
                space.read_into(from, &mut old);
                if old.iter().any(|byte| *byte != 0) {
                    self.trace_mem_writes.push((from, old, vec![0u8; n]));
                }
                for offset in 0..n as u64 {
                    if let Some(node) = space.get_shadow(from + offset) {
                        self.trace_shadow_writes
                            .push((ram, from + offset, Some(node), None));
                    }
                }
            }
            if n as u64 == page_size {
                space.pages.remove(&page);
            } else {
                // `AddressSpace::write` clears the shadow of what it writes.
                let _ = space.write(from, &vec![0u8; n]);
            }
        }
    }

    /// Store `data` in RAM the way a kernel fills a page it is handing out --
    /// a file's bytes behind a new mapping -- which no page protection
    /// applies to: a read-only mapping of a file holds the file. Otherwise a
    /// write like any other (traced, and it clears the shadow).
    pub fn write_ram_unprotected(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        let enforce = std::mem::replace(&mut self.enforce_page_faults, false);
        let ram = self.spaces_layout.ram;
        let result = self.write_space(ram, addr, data);
        self.enforce_page_faults = enforce;
        result
    }

    /// Place a piece of a program image -- a segment, a section -- at `addr`:
    /// `bytes` first, zeros for the rest of its `len`.
    ///
    /// `len` is what the file's headers claim and `bytes` is what the file
    /// has. Only the second is copied; the remainder (`.bss`, usually) is
    /// zero without being written, so a header naming a segment of a
    /// terabyte costs what its mapping costs and no more. It used to be
    /// built as one host buffer of `len` bytes and then stored page by page.
    pub fn load_image_range(&mut self, addr: u64, len: u64, bytes: &[u8]) -> Result<()> {
        self.zero_range(addr, len);
        let take = bytes.len().min(usize::try_from(len).unwrap_or(usize::MAX));
        self.write_ram_unprotected(addr, &bytes[..take])
    }

    /// How many of the `max_len` bytes at `addr` a guest read reaches before
    /// it would fault. All of them where faults are not enforced, or where
    /// unmapped memory is filled on demand.
    pub fn readable_len(&self, addr: u64, max_len: usize) -> usize {
        if !self.enforce_page_faults || self.lazy_memory.is_some() {
            return max_len;
        }
        self.page_map.readable_extent(addr, max_len)
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

        self.check_ram_access(
            space_id,
            addr,
            data.len(),
            crate::pcode::page_map::AccessKind::Write,
        )?;

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
                let curr_addr = addr.wrapping_add(i as u64);
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

    fn mapped_state() -> MachineState {
        use crate::pcode::page_map::prot;
        let mut state = MachineState::new();
        state.enforce_page_faults = true;
        state.page_map.map_region(0x10_0000, 0x4000, prot::RW, true);
        state
    }

    fn ram_pages(state: &MachineState) -> usize {
        state
            .spaces
            .get(&state.ram_space())
            .map_or(0, |space| space.pages.len())
    }

    /// A read the guest sized is refused before a buffer of that size exists:
    /// `vec![0; usize::MAX]` is a panic (`capacity overflow`), and it came
    /// before the page check that would have refused the read.
    #[test]
    fn a_read_of_an_impossible_size_is_an_error_not_an_allocation() {
        let mut state = mapped_state();
        let ram = state.ram_space();
        for size in [usize::MAX, isize::MAX as usize, 1 << 40] {
            assert!(state.read_space(ram, 0x10_0000, size).is_err());
        }
        // The same without page protection to refuse it first.
        state.enforce_page_faults = false;
        assert!(state.read_space(ram, 0x10_0000, usize::MAX).is_err());
        assert!(
            state
                .read_space(ram, 0x10_0000, MAX_GUEST_BUFFER + 1)
                .is_err()
        );

        // Positive control: a read that fits is served.
        state.write_space(ram, 0x10_0000, b"abc").unwrap();
        assert_eq!(state.read_space(ram, 0x10_0000, 3).unwrap(), b"abc");
    }

    /// `zero_range` makes the range read as zero, drops the pages wholly
    /// inside it instead of storing zeros in them, and touches nothing
    /// outside it -- for any length, in time that depends on the pages there
    /// are and not on the length asked for.
    #[test]
    fn zero_range_drops_whole_pages_and_clears_the_edges() {
        let mut state = mapped_state();
        let ram = state.ram_space();
        state.write_space(ram, 0x10_0000, &[0xAA; 0x4000]).unwrap();
        assert_eq!(ram_pages(&state), 4);

        state.zero_range(0x10_0800, 0x2800);
        assert_eq!(ram_pages(&state), 2, "the two pages wholly inside are gone");
        let after = state.read_space(ram, 0x10_0000, 0x4000).unwrap();
        assert!(
            after[..0x800].iter().all(|b| *b == 0xAA),
            "before the range"
        );
        assert!(after[0x800..0x3000].iter().all(|b| *b == 0), "the range");
        assert!(
            after[0x3000..].iter().all(|b| *b == 0xAA),
            "after the range"
        );

        // A length no host could back, over the same memory.
        state.zero_range(0, u64::MAX);
        assert_eq!(ram_pages(&state), 0);
        assert_eq!(state.read_space(ram, 0x10_3ff0, 16).unwrap(), [0u8; 16]);
    }

    /// Zeroing is not subject to the mapping's protection (a fresh read-only
    /// mapping is zero too), and under tracing it records what it removed so
    /// that a seek back can restore it.
    #[test]
    fn zero_range_ignores_protection_and_is_traced() {
        use crate::pcode::page_map::prot;
        let mut state = mapped_state();
        let ram = state.ram_space();
        state.write_space(ram, 0x10_1000, b"secret").unwrap();
        state.set_shadow_memory(ram, 0x10_1002, 7);
        state.page_map.mprotect(0x10_0000, 0x4000, prot::READ);
        assert!(state.write_space(ram, 0x10_1000, b"x").is_err());

        state.tracing_memory = true;
        state.zero_range(0x10_1000, 0x1000);
        assert_eq!(state.read_space(ram, 0x10_1000, 6).unwrap(), [0u8; 6]);
        assert_eq!(state.get_shadow_memory(ram, 0x10_1002), None);
        let (addr, old, new) = state.trace_mem_writes.last().expect("recorded");
        assert_eq!(
            (*addr, &old[..6], new.len()),
            (0x10_1000, &b"secret"[..], 0x1000)
        );
        assert!(new.iter().all(|b| *b == 0));
        assert_eq!(
            state.trace_shadow_writes.last(),
            Some(&(ram, 0x10_1002, Some(7), None))
        );

        // A page that held nothing is not a change, and is not recorded.
        let recorded = state.trace_mem_writes.len();
        state.zero_range(0x10_2000, 0x1000);
        assert_eq!(state.trace_mem_writes.len(), recorded);
    }

    /// With lazy memory installed, an unmapped page reads what the function
    /// says, takes writes, and records neither its fill nor anything else as
    /// an access; a mapped page keeps its contents and its protection.
    #[test]
    fn lazy_memory_fills_unmapped_pages_and_leaves_mapped_ones_alone() {
        use crate::pcode::page_map::prot;
        let mut state = MachineState::new();
        state.enforce_page_faults = true;
        state.tracing_memory = true;
        let ram = state.ram_space();
        state.page_map.map_region(0x1000, 0x1000, prot::READ, true);

        // No provider: still a fault.
        assert!(state.read_space(ram, 0x9000, 4).is_err());

        state.lazy_memory = Some(std::sync::Arc::new(|addr| (addr as u8) ^ 0x5a));
        let want: Vec<u8> = (0x9ffeu64..0xa002).map(|a| (a as u8) ^ 0x5a).collect();
        assert_eq!(
            state.read_space(ram, 0x9ffe, 4).unwrap(),
            want,
            "a read across two unmapped pages reads both fills"
        );
        assert!(state.trace_mem_writes.is_empty(), "the fill is not a write");

        state.write_space(ram, 0x2_0004, &[1, 2]).unwrap();
        assert_eq!(
            state.read_space(ram, 0x2_0003, 4).unwrap(),
            [0x03 ^ 0x5a, 1, 2, 0x06 ^ 0x5a]
        );

        // The read-only page mapped before keeps its zeros and refuses writes.
        assert_eq!(state.read_space(ram, 0x1000, 2).unwrap(), [0, 0]);
        assert!(state.write_space(ram, 0x1000, &[9]).is_err());
    }

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
