//! Debug module - Dynamic analysis and debugging functionality.
//!
//! Provides cross-platform debugging capabilities:
//! - Process attach/detach
//! - Breakpoint management
//! - Register/memory access
//! - Step execution

pub mod types;

#[cfg(target_os = "windows")]
pub mod windows;

// Legacy modules (to be refactored)
pub mod debugger;
pub mod memory;

#[cfg(target_os = "windows")]
pub use windows::WindowsDebugger as PlatformDebugger;

#[cfg(target_os = "windows")]
pub use windows::enumerate_processes;

use types::ProcessInfo;

/// Platform-agnostic debugger trait
pub trait Debugger {
    /// Enumerate running processes
    fn enumerate_processes() -> Vec<ProcessInfo>;
    
    /// Attach to a process by PID
    fn attach(&mut self, pid: u32) -> Result<(), String>;
    
    /// Detach from the current process
    fn detach(&mut self) -> Result<(), String>;
    
    /// Check if currently attached
    fn is_attached(&self) -> bool;
    
    /// Get the attached process ID
    fn attached_pid(&self) -> Option<u32>;

    /// Continue execution after a debug event
    fn continue_execution(&mut self) -> Result<(), String>;

    /// Single step (best-effort)
    fn single_step(&mut self) -> Result<(), String>;

    /// Set a software breakpoint
    fn set_sw_breakpoint(&mut self, address: u64) -> Result<(), String>;

    /// Remove a software breakpoint
    fn remove_sw_breakpoint(&mut self, address: u64) -> Result<(), String>;
}
