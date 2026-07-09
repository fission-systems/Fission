//! Guest page map with protection flags.
//!
//! Cleanroom design inspired by QEMU linux-user page protection concepts
//! (`PAGE_READ` / `PAGE_WRITE` / `PAGE_EXEC` / `PAGE_VALID`), without any
//! vendor code dependency or ABI coupling.
//!
//! Responsibilities:
//! - track mapped virtual regions and per-page protections
//! - gate access checks for R/W/X
//! - surface executable-page writes for SMC (JIT cache invalidation)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Page protection bits (same shape as POSIX PROT_* / QEMU PAGE_*).
pub mod prot {
    pub const NONE: u8 = 0;
    pub const READ: u8 = 0x01;
    pub const WRITE: u8 = 0x02;
    pub const EXEC: u8 = 0x04;
    pub const VALID: u8 = 0x08;
    /// Original write flag before W^X / SMC tracking demotes write permission.
    pub const WRITE_ORG: u8 = 0x10;
    /// Anonymous mapping (not file-backed).
    pub const ANON: u8 = 0x80;

    pub const RW: u8 = READ | WRITE;
    pub const RX: u8 = READ | EXEC;
    pub const RWX: u8 = READ | WRITE | EXEC;
}

pub const PAGE_SHIFT: u32 = 12;
pub const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;
pub const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

#[inline]
pub fn page_align_down(addr: u64) -> u64 {
    addr & PAGE_MASK
}

#[inline]
pub fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & PAGE_MASK
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    Read,
    Write,
    Execute,
}

impl AccessKind {
    pub fn required_prot(self) -> u8 {
        match self {
            AccessKind::Read => prot::READ,
            AccessKind::Write => prot::WRITE,
            AccessKind::Execute => prot::EXEC,
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum PageFault {
    #[error("page not mapped at 0x{addr:X} ({kind:?})")]
    NotMapped { addr: u64, kind: AccessKind },
    #[error("protection fault at 0x{addr:X} ({kind:?}, prot=0x{prot:02X})")]
    Prot {
        addr: u64,
        kind: AccessKind,
        prot: u8,
    },
}

/// A contiguous guest mapping.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuestMapping {
    pub start: u64,
    /// Exclusive end address (page-aligned).
    pub end: u64,
    pub prot: u8,
    pub anon: bool,
}

impl GuestMapping {
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// Sparse page flag table + region inventory for user-mode guest address space.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PageMap {
    /// Page base address → protection flags (must include VALID when mapped).
    flags: BTreeMap<u64, u8>,
    /// Ordered list of mappings (non-overlapping, coalesced where possible).
    mappings: Vec<GuestMapping>,
    /// Next address hint for anonymous `mmap` without a fixed base.
    pub mmap_hint: u64,
    /// Current program break (heap end) for `brk`.
    pub brk: u64,
    /// Initial program break (heap start).
    pub brk_base: u64,
}

impl PageMap {
    pub fn new() -> Self {
        Self {
            flags: BTreeMap::new(),
            mappings: Vec::new(),
            // High enough to avoid typical ELF load ranges; still below stack.
            mmap_hint: 0x0000_0000_6000_0000,
            brk: 0,
            brk_base: 0,
        }
    }

    pub fn mappings(&self) -> &[GuestMapping] {
        &self.mappings
    }

    pub fn set_brk_base(&mut self, base: u64) {
        let aligned = page_align_up(base);
        self.brk_base = aligned;
        self.brk = aligned;
    }

    /// Map `[start, start+len)` with the given protection. Overlapping pages are replaced.
    ///
    /// Page span is computed from the **original** half-open range, not from
    /// `align_down(start) + len`. Using the latter drops the trailing page when
    /// `start` is unaligned (e.g. ELF `.bss` at `0x…7D00` with size spanning
    /// into the next page) — silent `jit_write_space` failures then corrupt
    /// musl mallocng globals (`brk_cur`, size-class counters, locks).
    pub fn map_region(&mut self, start: u64, len: u64, mut page_prot: u8, anon: bool) {
        if len == 0 {
            return;
        }
        let end_addr = start.saturating_add(len);
        let start = page_align_down(start);
        let end = page_align_up(end_addr);
        page_prot |= prot::VALID;
        if anon {
            page_prot |= prot::ANON;
        }
        // Remember original write bit for SMC tracking.
        if page_prot & prot::WRITE != 0 {
            page_prot |= prot::WRITE_ORG;
        }

        let mut page = start;
        while page < end {
            self.flags.insert(page, page_prot);
            page += PAGE_SIZE;
        }

        self.mappings
            .retain(|m| m.end <= start || m.start >= end);
        self.mappings.push(GuestMapping {
            start,
            end,
            prot: page_prot,
            anon,
        });
        self.mappings.sort_by_key(|m| m.start);
        self.coalesce_mappings();
    }

    /// Unmap `[start, start+len)` (page-rounded).
    pub fn unmap_region(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        let end_addr = start.saturating_add(len);
        let start = page_align_down(start);
        let end = page_align_up(end_addr);
        let mut page = start;
        while page < end {
            self.flags.remove(&page);
            page += PAGE_SIZE;
        }
        // Split / trim overlapping mappings.
        let mut next = Vec::new();
        for m in self.mappings.drain(..) {
            if m.end <= start || m.start >= end {
                next.push(m);
                continue;
            }
            if m.start < start {
                next.push(GuestMapping {
                    start: m.start,
                    end: start,
                    prot: m.prot,
                    anon: m.anon,
                });
            }
            if m.end > end {
                next.push(GuestMapping {
                    start: end,
                    end: m.end,
                    prot: m.prot,
                    anon: m.anon,
                });
            }
        }
        self.mappings = next;
        self.mappings.sort_by_key(|m| m.start);
    }

    pub fn mprotect(&mut self, start: u64, len: u64, new_prot: u8) {
        let _ = self.mprotect_checked(start, len, new_prot);
    }

    /// Update protections on `[start, start+len)`. Returns `false` if any page
    /// in the range is unmapped (Linux `mprotect` → `-ENOMEM`).
    pub fn mprotect_checked(&mut self, start: u64, len: u64, mut new_prot: u8) -> bool {
        if len == 0 {
            return true;
        }
        let end_addr = start.saturating_add(len);
        let start = page_align_down(start);
        let end = page_align_up(end_addr);
        new_prot |= prot::VALID;
        if new_prot & prot::WRITE != 0 {
            new_prot |= prot::WRITE_ORG;
        }
        // Fail if any page is unmapped — do not partially apply.
        let mut page = start;
        while page < end {
            if !self
                .flags
                .get(&page)
                .is_some_and(|f| *f & prot::VALID != 0)
            {
                return false;
            }
            page += PAGE_SIZE;
        }
        page = start;
        while page < end {
            if let Some(f) = self.flags.get_mut(&page) {
                let anon = *f & prot::ANON;
                *f = new_prot | anon;
            }
            page += PAGE_SIZE;
        }
        // Update mapping metadata for overlapping ranges (best-effort).
        for m in &mut self.mappings {
            if m.end <= start || m.start >= end {
                continue;
            }
            // Full cover: replace prot.
            if m.start >= start && m.end <= end {
                m.prot = new_prot | if m.anon { prot::ANON } else { 0 };
            }
        }
        true
    }

    pub fn page_flags(&self, addr: u64) -> Option<u8> {
        self.flags.get(&page_align_down(addr)).copied()
    }

    pub fn is_mapped(&self, addr: u64) -> bool {
        self.page_flags(addr)
            .is_some_and(|f| f & prot::VALID != 0)
    }

    pub fn check_access(&self, addr: u64, kind: AccessKind) -> Result<(), PageFault> {
        match self.page_flags(addr) {
            None => Err(PageFault::NotMapped { addr, kind }),
            Some(f) if f & prot::VALID == 0 => Err(PageFault::NotMapped { addr, kind }),
            Some(f) if f & kind.required_prot() == 0 => Err(PageFault::Prot {
                addr,
                kind,
                prot: f,
            }),
            Some(_) => Ok(()),
        }
    }

    /// Check every byte of a multi-byte access.
    pub fn check_range(&self, addr: u64, size: usize, kind: AccessKind) -> Result<(), PageFault> {
        if size == 0 {
            return Ok(());
        }
        let end = addr.saturating_add(size as u64 - 1);
        let mut page = page_align_down(addr);
        let last = page_align_down(end);
        while page <= last {
            self.check_access(page, kind)?;
            if page == last {
                break;
            }
            page = page.saturating_add(PAGE_SIZE);
        }
        Ok(())
    }

    /// Allocate an anonymous region of `len` bytes (page-rounded) near `mmap_hint`.
    /// Returns the base address on success.
    pub fn mmap_anon(&mut self, len: u64, page_prot: u8) -> u64 {
        let len = page_align_up(len.max(1));
        let mut candidate = page_align_down(self.mmap_hint);
        // Simple linear scan for a free hole; wrap once if we hit high addresses.
        for _ in 0..4096 {
            if self.region_free(candidate, len) {
                self.map_region(candidate, len, page_prot | prot::ANON, true);
                self.mmap_hint = candidate.saturating_add(len);
                return candidate;
            }
            candidate = candidate.saturating_add(PAGE_SIZE);
            if candidate >= 0x0000_7FFF_0000_0000 {
                candidate = 0x0000_0000_4000_0000;
            }
        }
        // Fallback: force-map at hint even if overlapping (should be rare).
        let base = page_align_down(self.mmap_hint);
        self.map_region(base, len, page_prot | prot::ANON, true);
        self.mmap_hint = base.saturating_add(len);
        base
    }

    /// Adjust the program break. Returns the new break.
    ///
    /// Linux `brk` returns the **exact** requested address on success (musl
    /// compares `rax` to the request). Pages are mapped with page-aligned
    /// extents covering the expanded range.
    pub fn set_brk(&mut self, request: u64) -> u64 {
        if self.brk_base == 0 {
            // Lazy default heap if never initialized from the loader.
            self.set_brk_base(0x0000_0000_5000_0000);
        }
        if request == 0 {
            return self.brk;
        }
        // Failure: cannot move below heap base — return current break.
        if request < self.brk_base {
            return self.brk;
        }
        let old_brk = self.brk;
        if request > old_brk {
            let start = page_align_down(old_brk.max(self.brk_base));
            let end = page_align_up(request);
            if end > start {
                self.map_region(start, end - start, prot::RW | prot::ANON, true);
            }
        } else if request < old_brk {
            let unmap_from = page_align_up(request);
            let unmap_to = page_align_up(old_brk);
            if unmap_to > unmap_from {
                self.unmap_region(unmap_from, unmap_to - unmap_from);
            }
        }
        self.brk = request;
        request
    }

    /// Returns true if any page in `[addr, addr+size)` is executable.
    pub fn range_has_exec(&self, addr: u64, size: usize) -> bool {
        if size == 0 {
            return false;
        }
        let end = addr.saturating_add(size as u64 - 1);
        let mut page = page_align_down(addr);
        let last = page_align_down(end);
        while page <= last {
            if self
                .page_flags(page)
                .is_some_and(|f| f & prot::EXEC != 0)
            {
                return true;
            }
            if page == last {
                break;
            }
            page = page.saturating_add(PAGE_SIZE);
        }
        false
    }

    /// Collect distinct page bases in a write range that currently have EXEC.
    pub fn exec_pages_in_range(&self, addr: u64, size: usize) -> Vec<u64> {
        let mut out = Vec::new();
        if size == 0 {
            return out;
        }
        let end = addr.saturating_add(size as u64 - 1);
        let mut page = page_align_down(addr);
        let last = page_align_down(end);
        while page <= last {
            if self
                .page_flags(page)
                .is_some_and(|f| f & prot::EXEC != 0)
            {
                out.push(page);
            }
            if page == last {
                break;
            }
            page = page.saturating_add(PAGE_SIZE);
        }
        out
    }

    fn region_free(&self, start: u64, len: u64) -> bool {
        let end = start.saturating_add(len);
        !self
            .mappings
            .iter()
            .any(|m| m.start < end && m.end > start)
    }

    fn coalesce_mappings(&mut self) {
        if self.mappings.len() < 2 {
            return;
        }
        let mut out: Vec<GuestMapping> = Vec::with_capacity(self.mappings.len());
        for m in self.mappings.drain(..) {
            if let Some(last) = out.last_mut() {
                if last.end == m.start && last.prot == m.prot && last.anon == m.anon {
                    last.end = m.end;
                    continue;
                }
            }
            out.push(m);
        }
        self.mappings = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_and_check_access() {
        let mut pm = PageMap::new();
        pm.map_region(0x1000, 0x2000, prot::RW, true);
        assert!(pm.check_access(0x1000, AccessKind::Read).is_ok());
        assert!(pm.check_access(0x1000, AccessKind::Write).is_ok());
        assert!(pm.check_access(0x1000, AccessKind::Execute).is_err());
        assert!(pm.check_access(0x4000, AccessKind::Read).is_err());
    }

    #[test]
    fn mmap_anon_and_brk() {
        let mut pm = PageMap::new();
        pm.set_brk_base(0x5000_0000);
        assert_eq!(pm.set_brk(0), 0x5000_0000);
        // Exact return (not force page-aligned) — musl checks rax == request.
        let new = pm.set_brk(0x5000_1000);
        assert_eq!(new, 0x5000_1000);
        assert!(pm.is_mapped(0x5000_0000));
        // Non-page-aligned request still returns exact address.
        let mid = pm.set_brk(0x5000_1800);
        assert_eq!(mid, 0x5000_1800);
        assert!(pm.is_mapped(0x5000_1000));

        let base = pm.mmap_anon(0x3000, prot::RW);
        assert!(base >= 0x6000_0000 || base >= 0x4000_0000);
        assert!(pm.is_mapped(base));
        assert!(pm.is_mapped(base + 0x2000));
    }

    #[test]
    fn exec_pages_for_smc() {
        let mut pm = PageMap::new();
        pm.map_region(0x400000, 0x1000, prot::RX, false);
        pm.map_region(0x401000, 0x1000, prot::RW, true);
        assert_eq!(pm.exec_pages_in_range(0x400800, 0x1000), vec![0x400000]);
        assert!(pm.exec_pages_in_range(0x401000, 16).is_empty());
    }

    #[test]
    fn unmapped_access_faults() {
        let pm = PageMap::new();
        assert!(matches!(
            pm.check_access(0xdead_beef, AccessKind::Read),
            Err(PageFault::NotMapped { .. })
        ));
    }

    /// ELF `.bss` is often unaligned (e.g. `0x1007D00`, size `0x9E0` → ends
    /// `0x10086E0`). Both the first and second pages must be mapped.
    #[test]
    fn map_region_unaligned_start_covers_tail_page() {
        let mut pm = PageMap::new();
        // Mirrors static musl .bss layout that held mallocng brk_cur / bins.
        pm.map_region(0x1007D00, 0x9E0, prot::RW, false);
        assert!(pm.is_mapped(0x1007D00), "bss base");
        assert!(pm.is_mapped(0x1007F20), "init flag page");
        assert!(
            pm.is_mapped(0x10082B0),
            "tail page (brk_cur) must be mapped — align_down(start)+len drops it"
        );
        assert!(pm.is_mapped(0x10086DF), "last bss byte");
        assert!(!pm.is_mapped(0x1009000), "must not over-map past page_align_up(end)");
    }

    #[test]
    fn mprotect_unmapped_returns_false() {
        let mut pm = PageMap::new();
        pm.map_region(0x1000, 0x1000, prot::RW, true);
        assert!(pm.mprotect_checked(0x1000, 0x1000, prot::READ));
        assert!(!pm.mprotect_checked(0x2000, 0x1000, prot::RW), "unmapped");
        assert!(
            !pm.mprotect_checked(0x1000, 0x2000, prot::RW),
            "partial unmapped must fail"
        );
    }
}
