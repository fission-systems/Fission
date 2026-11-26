//! Platform-specific backends for dynamic debugging.
//!
//! Each OS subtree owns:
//! - **`memory`**: [`PlatformMemory`] implementation (process memory PAL).
//! - **`debugger`**: attach/breakpoints/registers (`Debugger` trait impl).
//!
//! # Architecture
//!
//! ```text
//! platform/
//! ├── mod.rs              - PlatformMemory trait + exports
//! ├── windows/
//! │   ├── memory.rs
//! │   └── debugger/       - Win32 Debug API (+ process enumeration)
//! ├── linux/
//! │   ├── memory.rs
//! │   └── debugger.rs     - ptrace
//! └── macos/
//!     ├── memory.rs
//!     └── debugger.rs     - stub
//! ```

use super::memory::{MemoryError, MemoryRegion};

// Shared by the `ptrace` backends; compiled everywhere so its tests run on
// every host.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
pub(crate) mod trap;

// The byte layouts the Windows backend reads; compiled everywhere so their
// tests run on every host.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) mod windows_layout;

// Platform-specific modules
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::{WindowsDebugger as PlatformDebugger, WindowsMemory, enumerate_processes};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::{LinuxDebugger as PlatformDebugger, LinuxMemory, enumerate_processes};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::{MacOSDebugger as PlatformDebugger, MacOSMemory, enumerate_processes};

/// Platform-agnostic trait for process memory operations
///
/// This trait defines the low-level byte-transfer primitive that all platform-specific
/// memory backends must provide. Callers use [`crate::debug::memory::MemoryManager`] to
/// enforce exact reads ([`read`](crate::debug::memory::MemoryManager::read)), explicit
/// partial prefix reads ([`read_partial`](crate::debug::memory::MemoryManager::read_partial)),
/// or gap-aware sparse acquisitions ([`read_sparse`](crate::debug::memory::MemoryManager::read_sparse)).
///
/// # Example
///
/// ```ignore
/// use crate::debug::memory::MemoryManager;
///
/// fn dump_memory(mem: &MemoryManager, address: u64) {
///     if let Ok(data) = mem.read_exact(address, 16) {
///         println!("{:?}", data);
///     }
/// }
/// ```
pub trait PlatformMemory: Send {
    /// Open a process for memory operations
    ///
    /// # Arguments
    /// * `pid` - The process ID to attach to
    ///
    /// # Errors
    /// Returns `MemoryError` if the process cannot be opened (access denied, not found, etc.)
    fn open_process(&mut self, pid: u32) -> Result<(), MemoryError>;

    /// Read memory from the target process into a buffer
    ///
    /// # Arguments
    /// * `address` - The virtual address to read from
    /// * `buffer` - The buffer to read into
    ///
    /// # Returns
    /// The number of contiguous leading bytes transferred into `buffer[..count]`.
    /// Implementations MUST return a count `<= buffer.len()` and MUST NOT expect
    /// callers to treat unread tail bytes in `buffer[count..]` as observed data.
    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError>;

    /// Write memory to the target process
    ///
    /// # Arguments
    /// * `address` - The virtual address to write to
    /// * `data` - The data to write
    ///
    /// # Returns
    /// The number of bytes successfully written
    fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError>;

    /// Query memory regions of the target process
    ///
    /// # Returns
    /// A vector of `MemoryRegion` describing the process's memory layout
    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError>;

    /// Check if a process is currently open
    fn is_open(&self) -> bool;
}

// ============================================================================
// Platform Type Alias
// ============================================================================

/// The platform-specific memory implementation for the current OS
#[cfg(target_os = "windows")]
pub type PlatformMemoryImpl = WindowsMemory;

#[cfg(target_os = "linux")]
pub type PlatformMemoryImpl = LinuxMemory;

#[cfg(target_os = "macos")]
pub type PlatformMemoryImpl = MacOSMemory;

/// Create a new platform-specific memory manager
pub fn new_platform_memory() -> PlatformMemoryImpl {
    PlatformMemoryImpl::new()
}
