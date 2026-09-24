//! The explicit fallback for Windows builds that do not enable the opt-in
//! native debugger. The emulator remains available independently.

use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{DebugState, ProcessInfo, RegisterState};
use fission_core::{FissionError, Result as FissionResult};
use std::sync::mpsc::{Receiver, Sender};

const UNPORTED: &str = "the Win32 debugger backend is not in this build; use --emulator, or build with \
                        --features fission-dynamic/windows_native_debugger to enable live debugging";

fn unported<T>() -> FissionResult<T> {
    Err(FissionError::debug(UNPORTED))
}

/// Stands in for the feature-gated Win32 `WindowsDebugger`.
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

/// No native event loop is available when the feature is disabled.
pub fn start_event_loop(
    _pid: u32,
    _tx: Sender<crate::debug::types::DebugEvent>,
    _stop_rx: Receiver<()>,
) {
}

/// Process enumeration is unavailable when the feature is disabled.
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    Vec::new()
}
