//! macOS-specific debugger implementation.
//!
//! This module provides debugging capabilities on macOS using the Mach API.
//! Currently a stub implementation - Mach debugging requires special entitlements.

use super::traits::Debugger;
use super::types::{DebugState, ProcessInfo, RegisterState};
use fission_core::{FissionError, Result as FissionResult};

/// macOS debugger implementation (stub)
///
/// Note: Full implementation requires:
/// - task_for_pid() which needs the com.apple.security.cs.debugger entitlement
/// - Code signing with the entitlement
/// - Running as root or being a debugger registered with the system
pub struct MacOSDebugger {
    /// Current debug state
    state: DebugState,
}

impl MacOSDebugger {
    /// Create a new macOS debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
        }
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }
}

impl Default for MacOSDebugger {
    fn default() -> Self {
        Self::new()
    }
}

/// Enumerate running processes on macOS
///
/// Uses sysctl to get process list (doesn't require special permissions)
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    // Use ps command as a simple approach
    // A more robust implementation would use sysctl with KERN_PROC
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "pid,comm"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
                if parts.len() >= 2 {
                    if let Ok(pid) = parts[0].trim().parse::<u32>() {
                        processes.push(ProcessInfo {
                            pid,
                            name: parts[1].trim().to_string(),
                            exe_path: None, // Would need to use proc_pidpath
                        });
                    }
                }
            }
        }
    }

    processes.sort_by_key(|p| p.pid);
    processes
}

impl Debugger for MacOSDebugger {
    fn enumerate_processes() -> Vec<ProcessInfo> {
        enumerate_processes()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        // task_for_pid requires special entitlements on macOS
        Err(FissionError::debug(format!(
            "macOS debugging not yet implemented. \
            Attaching to PID {} requires task_for_pid() which needs \
            the com.apple.security.cs.debugger entitlement.",
            pid
        )))
    }

    fn detach(&mut self) -> FissionResult<()> {
        Err(FissionError::debug("Not attached to any process"))
    }

    fn is_attached(&self) -> bool {
        false
    }

    fn attached_pid(&self) -> Option<u32> {
        None
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        Err(FissionError::debug("Not attached to any process"))
    }

    fn single_step(&mut self) -> FissionResult<()> {
        Err(FissionError::debug("Not attached to any process"))
    }

    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        Err(FissionError::debug(format!(
            "Cannot set breakpoint at 0x{:x}: not attached",
            address
        )))
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        Err(FissionError::debug(format!(
            "Cannot remove breakpoint at 0x{:x}: not attached",
            address
        )))
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        Err(FissionError::debug(format!(
            "Cannot read {} bytes at 0x{:x}: not attached",
            size, address
        )))
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        Err(FissionError::debug(format!(
            "Cannot write {} bytes at 0x{:x}: not attached",
            data.len(),
            address
        )))
    }

    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<RegisterState> {
        Err(FissionError::debug(format!(
            "Cannot fetch registers for thread {}: not attached",
            thread_id
        )))
    }
}
