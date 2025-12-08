//! Memory - Process memory operations
//!
//! Provides unified memory read/write/mapping operations across platforms.

use anyhow::Result;
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
pub struct MemoryManager {
    /// Target process handle/PID
    #[cfg(target_os = "windows")]
    process_handle: Option<isize>,

    #[cfg(target_os = "linux")]
    target_pid: Option<u32>,

    /// Cached memory regions
    regions: Vec<MemoryRegion>,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            process_handle: None,
            #[cfg(target_os = "linux")]
            target_pid: None,
            regions: Vec::new(),
        }
    }

    /// Open a process for memory operations
    #[cfg(target_os = "windows")]
    pub fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

        let handle = unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, false, pid).map_err(|e| MemoryError::ReadFailed {
                address: 0,
                reason: e.to_string(),
            })?
        };

        self.process_handle = Some(handle.0 as isize);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        self.target_pid = Some(pid);
        Ok(())
    }

    /// Read memory from the target process
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        let mut buffer = vec![0u8; size];
        self.read_into(address, &mut buffer)?;
        Ok(buffer)
    }

    /// Read memory into an existing buffer
    pub fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        #[cfg(target_os = "windows")]
        {
            self.read_windows(address, buffer)
        }

        #[cfg(target_os = "linux")]
        {
            self.read_linux(address, buffer)
        }
    }

    /// Write memory to the target process
    pub fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        #[cfg(target_os = "windows")]
        {
            self.write_windows(address, data)
        }

        #[cfg(target_os = "linux")]
        {
            self.write_linux(address, data)
        }
    }

    /// Get memory regions of the target process
    pub fn query_regions(&mut self) -> Result<&[MemoryRegion], MemoryError> {
        // TODO: Implement platform-specific memory region enumeration
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

    /// Read a primitive value from memory
    pub fn read_u64(&self, address: u64) -> Result<u64, MemoryError> {
        let data = self.read(address, 8)?;
        Ok(u64::from_le_bytes(data.try_into().unwrap()))
    }

    pub fn read_u32(&self, address: u64) -> Result<u32, MemoryError> {
        let data = self.read(address, 4)?;
        Ok(u32::from_le_bytes(data.try_into().unwrap()))
    }

    pub fn read_u16(&self, address: u64) -> Result<u16, MemoryError> {
        let data = self.read(address, 2)?;
        Ok(u16::from_le_bytes(data.try_into().unwrap()))
    }

    pub fn read_u8(&self, address: u64) -> Result<u8, MemoryError> {
        let data = self.read(address, 1)?;
        Ok(data[0])
    }
}

// Windows-specific implementations
#[cfg(target_os = "windows")]
impl MemoryManager {
    fn read_windows(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

        let handle_val = self.process_handle.ok_or(MemoryError::NoProcess)?;
        // SAFETY: HANDLE is repr(transparent) wrapper around isize
        let handle: HANDLE = unsafe { std::mem::transmute(handle_val) };
        let mut bytes_read = 0usize;

        unsafe {
            ReadProcessMemory(
                handle,
                address as *const std::ffi::c_void,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                buffer.len(),
                Some(&mut bytes_read),
            )
            .map_err(|e| MemoryError::ReadFailed {
                address,
                reason: e.to_string(),
            })?;
        }

        Ok(bytes_read)
    }

    fn write_windows(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;

        let handle_val = self.process_handle.ok_or(MemoryError::NoProcess)?;
        // SAFETY: HANDLE is repr(transparent) wrapper around isize
        let handle: HANDLE = unsafe { std::mem::transmute(handle_val) };
        let mut bytes_written = 0usize;

        unsafe {
            WriteProcessMemory(
                handle,
                address as *const std::ffi::c_void,
                data.as_ptr() as *const std::ffi::c_void,
                data.len(),
                Some(&mut bytes_written),
            )
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: e.to_string(),
            })?;
        }

        Ok(bytes_written)
    }
}

// Linux-specific implementations
#[cfg(target_os = "linux")]
impl MemoryManager {
    fn read_linux(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let pid = self.target_pid.ok_or(MemoryError::NoProcess)?;
        let mem_path = format!("/proc/{}/mem", pid);

        let mut file = File::open(&mem_path).map_err(|e| MemoryError::ReadFailed {
            address,
            reason: e.to_string(),
        })?;

        file.seek(SeekFrom::Start(address))
            .map_err(|e| MemoryError::ReadFailed {
                address,
                reason: e.to_string(),
            })?;

        let bytes_read = file.read(buffer).map_err(|e| MemoryError::ReadFailed {
            address,
            reason: e.to_string(),
        })?;

        Ok(bytes_read)
    }

    fn write_linux(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};

        let pid = self.target_pid.ok_or(MemoryError::NoProcess)?;
        let mem_path = format!("/proc/{}/mem", pid);

        let mut file = OpenOptions::new()
            .write(true)
            .open(&mem_path)
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: e.to_string(),
            })?;

        file.seek(SeekFrom::Start(address))
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: e.to_string(),
            })?;

        let bytes_written = file.write(data).map_err(|e| MemoryError::WriteFailed {
            address,
            reason: e.to_string(),
        })?;

        Ok(bytes_written)
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
