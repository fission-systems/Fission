//! Virtual memory region enumeration for a live Windows process.
//!
//! Wraps `VirtualQueryEx` to walk the full address space and classify
//! each region by state, type, and protection.

use std::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_FREE, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE, MEM_RESERVE,
    MEMORY_BASIC_INFORMATION, PAGE_PROTECTION_FLAGS, VirtualQueryEx,
};

/// Describes a single virtual-memory region in the target process.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub base_address: u64,
    pub allocation_base: u64,
    pub size: usize,
    pub state: MemoryState,
    pub region_type: MemoryType,
    pub protection: PAGE_PROTECTION_FLAGS,
    pub allocation_protection: PAGE_PROTECTION_FLAGS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryState {
    Commit,
    Reserve,
    Free,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Image,
    Mapped,
    Private,
    Unknown,
}

impl MemoryRegion {
    /// True if `address` falls inside this region.
    pub fn contains(&self, address: u64) -> bool {
        address >= self.base_address && address < self.base_address + self.size as u64
    }

    /// True if region is executable (any EXECUTE protection).
    pub fn is_executable(&self) -> bool {
        use windows::Win32::System::Memory::*;
        let p = self.protection;
        p == PAGE_EXECUTE
            || p == PAGE_EXECUTE_READ
            || p == PAGE_EXECUTE_READWRITE
            || p == PAGE_EXECUTE_WRITECOPY
    }

    /// True if region is writable (any WRITE protection).
    pub fn is_writable(&self) -> bool {
        use windows::Win32::System::Memory::*;
        let p = self.protection;
        p == PAGE_READWRITE
            || p == PAGE_EXECUTE_READWRITE
            || p == PAGE_WRITECOPY
            || p == PAGE_EXECUTE_WRITECOPY
    }

    /// True if region is both writable and executable (common in packers / self-modifying code).
    pub fn is_writable_executable(&self) -> bool {
        self.is_writable() && self.is_executable()
    }
}

/// Walk the entire virtual address space of `process` and return every region.
pub fn enumerate_memory_regions(process: HANDLE) -> Vec<MemoryRegion> {
    let mut regions = Vec::new();
    let mut address: u64 = 0;

    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let size = unsafe {
            VirtualQueryEx(
                process,
                Some(address as *const c_void),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };

        if size == 0 {
            break;
        }

        regions.push(MemoryRegion {
            base_address: mbi.BaseAddress as u64,
            allocation_base: mbi.AllocationBase as u64,
            size: mbi.RegionSize,
            state: match mbi.State {
                MEM_COMMIT => MemoryState::Commit,
                MEM_RESERVE => MemoryState::Reserve,
                MEM_FREE => MemoryState::Free,
                _ => MemoryState::Unknown,
            },
            region_type: match mbi.Type {
                MEM_IMAGE => MemoryType::Image,
                MEM_MAPPED => MemoryType::Mapped,
                MEM_PRIVATE => MemoryType::Private,
                _ => MemoryType::Unknown,
            },
            protection: mbi.Protect,
            allocation_protection: mbi.AllocationProtect,
        });

        // Advance past this region
        let next = (mbi.BaseAddress as u64) + mbi.RegionSize as u64;
        if next <= address {
            break; // Overflow or stuck
        }
        address = next;
    }

    regions
}

/// Return only committed executable regions.
pub fn find_executable_regions(process: HANDLE) -> Vec<MemoryRegion> {
    enumerate_memory_regions(process)
        .into_iter()
        .filter(|r| r.state == MemoryState::Commit && r.is_executable())
        .collect()
}

/// Return committed regions that are both writable and executable.
pub fn find_writable_executable_regions(process: HANDLE) -> Vec<MemoryRegion> {
    enumerate_memory_regions(process)
        .into_iter()
        .filter(|r| r.state == MemoryState::Commit && r.is_writable_executable())
        .collect()
}
