//! Common types for debugging functionality.

use std::collections::HashMap;

/// Information about a running process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
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
    /// Breakpoint hit
    BreakpointHit { address: u64, thread_id: u32 },
    /// Single step completed
    SingleStep { thread_id: u32 },
    /// Exception occurred
    Exception { code: u32, address: u64, first_chance: bool },
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

/// CPU register state (x64)
#[derive(Debug, Clone, Default)]
pub struct RegisterState {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
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
