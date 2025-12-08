//! Debugger - OS Debug API Wrapper
//!
//! Provides a unified interface for debugging across Windows and Linux platforms.
//! Windows uses the Debug API (windows-rs), Linux uses ptrace (nix).

use anyhow::Result;
use thiserror::Error;

/// Debugger-specific errors
#[derive(Error, Debug)]
pub enum DebugError {
    #[error("Failed to attach to process {pid}: {reason}")]
    AttachFailed { pid: u32, reason: String },

    #[error("Failed to detach from process {pid}: {reason}")]
    DetachFailed { pid: u32, reason: String },

    #[error("Process not found: {pid}")]
    ProcessNotFound { pid: u32 },

    #[error("Breakpoint error at {address:#x}: {reason}")]
    BreakpointError { address: u64, reason: String },

    #[error("Debug event error: {0}")]
    EventError(String),
}

/// Debug event types received from the target process
#[derive(Debug, Clone)]
pub enum DebugEvent {
    /// Process created or attached
    ProcessCreated { pid: u32, base_address: u64 },

    /// Thread created
    ThreadCreated { tid: u32 },

    /// Breakpoint hit
    BreakpointHit { address: u64, tid: u32 },

    /// Single step completed
    SingleStep { tid: u32 },

    /// Exception occurred
    Exception { code: u32, address: u64 },

    /// Process exited
    ProcessExited { exit_code: u32 },

    /// DLL/Library loaded
    ModuleLoaded { name: String, base_address: u64 },
}

/// Breakpoint types
#[derive(Debug, Clone)]
pub enum Breakpoint {
    /// Software breakpoint (INT3)
    Software { address: u64, original_byte: u8 },

    /// Hardware breakpoint (debug registers)
    Hardware { address: u64, register: u8 },
}

/// Main debugger interface
pub struct Debugger {
    /// Target process ID
    target_pid: Option<u32>,

    /// Whether the debugger is currently active
    is_active: bool,

    /// List of active breakpoints
    breakpoints: Vec<Breakpoint>,
}

impl Debugger {
    /// Create a new debugger instance
    pub fn new() -> Self {
        Self {
            target_pid: None,
            is_active: false,
            breakpoints: Vec::new(),
        }
    }

    /// Attach to an existing process by PID
    pub fn attach(&mut self, pid: u32) -> Result<(), DebugError> {
        log::info!("Attaching to process {}", pid);

        #[cfg(target_os = "windows")]
        {
            self.attach_windows(pid)?;
        }

        #[cfg(target_os = "linux")]
        {
            self.attach_linux(pid)?;
        }

        self.target_pid = Some(pid);
        self.is_active = true;

        log::info!("Successfully attached to process {}", pid);
        Ok(())
    }

    /// Detach from the current process
    pub fn detach(&mut self) -> Result<(), DebugError> {
        let pid = self
            .target_pid
            .ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        log::info!("Detaching from process {}", pid);

        #[cfg(target_os = "windows")]
        {
            self.detach_windows(pid)?;
        }

        #[cfg(target_os = "linux")]
        {
            self.detach_linux(pid)?;
        }

        self.target_pid = None;
        self.is_active = false;

        log::info!("Successfully detached from process {}", pid);
        Ok(())
    }

    /// Wait for the next debug event
    pub fn wait_for_event(&self) -> Result<DebugEvent, DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        // TODO: Implement platform-specific event loop
        Err(DebugError::EventError("Not implemented".into()))
    }

    /// Set a software breakpoint at the specified address
    pub fn set_breakpoint(&mut self, address: u64) -> Result<(), DebugError> {
        log::debug!("Setting breakpoint at {:#x}", address);

        // TODO: Implement platform-specific breakpoint insertion
        let bp = Breakpoint::Software {
            address,
            original_byte: 0, // Would be read from memory
        };

        self.breakpoints.push(bp);
        Ok(())
    }

    /// Remove a breakpoint at the specified address
    pub fn remove_breakpoint(&mut self, address: u64) -> Result<(), DebugError> {
        log::debug!("Removing breakpoint at {:#x}", address);

        self.breakpoints.retain(|bp| match bp {
            Breakpoint::Software { address: addr, .. } => *addr != address,
            Breakpoint::Hardware { address: addr, .. } => *addr != address,
        });

        Ok(())
    }

    /// Continue execution
    pub fn continue_execution(&self) -> Result<(), DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        // TODO: Implement platform-specific continue
        Ok(())
    }

    /// Step a single instruction
    pub fn single_step(&self) -> Result<(), DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        // TODO: Implement platform-specific single step
        Ok(())
    }

    /// Get current target PID
    pub fn target_pid(&self) -> Option<u32> {
        self.target_pid
    }

    /// Check if debugger is active
    pub fn is_active(&self) -> bool {
        self.is_active
    }
}

// Windows-specific implementations
#[cfg(target_os = "windows")]
impl Debugger {
    fn attach_windows(&mut self, pid: u32) -> Result<(), DebugError> {
        use windows::Win32::System::Diagnostics::Debug::DebugActiveProcess;

        unsafe {
            DebugActiveProcess(pid).map_err(|e| DebugError::AttachFailed {
                pid,
                reason: e.to_string(),
            })?;
        }

        Ok(())
    }

    fn detach_windows(&mut self, pid: u32) -> Result<(), DebugError> {
        use windows::Win32::System::Diagnostics::Debug::DebugActiveProcessStop;

        unsafe {
            DebugActiveProcessStop(pid).map_err(|e| DebugError::DetachFailed {
                pid,
                reason: e.to_string(),
            })?;
        }

        Ok(())
    }
}

// Linux-specific implementations
#[cfg(target_os = "linux")]
impl Debugger {
    fn attach_linux(&mut self, pid: u32) -> Result<(), DebugError> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        ptrace::attach(Pid::from_raw(pid as i32)).map_err(|e| DebugError::AttachFailed {
            pid,
            reason: e.to_string(),
        })?;

        Ok(())
    }

    fn detach_linux(&mut self, pid: u32) -> Result<(), DebugError> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        ptrace::detach(Pid::from_raw(pid as i32), None).map_err(|e| DebugError::DetachFailed {
            pid,
            reason: e.to_string(),
        })?;

        Ok(())
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}
