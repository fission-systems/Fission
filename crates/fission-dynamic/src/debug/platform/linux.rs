//! Linux Memory Implementation
//!
//! Provides process memory operations using Linux procfs:
//! - `/proc/{pid}/mem` for memory read/write
//! - `/proc/{pid}/maps` for memory region enumeration
//!
//! Note: Requires ptrace attachment or same-user ownership for access.

use super::{MemoryError, MemoryProtection, MemoryRegion, PlatformMemory};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

/// Linux-specific memory manager
///
/// Uses the procfs interface for process memory operations.
/// The target process must be traceable (same user or root).
pub struct LinuxMemory {
    /// Target process ID
    target_pid: Option<u32>,
}

impl LinuxMemory {
    /// Create a new Linux memory manager
    pub fn new() -> Self {
        Self { target_pid: None }
    }

    /// Get the target PID if set
    fn get_pid(&self) -> Result<u32, MemoryError> {
        self.target_pid.ok_or(MemoryError::NoProcess)
    }

    /// Build the `/proc/{pid}/maps` path for the given process
    #[inline]
    fn proc_maps_path(pid: u32) -> String {
        format!("/proc/{}/maps", pid)
    }

    /// Build the `/proc/{pid}/mem` path for the given process
    #[inline]
    fn proc_mem_path(pid: u32) -> String {
        format!("/proc/{}/mem", pid)
    }

    /// Parse a line from /proc/{pid}/maps
    ///
    /// Format: address-address perms offset dev inode pathname
    /// Example: 00400000-00452000 r-xp 00000000 08:02 173521 /usr/bin/dbus-daemon
    fn parse_maps_line(line: &str) -> Option<MemoryRegion> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }

        // Parse address range
        let addr_parts: Vec<&str> = parts[0].split('-').collect();
        if addr_parts.len() != 2 {
            return None;
        }

        let start = u64::from_str_radix(addr_parts[0], 16).ok()?;
        let end = u64::from_str_radix(addr_parts[1], 16).ok()?;

        // Parse permissions (rwxp)
        let perms = parts[1];
        let protection = MemoryProtection {
            read: perms.contains('r'),
            write: perms.contains('w'),
            execute: perms.contains('x'),
        };

        // Get pathname if available
        let name = if parts.len() >= 6 {
            Some(parts[5..].join(" "))
        } else {
            None
        };

        Some(MemoryRegion {
            base_address: start,
            size: (end - start) as usize,
            protection,
            name,
        })
    }
}

impl Default for LinuxMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformMemory for LinuxMemory {
    fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        // On Linux, we just store the PID and access /proc/{pid}/mem on demand
        // Could add validation that the process exists here
        let maps_path = Self::proc_maps_path(pid);
        if std::path::Path::new(&maps_path).exists() {
            self.target_pid = Some(pid);
            Ok(())
        } else {
            Err(MemoryError::ReadFailed {
                address: 0,
                reason: format!("Process {} does not exist", pid),
            })
        }
    }

    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        let pid = self.get_pid()?;
        let mem_path = Self::proc_mem_path(pid);

        let mut file = File::open(&mem_path).map_err(|e| MemoryError::ReadFailed {
            address,
            reason: format!("Failed to open {}: {}", mem_path, e),
        })?;

        file.seek(SeekFrom::Start(address))
            .map_err(|e| MemoryError::ReadFailed {
                address,
                reason: format!("Seek failed: {}", e),
            })?;

        let bytes_read = file.read(buffer).map_err(|e| MemoryError::ReadFailed {
            address,
            reason: format!("Read failed: {}", e),
        })?;

        Ok(bytes_read)
    }

    fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        let pid = self.get_pid()?;
        let mem_path = Self::proc_mem_path(pid);

        let mut file = OpenOptions::new()
            .write(true)
            .open(&mem_path)
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: format!("Failed to open {} for writing: {}", mem_path, e),
            })?;

        file.seek(SeekFrom::Start(address))
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: format!("Seek failed: {}", e),
            })?;

        let bytes_written = file.write(data).map_err(|e| MemoryError::WriteFailed {
            address,
            reason: format!("Write failed: {}", e),
        })?;

        Ok(bytes_written)
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        let pid = self.get_pid()?;
        let maps_path = Self::proc_maps_path(pid);

        let content = std::fs::read_to_string(&maps_path).map_err(|e| MemoryError::ReadFailed {
            address: 0,
            reason: format!("Failed to read {}: {}", maps_path, e),
        })?;

        let regions = content.lines().filter_map(Self::parse_maps_line).collect();

        Ok(regions)
    }

    fn is_open(&self) -> bool {
        self.target_pid.is_some()
    }
}
