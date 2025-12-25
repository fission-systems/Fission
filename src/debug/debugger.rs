//! Debugger - OS Debug API Wrapper
//!
//! Provides a unified interface for debugging across Windows, Linux, and macOS platforms.
//! - Windows: uses the Debug API (windows-rs)
//! - Linux: uses ptrace (nix)
//! - macOS: stub implementation (requires Mach API)

use thiserror::Error;
use super::memory::MemoryManager;

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
    
    #[error("Memory error: {0}")]
    MemoryError(String),
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
    
    /// No event (timeout or spurious wakeup)
    None,
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
    
    /// Memory manager for reading/writing process memory
    memory: MemoryManager,
    
    /// Last thread ID that triggered an event (needed for continue)
    #[cfg(target_os = "windows")]
    last_thread_id: Option<u32>,
}

impl Debugger {
    /// Create a new debugger instance
    pub fn new() -> Self {
        Self {
            target_pid: None,
            is_active: false,
            breakpoints: Vec::new(),
            memory: MemoryManager::new(),
            #[cfg(target_os = "windows")]
            last_thread_id: None,
        }
    }

    /// Attach to an existing process by PID
    pub fn attach(&mut self, pid: u32) -> Result<(), DebugError> {
        log::info!("Attaching to process {}", pid);

        #[cfg(target_os = "windows")]
        {
            self.attach_windows(pid)?;
            self.memory.open_process(pid).map_err(|e| DebugError::MemoryError(e.to_string()))?;
        }

        #[cfg(target_os = "linux")]
        {
            self.attach_linux(pid)?;
            self.memory.open_process(pid).map_err(|e| DebugError::MemoryError(e.to_string()))?;
        }
        
        #[cfg(target_os = "macos")]
        {
            // macOS debugging not yet implemented
            return Err(DebugError::AttachFailed {
                pid,
                reason: "macOS debugging requires Mach API (not yet implemented)".into(),
            });
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            self.target_pid = Some(pid);
            self.is_active = true;
            log::info!("Successfully attached to process {}", pid);
        }

        Ok(())
    }

    /// Detach from the current process
    pub fn detach(&mut self) -> Result<(), DebugError> {
        let pid = self
            .target_pid
            .ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        log::info!("Detaching from process {}", pid);
        
        // Restore all breakpoints before detaching
        for bp in &self.breakpoints.clone() {
            if let Breakpoint::Software { address, original_byte } = bp {
                let _ = self.memory.write(*address, &[*original_byte]);
            }
        }

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
        self.breakpoints.clear();

        log::info!("Successfully detached from process {}", pid);
        Ok(())
    }

    /// Wait for the next debug event
    pub fn wait_for_event(&mut self) -> Result<DebugEvent, DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        #[cfg(target_os = "windows")]
        {
            return self.wait_for_event_windows();
        }

        #[cfg(target_os = "linux")]
        {
            return self.wait_for_event_linux();
        }
        
        #[cfg(target_os = "macos")]
        {
            Err(DebugError::EventError("macOS debugging not implemented".into()))
        }
    }

    /// Set a software breakpoint at the specified address
    pub fn set_breakpoint(&mut self, address: u64) -> Result<(), DebugError> {
        log::debug!("Setting breakpoint at {:#x}", address);
        
        // Check if breakpoint already exists
        if self.breakpoints.iter().any(|bp| match bp {
            Breakpoint::Software { address: a, .. } => *a == address,
            Breakpoint::Hardware { address: a, .. } => *a == address,
        }) {
            return Ok(()); // Already set
        }

        // Read the original byte
        let original_byte = self.memory.read_u8(address)
            .map_err(|e| DebugError::BreakpointError {
                address,
                reason: format!("Failed to read original byte: {}", e),
            })?;
        
        // Write INT3 (0xCC)
        self.memory.write(address, &[0xCC])
            .map_err(|e| DebugError::BreakpointError {
                address,
                reason: format!("Failed to write INT3: {}", e),
            })?;

        let bp = Breakpoint::Software {
            address,
            original_byte,
        };

        self.breakpoints.push(bp);
        log::debug!("Breakpoint set at {:#x}, original byte: {:#x}", address, original_byte);
        Ok(())
    }

    /// Remove a breakpoint at the specified address
    pub fn remove_breakpoint(&mut self, address: u64) -> Result<(), DebugError> {
        log::debug!("Removing breakpoint at {:#x}", address);

        // Find and remove the breakpoint, restoring original byte
        let mut found = None;
        self.breakpoints.retain(|bp| {
            match bp {
                Breakpoint::Software { address: addr, original_byte } if *addr == address => {
                    found = Some(*original_byte);
                    false
                }
                Breakpoint::Hardware { address: addr, .. } if *addr == address => false,
                _ => true,
            }
        });
        
        // Restore original byte if it was a software breakpoint
        if let Some(original) = found {
            self.memory.write(address, &[original])
                .map_err(|e| DebugError::BreakpointError {
                    address,
                    reason: format!("Failed to restore original byte: {}", e),
                })?;
        }

        Ok(())
    }

    /// Continue execution
    pub fn continue_execution(&mut self) -> Result<(), DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        #[cfg(target_os = "windows")]
        {
            return self.continue_windows();
        }

        #[cfg(target_os = "linux")]
        {
            return self.continue_linux();
        }
        
        #[cfg(target_os = "macos")]
        {
            Err(DebugError::EventError("macOS debugging not implemented".into()))
        }
    }

    /// Step a single instruction
    pub fn single_step(&mut self) -> Result<(), DebugError> {
        if !self.is_active {
            return Err(DebugError::EventError("Debugger not active".into()));
        }

        #[cfg(target_os = "windows")]
        {
            return self.single_step_windows();
        }

        #[cfg(target_os = "linux")]
        {
            return self.single_step_linux();
        }
        
        #[cfg(target_os = "macos")]
        {
            Err(DebugError::EventError("macOS debugging not implemented".into()))
        }
    }

    /// Get current target PID
    pub fn target_pid(&self) -> Option<u32> {
        self.target_pid
    }

    /// Check if debugger is active
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    
    /// Get active breakpoints
    pub fn breakpoints(&self) -> &[Breakpoint] {
        &self.breakpoints
    }
}

// ============================================================================
// Windows-specific implementations
// ============================================================================
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
    
    fn wait_for_event_windows(&mut self) -> Result<DebugEvent, DebugError> {
        use windows::Win32::System::Diagnostics::Debug::{
            WaitForDebugEvent, DEBUG_EVENT,
            EXCEPTION_DEBUG_EVENT, CREATE_THREAD_DEBUG_EVENT, 
            CREATE_PROCESS_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT,
            LOAD_DLL_DEBUG_EVENT, OUTPUT_DEBUG_STRING_EVENT,
        };
        use windows::Win32::Foundation::WAIT_TIMEOUT;
        
        let mut event: DEBUG_EVENT = unsafe { std::mem::zeroed() };
        
        // Wait for 100ms to avoid blocking forever
        let result = unsafe { WaitForDebugEvent(&mut event, 100) };
        
        if result.is_err() {
            // Timeout or error
            return Ok(DebugEvent::None);
        }
        
        // Store thread ID for continue
        self.last_thread_id = Some(event.dwThreadId);
        
        let debug_event = match event.dwDebugEventCode {
            CREATE_PROCESS_DEBUG_EVENT => {
                let info = unsafe { event.u.CreateProcessInfo };
                DebugEvent::ProcessCreated {
                    pid: event.dwProcessId,
                    base_address: unsafe { info.lpBaseOfImage } as u64,
                }
            }
            CREATE_THREAD_DEBUG_EVENT => {
                DebugEvent::ThreadCreated { tid: event.dwThreadId }
            }
            EXCEPTION_DEBUG_EVENT => {
                let info = unsafe { event.u.Exception };
                let code = info.ExceptionRecord.ExceptionCode.0 as u32;
                let address = info.ExceptionRecord.ExceptionAddress as u64;
                
                // Check if it's a breakpoint (0x80000003)
                if code == 0x80000003 {
                    DebugEvent::BreakpointHit { 
                        address, 
                        tid: event.dwThreadId 
                    }
                } else if code == 0x80000004 {
                    // Single step
                    DebugEvent::SingleStep { tid: event.dwThreadId }
                } else {
                    DebugEvent::Exception { code, address }
                }
            }
            EXIT_PROCESS_DEBUG_EVENT => {
                let info = unsafe { event.u.ExitProcess };
                DebugEvent::ProcessExited { exit_code: info.dwExitCode }
            }
            LOAD_DLL_DEBUG_EVENT => {
                let info = unsafe { event.u.LoadDll };
                DebugEvent::ModuleLoaded {
                    name: String::new(), // Would need to read from memory
                    base_address: info.lpBaseOfDll as u64,
                }
            }
            _ => DebugEvent::None,
        };
        
        Ok(debug_event)
    }
    
    fn continue_windows(&mut self) -> Result<(), DebugError> {
        use windows::Win32::System::Diagnostics::Debug::{ContinueDebugEvent, DBG_CONTINUE};
        
        let pid = self.target_pid.ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        let tid = self.last_thread_id.unwrap_or(0);
        
        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| DebugError::EventError(format!("ContinueDebugEvent failed: {}", e)))?;
        }
        
        Ok(())
    }
    
    fn single_step_windows(&mut self) -> Result<(), DebugError> {
        use windows::Win32::System::Diagnostics::Debug::{ContinueDebugEvent, DBG_CONTINUE};
        use windows::Win32::System::Threading::{OpenThread, GetThreadContext, SetThreadContext, THREAD_ALL_ACCESS};
        use windows::Win32::System::Diagnostics::Debug::CONTEXT;
        use windows::Win32::Foundation::HANDLE;
        
        let tid = self.last_thread_id.ok_or(DebugError::EventError("No thread to step".into()))?;
        
        unsafe {
            // Open the thread
            let thread_handle = OpenThread(THREAD_ALL_ACCESS, false, tid)
                .map_err(|e| DebugError::EventError(format!("OpenThread failed: {}", e)))?;
            
            // Get thread context
            let mut context: CONTEXT = std::mem::zeroed();
            context.ContextFlags = 0x10001; // CONTEXT_CONTROL
            
            GetThreadContext(thread_handle, &mut context)
                .map_err(|e| DebugError::EventError(format!("GetThreadContext failed: {}", e)))?;
            
            // Set trap flag (TF) in EFLAGS
            context.EFlags |= 0x100;
            
            SetThreadContext(thread_handle, &context)
                .map_err(|e| DebugError::EventError(format!("SetThreadContext failed: {}", e)))?;
        }
        
        // Continue execution (will stop after one instruction)
        self.continue_windows()
    }
}

// ============================================================================
// Linux-specific implementations
// ============================================================================
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
    
    fn wait_for_event_linux(&mut self) -> Result<DebugEvent, DebugError> {
        use nix::sys::wait::{waitpid, WaitStatus, WaitPidFlag};
        use nix::unistd::Pid;
        use nix::sys::signal::Signal;
        
        let pid = self.target_pid.ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        
        match waitpid(Pid::from_raw(pid as i32), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {
                // Read instruction pointer to get breakpoint address
                Ok(DebugEvent::BreakpointHit { 
                    address: 0, // Would need to read RIP/EIP
                    tid: pid 
                })
            }
            Ok(WaitStatus::Stopped(_, Signal::SIGSTOP)) => {
                Ok(DebugEvent::ProcessCreated { pid, base_address: 0 })
            }
            Ok(WaitStatus::Exited(_, code)) => {
                Ok(DebugEvent::ProcessExited { exit_code: code as u32 })
            }
            Ok(WaitStatus::Signaled(_, sig, _)) => {
                Ok(DebugEvent::Exception { code: sig as u32, address: 0 })
            }
            Ok(WaitStatus::StillAlive) => {
                Ok(DebugEvent::None)
            }
            Ok(_) => Ok(DebugEvent::None),
            Err(e) => Err(DebugError::EventError(format!("waitpid failed: {}", e))),
        }
    }
    
    fn continue_linux(&mut self) -> Result<(), DebugError> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;
        
        let pid = self.target_pid.ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        
        ptrace::cont(Pid::from_raw(pid as i32), None)
            .map_err(|e| DebugError::EventError(format!("ptrace cont failed: {}", e)))?;
        
        Ok(())
    }
    
    fn single_step_linux(&mut self) -> Result<(), DebugError> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;
        
        let pid = self.target_pid.ok_or(DebugError::ProcessNotFound { pid: 0 })?;
        
        ptrace::step(Pid::from_raw(pid as i32), None)
            .map_err(|e| DebugError::EventError(format!("ptrace step failed: {}", e)))?;
        
        Ok(())
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}
