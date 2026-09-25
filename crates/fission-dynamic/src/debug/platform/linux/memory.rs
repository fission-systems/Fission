//! Linux Memory Implementation
//!
//! Provides process memory operations using Linux procfs:
//! - `/proc/{pid}/mem` for memory read/write
//! - `/proc/{pid}/maps` for memory region enumeration
//!
//! Note: Requires ptrace attachment or same-user ownership for access.

use super::super::PlatformMemory;
use crate::debug::memory::{MemoryError, MemoryProtection, MemoryRegion};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

/// Full procfs mapping facts, including the file coordinate omitted by the
/// cross-platform memory-region API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LinuxProcessMapping {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub file_offset: u64,
    pub device: String,
    pub inode: u64,
    pub path: Option<String>,
}

/// Read every mapping record and retain malformed-line diagnostics instead of
/// silently presenting a filtered list as complete.
pub(super) fn read_process_mappings(
    pid: u32,
) -> std::io::Result<(Vec<LinuxProcessMapping>, Vec<String>)> {
    let path = format!("/proc/{pid}/maps");
    let content = std::fs::read_to_string(&path)?;
    let mut mappings = Vec::new();
    let mut diagnostics = Vec::new();

    for (index, line) in content.lines().enumerate() {
        match LinuxMemory::parse_maps_line(line) {
            Some(mapping) => mappings.push(mapping),
            None => diagnostics.push(format!("could not parse {path} line {}", index + 1)),
        }
    }

    Ok((mappings, diagnostics))
}

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
    fn parse_maps_line(line: &str) -> Option<LinuxProcessMapping> {
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
        if end <= start {
            return None;
        }

        let permissions = parts[1];
        if permissions.len() != 4 {
            return None;
        }
        let file_offset = u64::from_str_radix(parts[2], 16).ok()?;
        let inode = parts[4].parse().ok()?;
        let path = if parts.len() >= 6 {
            Some(decode_proc_maps_path(&parts[5..].join(" ")))
        } else {
            None
        };

        Some(LinuxProcessMapping {
            start,
            end,
            permissions: permissions.to_string(),
            file_offset,
            device: parts[3].to_string(),
            inode,
            path,
        })
    }
}

fn decode_proc_maps_path(encoded: &str) -> String {
    let encoded = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] == b'\\'
            && index + 3 < encoded.len()
            && encoded[index + 1..index + 4]
                .iter()
                .all(|byte| (b'0'..=b'7').contains(byte))
        {
            let value = u16::from(encoded[index + 1] - b'0') * 64
                + u16::from(encoded[index + 2] - b'0') * 8
                + u16::from(encoded[index + 3] - b'0');
            if let Ok(byte) = u8::try_from(value) {
                decoded.push(byte);
                index += 4;
            } else {
                decoded.push(encoded[index]);
                index += 1;
            }
        } else {
            decoded.push(encoded[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
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

        let (mappings, _) = read_process_mappings(pid).map_err(|e| MemoryError::ReadFailed {
            address: 0,
            reason: format!("Failed to read {}: {}", maps_path, e),
        })?;
        let regions = mappings
            .into_iter()
            .filter_map(|mapping| {
                Some(MemoryRegion {
                    base_address: mapping.start,
                    size: usize::try_from(mapping.end.checked_sub(mapping.start)?).ok()?,
                    protection: MemoryProtection {
                        read: mapping.permissions.contains('r'),
                        write: mapping.permissions.contains('w'),
                        execute: mapping.permissions.contains('x'),
                    },
                    name: mapping.path,
                })
            })
            .collect();

        Ok(regions)
    }

    fn is_open(&self) -> bool {
        self.target_pid.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{LinuxMemory, decode_proc_maps_path};

    #[test]
    fn proc_maps_parser_preserves_file_coordinates_and_decodes_path() {
        let mapping = LinuxMemory::parse_maps_line(
            r"7f120000-7f121000 r-xp 00002000 08:02 12345 /tmp/lib\040with\040space.so (deleted)",
        )
        .expect("valid proc maps row");

        assert_eq!(mapping.start, 0x7f12_0000);
        assert_eq!(mapping.end, 0x7f12_1000);
        assert_eq!(mapping.permissions, "r-xp");
        assert_eq!(mapping.file_offset, 0x2000);
        assert_eq!(mapping.device, "08:02");
        assert_eq!(mapping.inode, 12345);
        assert_eq!(
            mapping.path.as_deref(),
            Some("/tmp/lib with space.so (deleted)")
        );
    }

    #[test]
    fn proc_maps_parser_retains_anonymous_and_special_mapping_names() {
        let anonymous = LinuxMemory::parse_maps_line("7fff0000-7fff1000 rw-p 00000000 00:00 0")
            .expect("valid anonymous mapping");
        let stack = LinuxMemory::parse_maps_line("7fff1000-7fff2000 rw-p 00000000 00:00 0 [stack]")
            .expect("valid named mapping");

        assert_eq!(anonymous.path, None);
        assert_eq!(stack.path.as_deref(), Some("[stack]"));
        assert_eq!(anonymous.inode, 0);
    }

    #[test]
    fn proc_maps_parser_rejects_invalid_ranges_and_offsets() {
        assert!(LinuxMemory::parse_maps_line("1000-1000 r--p 0 00:00 0 [heap]").is_none());
        assert!(LinuxMemory::parse_maps_line("1000-2000 r--p nope 00:00 0 [heap]").is_none());
    }

    #[test]
    fn proc_maps_path_decoder_leaves_non_escape_backslashes_alone() {
        assert_eq!(decode_proc_maps_path("/tmp/a\\name"), "/tmp/a\\name");
    }
}
