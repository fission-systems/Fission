//! Windows-specific debugger implementation using Win32 Debug API.

mod process;

pub use process::enumerate_processes;

use super::types::{DebugState, DebugStatus, ProcessInfo};
use super::Debugger;

use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use windows::Win32::System::Diagnostics::Debug::{
    DebugActiveProcess, DebugActiveProcessStop, WaitForDebugEvent, ContinueDebugEvent,
    DEBUG_EVENT, EXCEPTION_DEBUG_EVENT, CREATE_THREAD_DEBUG_EVENT,
    EXIT_THREAD_DEBUG_EVENT, CREATE_PROCESS_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT,
    LOAD_DLL_DEBUG_EVENT,
};
use windows::Win32::Foundation::NTSTATUS;

const DBG_CONTINUE: NTSTATUS = NTSTATUS(0x00010002i32);
const EXCEPTION_BREAKPOINT_CODE: u32 = 0x80000003;
const EXCEPTION_SINGLE_STEP_CODE: u32 = 0x80000004;

/// Windows debugger implementation
pub struct WindowsDebugger {
    /// Current debug state
    state: DebugState,
}

impl WindowsDebugger {
    /// Create a new Windows debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
        }
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }
}

/// Start debug event loop for the attached process
pub fn start_event_loop(
    pid: u32,
    tx: Sender<super::types::DebugEvent>,
    stop_rx: Receiver<()>,
) {
    thread::spawn(move || {
        let mut debug_event = DEBUG_EVENT::default();
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            let wait_ok = unsafe { WaitForDebugEvent(&mut debug_event, 100) };
            if wait_ok.is_ok() {
                let code = debug_event.dwDebugEventCode;
                let proc_id = debug_event.dwProcessId;
                let thread_id = debug_event.dwThreadId;

                let evt_opt = match code {
                    EXCEPTION_DEBUG_EVENT => unsafe {
                        let info = debug_event.u.Exception;
                        let record = info.ExceptionRecord;
                        let is_first = info.dwFirstChance != 0;
                        let address = record.ExceptionAddress as u64;
                        let code_raw: u32 = record.ExceptionCode.0 as u32;
                        if code_raw == EXCEPTION_BREAKPOINT_CODE {
                            Some(super::types::DebugEvent::BreakpointHit { address, thread_id })
                        } else if code_raw == EXCEPTION_SINGLE_STEP_CODE {
                            Some(super::types::DebugEvent::SingleStep { thread_id })
                        } else {
                            Some(super::types::DebugEvent::Exception { code: code_raw, address, first_chance: is_first })
                        }
                    },
                    CREATE_PROCESS_DEBUG_EVENT => Some(super::types::DebugEvent::ProcessCreated {
                        pid: proc_id,
                        main_thread_id: thread_id,
                    }),
                    EXIT_PROCESS_DEBUG_EVENT => {
                        let exit_code = unsafe { debug_event.u.ExitProcess.dwExitCode };
                        Some(super::types::DebugEvent::ProcessExited { exit_code })
                    }
                    CREATE_THREAD_DEBUG_EVENT => Some(super::types::DebugEvent::ThreadCreated { thread_id }),
                    EXIT_THREAD_DEBUG_EVENT => {
                        let _exit_code = unsafe { debug_event.u.ExitThread.dwExitCode };
                        Some(super::types::DebugEvent::ThreadExited { thread_id })
                    }
                    LOAD_DLL_DEBUG_EVENT => Some(super::types::DebugEvent::DllLoaded {
                        base_address: unsafe { debug_event.u.LoadDll.lpBaseOfDll } as u64,
                        name: "<dll>".into(),
                    }),
                    _ => None,
                };

                if let Some(evt) = evt_opt {
                    let _ = tx.send(evt);
                }

                unsafe {
                    let _ = ContinueDebugEvent(proc_id, thread_id, DBG_CONTINUE);
                }
            } else {
                // no event, just wait a bit
                thread::sleep(Duration::from_millis(10));
            }
        }
    });
}

impl Default for WindowsDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl Debugger for WindowsDebugger {
    fn enumerate_processes() -> Vec<ProcessInfo> {
        process::enumerate_processes()
    }

    fn attach(&mut self, pid: u32) -> Result<(), String> {
        self.state.status = DebugStatus::Attaching;
        
        unsafe {
            DebugActiveProcess(pid)
                .map_err(|e| format!("Failed to attach to process {}: {:?}", pid, e))?;
        }
        
        self.state.attached_pid = Some(pid);
        self.state.status = DebugStatus::Running;
        self.state.last_event = Some(format!("Attached to PID {}", pid));
        
        Ok(())
    }

    fn detach(&mut self) -> Result<(), String> {
        let pid = self.state.attached_pid
            .ok_or_else(|| "Not attached to any process".to_string())?;
        
        unsafe {
            DebugActiveProcessStop(pid)
                .map_err(|e| format!("Failed to detach from process {}: {:?}", pid, e))?;
        }
        
        self.state.attached_pid = None;
        self.state.main_thread_id = None;
        self.state.last_thread_id = None;
        self.state.status = DebugStatus::Detached;
        self.state.last_event = Some("Detached".to_string());
        
        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.state.attached_pid.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        self.state.attached_pid
    }

    fn continue_execution(&mut self) -> Result<(), String> {
        let pid = self.state.attached_pid.ok_or("Not attached")?;
        let tid = self.state.last_thread_id.or(self.state.main_thread_id).ok_or("No thread id")?;
        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| format!("Continue failed: {:?}", e))?;
        }
        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn single_step(&mut self) -> Result<(), String> {
        // Fallback: just continue (no trap flag without extra kernel feature)
        self.continue_execution()
    }

    fn set_sw_breakpoint(&mut self, _address: u64) -> Result<(), String> {
        // Placeholder: record breakpoint in state without patching memory
        let bp = super::types::Breakpoint {
            address: _address,
            original_byte: 0,
            enabled: true,
        };
        self.state.breakpoints.insert(_address, bp);
        self.state.last_event = Some(format!("Breakpoint set 0x{:016x}", _address));
        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, _address: u64) -> Result<(), String> {
        self.state.breakpoints.remove(&_address);
        self.state.last_event = Some(format!("Breakpoint removed 0x{:016x}", _address));
        Ok(())
    }
}
