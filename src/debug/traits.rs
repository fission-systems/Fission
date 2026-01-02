//! Debug Traits - Platform-Agnostic Debugger Interface
//!
//! This module defines the [`Debugger`] trait that all platform-specific
//! debugger implementations must satisfy. It provides a unified API for:
//!
//! - Process attachment and detachment
//! - Execution control (continue, single-step)
//! - Breakpoint management (software breakpoints)
//! - Memory read/write operations
//! - Register inspection
//!
//! # Platform Implementations
//!
//! - Windows: Uses Win32 Debug API (`WaitForDebugEvent`, `ContinueDebugEvent`)
//! - Linux: Uses `ptrace` system call
//! - macOS: Stub implementation (Mach API not yet implemented)
//!
//! # Example
//!
//! ```ignore
//! use crate::debug::{Debugger, PlatformDebugger};
//!
//! let mut dbg = PlatformDebugger::default();
//! dbg.attach(1234)?;
//! dbg.set_sw_breakpoint(0x401000)?;
//! dbg.continue_execution()?;
//! ```

use super::types::{ProcessInfo, RegisterState};

/// Platform-agnostic debugger trait
///
/// This trait defines the common interface for all platform-specific debugger implementations.
/// Each platform (Windows, Linux, macOS) provides its own implementation.
pub trait Debugger: Send {
    /// Enumerate running processes on the system
    fn enumerate_processes() -> Vec<ProcessInfo>
    where
        Self: Sized;

    /// Attach to a process by PID
    fn attach(&mut self, pid: u32) -> Result<(), String>;

    /// Detach from the current process
    fn detach(&mut self) -> Result<(), String>;

    /// Check if currently attached to a process
    fn is_attached(&self) -> bool;

    /// Get the attached process ID (if any)
    fn attached_pid(&self) -> Option<u32>;

    /// Continue execution after a debug event
    fn continue_execution(&mut self) -> Result<(), String>;

    /// Single step one instruction
    fn single_step(&mut self) -> Result<(), String>;

    /// Set a software breakpoint at the given address
    fn set_sw_breakpoint(&mut self, address: u64) -> Result<(), String>;

    /// Remove a software breakpoint at the given address
    fn remove_sw_breakpoint(&mut self, address: u64) -> Result<(), String>;

    /// Read memory from the target process
    fn read_memory(&self, address: u64, size: usize) -> Result<Vec<u8>, String>;

    /// Write memory to the target process
    fn write_memory(&mut self, address: u64, data: &[u8]) -> Result<(), String>;

    /// Fetch CPU registers for a thread
    fn fetch_registers(&mut self, thread_id: u32) -> Result<RegisterState, String>;
}
