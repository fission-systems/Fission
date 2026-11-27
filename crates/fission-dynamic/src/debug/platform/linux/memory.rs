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
    Ok(parse_process_mappings(&content, &path))
}

fn parse_process_mappings(content: &str, path: &str) -> (Vec<LinuxProcessMapping>, Vec<String>) {
    let mut mappings = Vec::new();
    let mut diagnostics = Vec::new();

    for (index, line) in content.lines().enumerate() {
        match LinuxMemory::parse_maps_line(line) {
            Some(mapping) => mappings.push(mapping),
            None => diagnostics.push(format!("could not parse {path} line {}", index + 1)),
        }
    }

    if mappings.is_empty() && diagnostics.is_empty() {
        diagnostics.push(format!("{path} contained no mapping records"));
    }

    (mappings, diagnostics)
}

/// Parse Linux `starttime` (22nd space-separated field in `/proc/<pid>/stat`, i.e. index 19 after `) `).
pub fn parse_proc_stat_start_time(stat_content: &str) -> Option<u64> {
    let close_paren = stat_content.rfind(')')?;
    let tail = stat_content.get(close_paren + 1..)?;
    // Fields after `(comm)`: index 0 is `state` (field 3), so field 22 (`starttime`) is index 19.
    tail.split_whitespace().nth(19)?.parse::<u64>().ok()
}

/// Linux-specific memory manager
///
/// Uses the procfs interface for process memory operations.
/// The target process must be traceable (same user or root).
pub struct LinuxMemory {
    /// Target process ID
    target_pid: Option<u32>,
    /// Start time from `/proc/{pid}/stat` captured on `open_process` to detect PID reuse.
    expected_start_time: Option<u64>,
}

impl LinuxMemory {
    /// Create a new Linux memory manager
    pub fn new() -> Self {
        Self {
            target_pid: None,
            expected_start_time: None,
        }
    }

    /// Return a stable process lifetime token derived from PID and procfs start time.
    pub fn process_lifetime_token(&self) -> Option<String> {
        let pid = self.target_pid?;
        let start_time = self.expected_start_time?;
        Some(format!("linux-proc-start:{pid}:{start_time}"))
    }

    fn read_proc_stat_start_time(pid: u32) -> Option<u64> {
        let stat_path = format!("/proc/{pid}/stat");
        let content = std::fs::read_to_string(stat_path).ok()?;
        parse_proc_stat_start_time(&content)
    }

    fn verify_process_identity(&self, pid: u32, address: u64) -> Result<(), MemoryError> {
        let Some(expected) = self.expected_start_time else {
            return Ok(());
        };
        match Self::read_proc_stat_start_time(pid) {
            Some(actual) if actual == expected => Ok(()),
            _ => Err(MemoryError::ProcessExited { address }),
        }
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
        // On Linux, we record PID and procfs `starttime` to guard against PID reuse.
        let maps_path = Self::proc_maps_path(pid);
        if std::path::Path::new(&maps_path).exists() {
            self.target_pid = Some(pid);
            self.expected_start_time = Self::read_proc_stat_start_time(pid);
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
        self.verify_process_identity(pid, address)?;
        if buffer.is_empty() {
            return Ok(0);
        }
        let mem_path = Self::proc_mem_path(pid);

        let mut file = File::open(&mem_path).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => MemoryError::AccessDenied { address },
            std::io::ErrorKind::NotFound => MemoryError::ProcessExited { address },
            _ if matches!(e.raw_os_error(), Some(3)) => MemoryError::ProcessExited { address },
            _ => MemoryError::ReadFailed {
                address,
                reason: format!("Failed to open {}: {}", mem_path, e),
            },
        })?;

        file.seek(SeekFrom::Start(address))
            .map_err(|e| MemoryError::ReadFailed {
                address,
                reason: format!("Seek failed: {}", e),
            })?;

        let bytes_read = file.read(buffer).map_err(|e| match e.raw_os_error() {
            Some(13) => MemoryError::AccessDenied { address }, // EACCES
            Some(3) => MemoryError::ProcessExited { address }, // ESRCH
            Some(5 | 14) => MemoryError::Unmapped {
                address,
                size: buffer.len(),
            }, // EIO / EFAULT
            _ => MemoryError::ReadFailed {
                address,
                reason: format!("Read failed: {}", e),
            },
        })?;

        crate::debug::memory::checked_transfer_len(address, buffer.len(), bytes_read)
    }

    fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        let pid = self.get_pid()?;
        self.verify_process_identity(pid, address)?;
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
        self.verify_process_identity(pid, 0)?;
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
    use super::{LinuxMemory, decode_proc_maps_path, parse_process_mappings};

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
    fn empty_proc_maps_response_is_reported_incomplete() {
        let (mappings, diagnostics) = parse_process_mappings("", "/proc/123/maps");

        assert!(mappings.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("no mapping records"));
    }

    #[test]
    fn proc_maps_path_decoder_leaves_non_escape_backslashes_alone() {
        assert_eq!(decode_proc_maps_path("/tmp/a\\name"), "/tmp/a\\name");
    }

    #[test]
    fn proc_stat_start_time_parser_handles_spaces_and_nested_parens() {
        // Fields 1..=22 where comm contains spaces and parentheses.
        let stat = "1234 (worker (v2) main) S 1 1234 1234 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 4 0 987654321 1000 10";
        assert_eq!(super::parse_proc_stat_start_time(stat), Some(987_654_321));
        assert_eq!(super::parse_proc_stat_start_time("1234 (short) S 1"), None);
    }
}
