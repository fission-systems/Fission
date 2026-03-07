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
}

// ============================================================================
// Time Travel Debugging Trait
// ============================================================================

use super::ttd::ExecutionSnapshot;

/// Time-travel debugging backend trait
///
/// This trait provides a unified interface for all time-travel debugging
/// backends including:
/// - **RR (Record and Replay)**: Linux-only, uses GDB/MI protocol
/// - **TTD (Internal)**: Cross-platform, snapshot-based
/// - **Windows TTD**: Windows-only (future integration with WinDbg)
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
