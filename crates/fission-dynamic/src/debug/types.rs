//! Common Types for Debugging Functionality
//!
//! This module defines the core data structures used throughout the debug system:
//!
//! - [`ProcessInfo`] - Basic information about a running process
//! - [`DebugEvent`] - Events received from the debugger (breakpoints, exceptions, etc.)
//! - [`DebugState`] - Current state of the debugging session
//! - [`RegisterState`] - CPU register values (x86-64)
//! - [`Breakpoint`] - Software breakpoint representation
//!
//! These types are platform-agnostic and used by all debugger implementations.

pub use fission_ttd::RegisterState;
use std::collections::HashMap;

/// Information about a running process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
    pub name: String,
    /// Full path to the executable (if available)
    pub exe_path: Option<String>,
}

/// Debug event received from the debugger
#[derive(Debug, Clone)]
pub enum DebugEvent {
    /// Process created/attached
    ProcessCreated { pid: u32, main_thread_id: u32 },
    /// Process exited
    ProcessExited { exit_code: u32 },
    /// Thread created
    ThreadCreated { thread_id: u32 },
    /// Thread exited
    ThreadExited { thread_id: u32 },
    /// DLL loaded
    DllLoaded { base_address: u64, name: String },
    /// Breakpoint hit
    BreakpointHit { address: u64, thread_id: u32 },
    /// Single step completed
    SingleStep { thread_id: u32 },
    /// Exception occurred
    Exception {
        code: u32,
        address: u64,
        first_chance: bool,
    },
}

/// Debug session status
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DebugStatus {
    #[default]
    Detached,
    Attaching,
    Running,
    Suspended,
    Terminated,
}

/// Software breakpoint info
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Breakpoint address
    pub address: u64,
    /// Original byte at this address
    pub original_byte: u8,
    /// Is this breakpoint enabled?
    pub enabled: bool,
}

/// Debug state for GUI
#[derive(Debug, Clone, Default)]
pub struct DebugState {
    /// Attached process ID
    pub attached_pid: Option<u32>,
    /// Main thread ID
    pub main_thread_id: Option<u32>,
    /// Last event thread ID
    pub last_thread_id: Option<u32>,
    /// Current debug status
    pub status: DebugStatus,
    /// Active breakpoints
    pub breakpoints: HashMap<u64, Breakpoint>,
    /// Current register state
    pub registers: Option<RegisterState>,
    /// Last event
    pub last_event: Option<String>,
}
