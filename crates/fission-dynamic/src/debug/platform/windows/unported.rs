//! What a Windows build has instead of the Win32 debugger, until that is
//! ported.
//!
//! Not a placeholder for something unwritten -- the real backend exists, in
//! `debugger/`, and is roughly two thousand lines. It stopped compiling
//! against windows-rs 0.54 and nothing noticed, because no job built it: the
//! crate's debug layer is behind `interactive_runtime`, the CLI's is behind
//! `debugger`, and neither was a default. Turning them on is what made 269
//! errors visible.
//!
//! So this says so, once, at the point where a caller would otherwise get a
//! debugger that does nothing. Every method fails with the same sentence, and
//! the emulator backend -- which runs Windows binaries on every host and is
//! the one under test -- is unaffected and is what `--emulator` selects.

use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{DebugState, ProcessInfo, RegisterState};
use fission_core::{FissionError, Result as FissionResult};
use std::sync::mpsc::{Receiver, Sender};

const UNPORTED: &str = "the Win32 debugger backend is not in this build (it does not compile \
                        against windows-rs 0.54); use --emulator, or build with \
                        --features fission-dynamic/windows_native_debugger to work on the port";

fn unported<T>() -> FissionResult<T> {
    Err(FissionError::debug(UNPORTED))
}

/// Stands in for the Win32 `WindowsDebugger`.
pub struct WindowsDebugger {
    state: DebugState,
}

impl WindowsDebugger {
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
        }
    }

    pub fn state(&self) -> &DebugState {
        &self.state
    }
}

impl Default for WindowsDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionBackend for WindowsDebugger {
    fn enumerate_processes() -> Vec<ProcessInfo> {
        Vec::new()
    }

    fn attach(&mut self, _pid: u32) -> FissionResult<()> {
        unported()
    }

    fn detach(&mut self) -> FissionResult<()> {
        unported()
    }

    fn is_attached(&self) -> bool {
        false
    }

    fn attached_pid(&self) -> Option<u32> {
        None
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        unported()
    }

    fn single_step(&mut self) -> FissionResult<()> {
        unported()
    }

    fn set_sw_breakpoint(&mut self, _address: u64) -> FissionResult<()> {
        unported()
    }

    fn remove_sw_breakpoint(&mut self, _address: u64) -> FissionResult<()> {
        unported()
    }

    fn read_memory(&self, _address: u64, _size: usize) -> FissionResult<Vec<u8>> {
        unported()
    }

    fn write_memory(&mut self, _address: u64, _data: &[u8]) -> FissionResult<()> {
        unported()
    }

    fn fetch_registers(&mut self, _thread_id: u32) -> FissionResult<RegisterState> {
        unported()
    }

    fn get_state(&self) -> DebugState {
        self.state.clone()
    }
}

/// The Win32 debug-event loop is part of the unported backend; there is
/// nothing to listen to, so this returns rather than spawning a thread that
/// would never send anything.
pub fn start_event_loop(
    _pid: u32,
    _tx: Sender<crate::debug::types::DebugEvent>,
    _stop_rx: Receiver<()>,
) {
}

/// Process enumeration lives in the unported backend too. An empty list is
/// the truthful answer here -- a caller that wanted one gets no candidates
/// rather than a wrong one.
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    Vec::new()
}
