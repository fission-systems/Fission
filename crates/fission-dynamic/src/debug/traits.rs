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
use fission_core::Result as FissionResult;

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
    fn attach(&mut self, pid: u32) -> FissionResult<()>;

    /// Detach from the current process
    fn detach(&mut self) -> FissionResult<()>;

    /// Check if currently attached to a process
    fn is_attached(&self) -> bool;

    /// Get the attached process ID (if any)
    fn attached_pid(&self) -> Option<u32>;

    /// Continue execution after a debug event
    fn continue_execution(&mut self) -> FissionResult<()>;

    /// Single step one instruction
    fn single_step(&mut self) -> FissionResult<()>;

    /// Set a software breakpoint at the given address
    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()>;

    /// Remove a software breakpoint at the given address
    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()>;

    /// Read memory from the target process
    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>>;

    /// Write memory to the target process
    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()>;

    /// Fetch CPU registers for a thread
    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<RegisterState>;

    /// Write CPU registers for a thread
    fn set_registers(&mut self, thread_id: u32, regs: &RegisterState) -> FissionResult<()> {
        let _ = (thread_id, regs);
        Err(fission_core::err!(
            debug,
            "set_registers is not supported on this platform"
        ))
    }

    /// Launch a new process under the debugger
    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        let _ = (path, args);
        Err(fission_core::err!(
            debug,
            "launch is not supported on this platform"
        ))
    }

    /// Single step over a CALL (step over)
    fn step_over(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "step_over is not supported on this platform"
        ))
    }

    /// Step out of the current function (run to return)
    fn step_out(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "step_out is not supported on this platform"
        ))
    }

    /// Pause the target process
    fn pause(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "pause is not supported on this platform"
        ))
    }

    /// Terminate the target process
    fn terminate(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "terminate is not supported on this platform"
        ))
    }

    /// Switch the active thread for subsequent operations
    fn set_current_thread(&mut self, thread_id: u32) -> FissionResult<()> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "set_current_thread is not supported on this platform"
        ))
    }

    /// Suspend a thread by ID
    fn suspend_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "suspend_thread is not supported on this platform"
        ))
    }

    /// Resume a thread by ID
    fn resume_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "resume_thread is not supported on this platform"
        ))
    }

    /// Skip the current instruction by advancing the instruction pointer.
    fn skip_instruction(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "skip_instruction is not supported on this platform"
        ))
    }

    /// Set a memory breakpoint on a range
    fn set_memory_breakpoint(
        &mut self,
        address: u64,
        size: usize,
        kind: super::types::MemoryBpKind,
    ) -> FissionResult<()> {
        let _ = (address, size, kind);
        Err(fission_core::err!(
            debug,
            "set_memory_breakpoint is not supported on this platform"
        ))
    }

    /// Remove a memory breakpoint
    fn remove_memory_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "remove_memory_breakpoint is not supported on this platform"
        ))
    }

    /// Set a DLL load breakpoint
    fn set_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        let _ = dll_name;
        Err(fission_core::err!(
            debug,
            "set_dll_breakpoint is not supported on this platform"
        ))
    }

    /// Remove a DLL load breakpoint
    fn remove_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        let _ = dll_name;
        Err(fission_core::err!(
            debug,
            "remove_dll_breakpoint is not supported on this platform"
        ))
    }

    /// Set an exception breakpoint
    fn set_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let _ = code;
        Err(fission_core::err!(
            debug,
            "set_exception_breakpoint is not supported on this platform"
        ))
    }

    /// Remove an exception breakpoint
    fn remove_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let _ = code;
        Err(fission_core::err!(
            debug,
            "remove_exception_breakpoint is not supported on this platform"
        ))
    }

    /// Enable a breakpoint by address
    fn enable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "enable_breakpoint is not supported on this platform"
        ))
    }

    /// Disable a breakpoint by address
    fn disable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "disable_breakpoint is not supported on this platform"
        ))
    }

    /// List all active breakpoints
    fn list_breakpoints(&self) -> Vec<super::types::Breakpoint> {
        Vec::new()
    }

    /// Allocate memory in the target process
    fn remote_alloc(&mut self, address: u64, size: usize) -> FissionResult<u64> {
        let _ = (address, size);
        Err(fission_core::err!(
            debug,
            "remote_alloc is not supported on this platform"
        ))
    }

    /// Free memory in the target process
    fn remote_free(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "remote_free is not supported on this platform"
        ))
    }

    /// Get page protection rights at an address
    fn get_page_rights(&self, address: u64) -> FissionResult<u32> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "get_page_rights is not supported on this platform"
        ))
    }

    /// Set page protection rights for a region
    fn set_page_rights(&mut self, address: u64, size: usize, protect: u32) -> FissionResult<()> {
        let _ = (address, size, protect);
        Err(fission_core::err!(
            debug,
            "set_page_rights is not supported on this platform"
        ))
    }

    /// Peek a value from the stack at an offset from RSP
    fn stack_peek(&self, offset: isize) -> FissionResult<u64> {
        let _ = offset;
        Err(fission_core::err!(
            debug,
            "stack_peek is not supported on this platform"
        ))
    }

    /// Pop a value from the stack (adjusts RSP and returns the value)
    fn stack_pop(&mut self) -> FissionResult<u64> {
        Err(fission_core::err!(
            debug,
            "stack_pop is not supported on this platform"
        ))
    }

    /// Push a value onto the stack (adjusts RSP)
    fn stack_push(&mut self, value: u64) -> FissionResult<()> {
        let _ = value;
        Err(fission_core::err!(
            debug,
            "stack_push is not supported on this platform"
        ))
    }

    /// Search for a byte pattern in target memory
    fn find_pattern(&self, start: u64, size: usize, pattern: &[u8]) -> FissionResult<Vec<u64>> {
        let _ = (start, size, pattern);
        Err(fission_core::err!(
            debug,
            "find_pattern is not supported on this platform"
        ))
    }

    /// Enumerate exports from a module by base address
    fn get_module_exports(&self, base: u64) -> FissionResult<Vec<super::types::ExportInfo>> {
        let _ = base;
        Err(fission_core::err!(
            debug,
            "get_module_exports is not supported on this platform"
        ))
    }

    /// Enumerate imports from a module by base address
    fn get_module_imports(&self, base: u64) -> FissionResult<Vec<super::types::ImportInfo>> {
        let _ = base;
        Err(fission_core::err!(
            debug,
            "get_module_imports is not supported on this platform"
        ))
    }

    /// Set a single register by name (e.g. "rax", "rip")
    fn set_register(&mut self, thread_id: u32, name: &str, value: u64) -> FissionResult<()> {
        let _ = (thread_id, name, value);
        Err(fission_core::err!(
            debug,
            "set_register is not supported on this platform"
        ))
    }

    /// Get a CPU flag by name (e.g. "zf", "cf")
    fn get_flag(&self, flag: &str) -> FissionResult<bool> {
        let _ = flag;
        Err(fission_core::err!(
            debug,
            "get_flag is not supported on this platform"
        ))
    }

    /// Set a CPU flag by name (e.g. "zf", "cf")
    fn set_flag(&mut self, flag: &str, value: bool) -> FissionResult<()> {
        let _ = (flag, value);
        Err(fission_core::err!(
            debug,
            "set_flag is not supported on this platform"
        ))
    }
}

// ============================================================================
// Time Travel Debugging Trait
// ============================================================================

use super::timeline::ExecutionSnapshot;

/// Time-travel / reversible execution backend trait
///
/// Backends include:
/// - **Internal recorder** (`fission-ttd`): snapshot-based timeline (used heavily on Windows flows)
/// - **RR (Record and Replay)**: Linux-only GDB/MI integration (`crate::debug::rr`)
///
/// # Example
///
/// ```ignore
/// use crate::debug::TimeTravelDebugger;
///
/// // Record execution
/// debugger.start_recording()?;
/// // ... run program ...
/// debugger.stop_recording()?;
///
/// // Navigate timeline
/// debugger.seek_to(100)?;          // Go to step 100
/// debugger.reverse_step()?;        // Step backwards
/// debugger.reverse_continue()?;    // Run backwards to breakpoint
/// ```
pub trait TimeTravelDebugger: Send {
    /// Start recording execution
    fn start_recording(&mut self) -> FissionResult<()>;

    /// Stop recording execution
    fn stop_recording(&mut self) -> FissionResult<()>;

    /// Check if currently recording
    fn is_recording(&self) -> bool;

    /// Check if in replay/navigation mode
    fn is_replay_mode(&self) -> bool;

    /// Seek to a specific step/position in the timeline
    fn seek_to(&mut self, position: u64) -> FissionResult<ExecutionSnapshot>;

    /// Step backwards one instruction
    fn reverse_step(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Continue backwards until next breakpoint
    fn reverse_continue(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Step forwards one instruction (in replay mode)
    fn forward_step(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Continue forwards until next breakpoint (in replay mode)
    fn forward_continue(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Get current position in timeline
    fn current_position(&self) -> Option<u64>;

    /// Get current execution snapshot
    fn current_snapshot(&self) -> Option<&ExecutionSnapshot>;

    /// Get timeline range (min_step, max_step)
    fn timeline_range(&self) -> Option<(u64, u64)>;

    /// Get total number of recorded steps
    fn step_count(&self) -> usize;

    /// Clear all recorded data
    fn clear_timeline(&mut self);
}
