//! macOS Memory Implementation
//!
//! Provides process memory operations using Mach VM APIs (`mach_vm_read_overwrite`,
//! `mach_vm_write`, `mach_vm_region`), supporting self-inspection via `mach_task_self_`
//! and foreign targets when authorized for task-port access.

use super::super::PlatformMemory;
use super::mach;
use crate::debug::memory::{MemoryError, MemoryProtection, MemoryRegion, checked_transfer_len};
use std::time::Duration;

enum TaskPortHandle {
    BorrowedSelf(mach::mach_port_t),
    Owned(mach::OwnedMachPort),
}

impl TaskPortHandle {
    fn raw(&self) -> mach::mach_port_t {
        match self {
            Self::BorrowedSelf(port) => *port,
            Self::Owned(owned) => owned.raw(),
        }
    }
}

/// macOS-specific memory manager backed by Mach task VM calls.
pub struct MacOSMemory {
    /// Target Mach task port (RAII-owned when acquired for a foreign PID).
    task: Option<TaskPortHandle>,
}

impl MacOSMemory {
    /// Create a new macOS memory manager.
    pub fn new() -> Self {
        Self { task: None }
    }

    fn release_task(&mut self) {
        self.task.take();
    }

    fn get_task(&self) -> Result<mach::mach_port_t, MemoryError> {
        self.task
            .as_ref()
            .map(TaskPortHandle::raw)
            .ok_or(MemoryError::NoProcess)
    }
}

impl Drop for MacOSMemory {
    fn drop(&mut self) {
        self.release_task();
    }
}

impl Default for MacOSMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformMemory for MacOSMemory {
    fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        self.release_task();
        if pid == std::process::id() {
            self.task = Some(TaskPortHandle::BorrowedSelf(mach::own_task_port()));
            return Ok(());
        }
        let task =
            mach::task_port_for_pid(pid, Duration::from_secs(5)).map_err(|err| match err {
                mach::TaskPortError::Denied(_) => MemoryError::AccessDenied { address: 0 },
                mach::TaskPortError::TimedOut { .. }
                | mach::TaskPortError::Busy { .. }
                | mach::TaskPortError::Disconnected { .. }
                | mach::TaskPortError::NullPort { .. } => MemoryError::ReadFailed {
                    address: 0,
                    reason: format!("{err:?}: {}", err.remediation()),
                },
            })?;
        self.task = Some(TaskPortHandle::Owned(task));
        Ok(())
    }

    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        let task = self.get_task()?;
        if buffer.is_empty() {
            return Ok(0);
        }
        match mach::read_into_raw(task, address, buffer) {
            Ok(read) => checked_transfer_len(address, buffer.len(), read),
            Err(kr) => {
                if kr == mach::KERN_INVALID_TASK || kr == mach::MACH_SEND_INVALID_DEST {
                    return Err(MemoryError::ProcessExited { address });
                }
                if kr == mach::KERN_PROTECTION_FAILURE {
                    return Err(MemoryError::AccessDenied { address });
                }
                if kr == mach::KERN_INVALID_ADDRESS {
                    // XNU can return KERN_INVALID_ADDRESS even for mapped `PROT_NONE`
                    // pages; check the VM map to distinguish unmapped vs access denied.
                    return match mach::region_protection(task, address) {
                        Ok((_, _, prot)) if (prot & mach::VM_PROT_READ) == 0 => {
                            Err(MemoryError::AccessDenied { address })
                        }
                        _ => Err(MemoryError::Unmapped {
                            address,
                            size: buffer.len(),
                        }),
                    };
                }
                Err(MemoryError::ReadFailed {
                    address,
                    reason: format!("mach_vm_read_overwrite failed: kern_return_t {kr}"),
                })
            }
        }
    }

    fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        let task = self.get_task()?;
        mach::write_memory(task, address, data)
            .map_err(|reason| MemoryError::WriteFailed { address, reason })?;
        Ok(data.len())
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        let _task = self.get_task()?;
        let _ = MemoryProtection::RX;
        Ok(Vec::new())
    }

    fn is_open(&self) -> bool {
        self.task.is_some()
    }
}
