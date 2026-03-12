//! Memory - Process memory operations
//!
//! Provides unified memory read/write/mapping operations across platforms.
//! Uses platform-specific implementations via the `platform` module.

use super::platform::{PlatformMemory, PlatformMemoryImpl};
use thiserror::Error;

/// Memory operation errors
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Failed to read memory at {address:#x}: {reason}")]
    ReadFailed { address: u64, reason: String },

    #[error("Failed to write memory at {address:#x}: {reason}")]
    WriteFailed { address: u64, reason: String },

    #[error("Invalid memory region: {address:#x} - {address:#x}")]
    InvalidRegion { address: u64, size: usize },

    #[error("Access denied at {address:#x}")]
    AccessDenied { address: u64 },

    #[error("No process attached")]
    NoProcess,
}

/// Memory protection flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryProtection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl MemoryProtection {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const NONE: Self = Self {
        read: false,
        write: false,
        execute: false,
    };
}

/// Represents a memory region in the target process
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start address of the region
    pub base_address: u64,

    /// Size of the region in bytes
    pub size: usize,

    /// Memory protection flags
    pub protection: MemoryProtection,

    /// Optional name (e.g., module name, "[stack]", "[heap]")
    pub name: Option<String>,
}

/// Memory manager for reading/writing process memory
///
/// This is a high-level wrapper around platform-specific memory operations.
/// It provides convenience methods for common operations like reading primitives
/// and strings.
///
/// # Example
///
/// ```ignore
/// let mut mem = MemoryManager::new();
/// mem.open_process(1234)?;
/// let value = mem.read_u64(0x7fff0000)?;
/// ```
pub struct MemoryManager {
    /// Platform-specific memory implementation
    platform: PlatformMemoryImpl,
    /// Cached memory regions
    regions: Vec<MemoryRegion>,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new() -> Self {
        Self {
            platform: PlatformMemoryImpl::new(),
            regions: Vec::new(),
        }
    }

    /// Open a process for memory operations
    pub fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        self.platform.open_process(pid)
    }

    /// Check if a process is currently open
    pub fn is_open(&self) -> bool {
        self.platform.is_open()
    }

    /// Read memory from the target process
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        let mut buffer = vec![0u8; size];
        self.read_into(address, &mut buffer)?;
        Ok(buffer)
    }

    /// Read memory into an existing buffer
    pub fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        self.platform.read_into(address, buffer)
    }

    /// Write memory to the target process
    pub fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        self.platform.write(address, data)
    }

    /// Get memory regions of the target process
    pub fn query_regions(&mut self) -> Result<&[MemoryRegion], MemoryError> {
        self.regions = self.platform.query_regions()?;
        Ok(&self.regions)
    }

    /// Read a null-terminated string from memory
    pub fn read_string(&self, address: u64, max_len: usize) -> Result<String, MemoryError> {
        let mut buffer = vec![0u8; max_len];
        let bytes_read = self.read_into(address, &mut buffer)?;

        // Find null terminator
        let null_pos = buffer.iter().position(|&b| b == 0).unwrap_or(bytes_read);

        String::from_utf8(buffer[..null_pos].to_vec()).map_err(|e| MemoryError::ReadFailed {
            address,
            reason: format!("Invalid UTF-8: {}", e),
        })
    }

    /// Read a u64 value from memory (little-endian)
    pub fn read_u64(&self, address: u64) -> Result<u64, MemoryError> {
        let data = self.read(address, 8)?;
        let bytes: [u8; 8] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u64".to_string(),
        })?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a u32 value from memory (little-endian)
    pub fn read_u32(&self, address: u64) -> Result<u32, MemoryError> {
        let data = self.read(address, 4)?;
        let bytes: [u8; 4] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u32".to_string(),
        })?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a u16 value from memory (little-endian)
    pub fn read_u16(&self, address: u64) -> Result<u16, MemoryError> {
        let data = self.read(address, 2)?;
        let bytes: [u8; 2] = data.try_into().map_err(|_| MemoryError::ReadFailed {
            address,
            reason: "Invalid data length for u16".to_string(),
        })?;
        Ok(u16::from_le_bytes(bytes))
    }

    /// Read a u8 value from memory
    pub fn read_u8(&self, address: u64) -> Result<u8, MemoryError> {
        let data = self.read(address, 1)?;
        Ok(data[0])
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
