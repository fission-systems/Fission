//! Common Types for Debugging Functionality
//!
//! This module defines the core data structures used throughout the debug system:
//!
//! - [`ProcessInfo`] - Basic information about a running process
//! - [`DebugEvent`] - Events received from the debugger (breakpoints, exceptions, etc.)
//! - [`DebugState`] - Current state of the debugging session
//! - [`RegisterState`] - CPU register values (x86-64)
//! - [`Breakpoint`] - Software breakpoint representation
//! - [`ThreadInfo`] - Per-thread tracking for multi-threaded targets
//! - [`ModuleInfo`] - Loaded module/DLL information
//!
//! These types are platform-agnostic and used by all debugger implementations.

pub use fission_ttd::RegisterState;
use std::collections::{BTreeMap, HashMap};

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

/// Information about a thread in the debugged process
#[derive(Debug, Clone)]
pub struct ThreadInfo {
    /// Thread ID
    pub thread_id: u32,
    /// Thread start address (if known)
    pub start_address: u64,
    /// Whether this thread is currently suspended by the debugger
    pub suspended: bool,
    /// Whether this is the main (initial) thread
    pub is_main: bool,
}

/// Information about a loaded module (DLL / EXE) in the target process
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Base address where the module is loaded
    pub base_address: u64,
    /// Size of the module in memory (if known)
    pub size: u64,
    /// Full path to the module file
    pub path: String,
    /// Short name (e.g., "ntdll.dll")
    pub name: String,
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
    /// DLL unloaded
    DllUnloaded { base_address: u64 },
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
    /// Debug string output from the target process
    OutputString { message: String },
}

/// Hardware breakpoint kind (x86 debug register configuration).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HwBreakpointKind {
    /// Break on instruction execution (DR7 type 00)
    Execute,
    /// Break on memory write (DR7 type 01)
    Write,
    /// Break on memory read or write (DR7 type 11)
    ReadWrite,
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

/// How to handle a first-chance exception that is not a user breakpoint
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExceptionPolicy {
    /// Pass to application (DBG_EXCEPTION_NOT_HANDLED)
    PassToApplication,
    /// Swallow and continue (DBG_CONTINUE)
    SwallowContinue,
    /// Stop and notify the user
    Break,
}

impl Default for ExceptionPolicy {
    fn default() -> Self {
        Self::PassToApplication
    }
}

/// Memory breakpoint access kind
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryBpKind {
    /// Break on read
    Read,
    /// Break on write
    Write,
    /// Break on execute
    Execute,
    /// Break on any access (read, write, or execute)
    Access,
}

/// Unified breakpoint kind
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointKind {
    /// Software breakpoint (INT3)
    Software,
    /// Hardware breakpoint (x86 debug registers)
    Hardware(HwBreakpointKind),
    /// Memory breakpoint (guard page / page protection)
    Memory { size: usize, kind: MemoryBpKind },
    /// DLL load breakpoint
    Dll { name: String },
    /// Exception breakpoint
    Exception { code: u32 },
}

impl Default for BreakpointKind {
    fn default() -> Self {
        BreakpointKind::Software
    }
}

/// Breakpoint info (software, hardware, memory, DLL, or exception)
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Breakpoint address (meaning varies by kind)
    pub address: u64,
    /// Original byte at this address (for software breakpoints)
    pub original_byte: u8,
    /// Is this breakpoint enabled?
    pub enabled: bool,
    /// Is this a temporary breakpoint (e.g., step-over helper)?
    pub temporary: bool,
    /// Breakpoint kind
    pub kind: BreakpointKind,
    /// Hit count
    pub hits: u64,
    /// Optional condition expression
    pub condition: Option<String>,
}

impl Default for Breakpoint {
    fn default() -> Self {
        Self {
            address: 0,
            original_byte: 0,
            enabled: true,
            temporary: false,
            kind: BreakpointKind::Software,
            hits: 0,
            condition: None,
        }
    }
}

/// Information about a module export
#[derive(Debug, Clone)]
pub struct ExportInfo {
    /// Export name (if available)
    pub name: String,
    /// Export address
    pub address: u64,
    /// Export ordinal
    pub ordinal: Option<u32>,
}

/// Information about a module import
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Importing module name
    pub module: String,
    /// Import name (if by name)
    pub name: Option<String>,
    /// Import ordinal (if by ordinal)
    pub ordinal: Option<u16>,
    /// IAT slot address
    pub address: u64,
}

/// Debug state for GUI and session tracking
#[derive(Debug, Clone, Default)]
pub struct DebugState {
    /// Attached process ID
    pub attached_pid: Option<u32>,
    /// Main thread ID
    pub main_thread_id: Option<u32>,
    /// Last event thread ID
    pub last_thread_id: Option<u32>,
    /// Currently selected thread for register/step operations
    pub current_thread_id: Option<u32>,
    /// Current debug status
    pub status: DebugStatus,
    /// Active breakpoints
    pub breakpoints: HashMap<u64, Breakpoint>,
    /// Active threads (ordered by thread ID)
    pub threads: BTreeMap<u32, ThreadInfo>,
    /// Loaded modules (keyed by base address)
    pub modules: BTreeMap<u64, ModuleInfo>,
    /// Current register state (for the current thread)
    pub registers: Option<RegisterState>,
    /// Last event description
    pub last_event: Option<String>,
    /// Whether the initial system breakpoint has been consumed
    pub system_breakpoint_consumed: bool,
    /// Total debug events processed
    pub event_count: u64,
}
