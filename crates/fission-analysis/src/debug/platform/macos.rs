//! macOS Memory Implementation (Stub)
//!
//! macOS requires the Mach API (`task_for_pid`, `mach_vm_read`, etc.)
//! for process memory operations. This requires special entitlements
//! or running as root.
//!
//! # Current Status
//!
//! This is a stub implementation. Full support would require:
//! - Code signing with `com.apple.security.cs.debugger` entitlement
//! - Or running with `sudo`
//! - Implementation using mach_vm_* APIs

use super::{MemoryError, MemoryRegion, PlatformMemory};

/// macOS-specific memory manager (stub)
///
/// Currently unimplemented. macOS memory operations require:
/// - Special entitlements for process access
/// - Mach API (task_for_pid, mach_vm_read, etc.)
pub struct MacOSMemory {
    /// Target process ID (stored but not used in stub)
    _target_pid: Option<u32>,
}

impl MacOSMemory {
    /// Create a new macOS memory manager
    pub fn new() -> Self {
        Self { _target_pid: None }
    }
}

impl Default for MacOSMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformMemory for MacOSMemory {
    fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        // Store PID for future implementation (unused in stub)
        let _ = pid;

        // For now, return success but operations will fail
        // A full implementation would call task_for_pid here
        Ok(())
    }

    fn read_into(&self, address: u64, _buffer: &mut [u8]) -> Result<usize, MemoryError> {
        Err(MemoryError::ReadFailed {
            address,
            reason: "macOS memory reading requires Mach API (not yet implemented)".into(),
        })
    }

    fn write(&self, address: u64, _data: &[u8]) -> Result<usize, MemoryError> {
        Err(MemoryError::WriteFailed {
            address,
            reason: "macOS memory writing requires Mach API (not yet implemented)".into(),
        })
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        // A full implementation would use mach_vm_region
        // For now, return empty list
        Ok(Vec::new())
    }

    fn is_open(&self) -> bool {
        false // Stub implementation always returns false
    }
}
