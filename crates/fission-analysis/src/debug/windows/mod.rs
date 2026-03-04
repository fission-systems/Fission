//! Windows-specific debugger implementation using Win32 Debug API.

mod process;

pub use process::enumerate_processes;

use super::traits::Debugger;
use super::ttd::Timeline;
use super::types::{Breakpoint, DebugState, DebugStatus, ProcessInfo, RegisterState};
use fission_core::{FissionError, Result as FissionResult};

use crossbeam_channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_FLAGS, CREATE_PROCESS_DEBUG_EVENT, CREATE_THREAD_DEBUG_EVENT,
    ContinueDebugEvent, DEBUG_EVENT, DebugActiveProcess, DebugActiveProcessStop,
    EXCEPTION_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT, EXIT_THREAD_DEBUG_EVENT, GetThreadContext,
    LOAD_DLL_DEBUG_EVENT, ReadProcessMemory, SetThreadContext, WaitForDebugEvent,
    WriteProcessMemory,
};
use windows::Win32::System::Memory::{
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtectEx,
};
use windows::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_ALL_ACCESS, THREAD_ALL_ACCESS,
};

const DBG_CONTINUE: NTSTATUS = NTSTATUS(0x00010002i32);
const EXCEPTION_BREAKPOINT_CODE: u32 = 0x80000003;
const EXCEPTION_SINGLE_STEP_CODE: u32 = 0x80000004;

const CONTEXT_AMD64: u32 = 0x100000;
const CONTEXT_CONTROL: u32 = CONTEXT_AMD64 | 0x1;
const CONTEXT_INTEGER: u32 = CONTEXT_AMD64 | 0x2;
const CONTEXT_SEGMENTS: u32 = CONTEXT_AMD64 | 0x4;
const CONTEXT_FLOATING_POINT: u32 = CONTEXT_AMD64 | 0x8;
const CONTEXT_DEBUG_REGISTERS: u32 = CONTEXT_AMD64 | 0x10;
const CONTEXT_FULL: u32 = CONTEXT_CONTROL | CONTEXT_INTEGER | CONTEXT_FLOATING_POINT;
const CONTEXT_ALL: u32 = CONTEXT_CONTROL
    | CONTEXT_INTEGER
    | CONTEXT_SEGMENTS
    | CONTEXT_FLOATING_POINT
    | CONTEXT_DEBUG_REGISTERS;

/// Windows debugger implementation
pub struct WindowsDebugger {
    /// Current debug state
    state: DebugState,
    /// Handle to the attached process
    process_handle: Option<HANDLE>,
    /// TTD Timeline for auto-recording (shared with UI)
    pub ttd_timeline: Option<Arc<Mutex<Timeline>>>,
}

impl WindowsDebugger {
    /// Create a new Windows debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
            process_handle: None,
            ttd_timeline: None,
        }
    }

    /// Set the TTD timeline for auto-recording during debugging
    pub fn set_ttd_timeline(&mut self, timeline: Arc<Mutex<Timeline>>) {
        self.ttd_timeline = Some(timeline);
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }

    /// Ensure process handle is available
    fn ensure_process_handle(&mut self) -> FissionResult<HANDLE> {
        if let Some(h) = self.process_handle {
            return Ok(h);
        }
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        unsafe {
            let h = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
                .map_err(|e| FissionError::debug(format!("OpenProcess failed: {:?}", e)))?;
            self.process_handle = Some(h);
            Ok(h)
        }
    }

    /// Record a TTD snapshot if recording is active
    fn record_ttd_snapshot(&self, thread_id: u32, registers: &super::types::RegisterState) {
        if let Some(timeline_arc) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline_arc.lock() {
                if timeline.is_recording() {
                    timeline.record_step_internal(registers.clone(), thread_id);
                }
            }
        }
    }
}

/// Start debug event loop for the attached process
pub fn start_event_loop(pid: u32, tx: Sender<super::types::DebugEvent>, stop_rx: Receiver<()>) {
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
                            Some(super::types::DebugEvent::Exception {
                                code: code_raw,
                                address,
                                first_chance: is_first,
                            })
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
                    CREATE_THREAD_DEBUG_EVENT => {
                        Some(super::types::DebugEvent::ThreadCreated { thread_id })
                    }
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

// SAFETY: Windows HANDLE values are system-wide references (kernel objects) that are safe
// to pass between threads; the debugger is protected by an external Mutex in AppState.
unsafe impl Send for WindowsDebugger {}

impl Debugger for WindowsDebugger {
    fn enumerate_processes() -> Vec<ProcessInfo> {
        process::enumerate_processes()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        self.state.status = DebugStatus::Attaching;

        unsafe {
            DebugActiveProcess(pid).map_err(|e| {
                FissionError::debug(format!("Failed to attach to process {}: {:?}", pid, e))
            })?;
        }

        self.state.attached_pid = Some(pid);
        self.state.status = DebugStatus::Running;
        self.state.last_event = Some(format!("Attached to PID {}", pid));

        // Open process handle immediately
        let _ = self.ensure_process_handle();

        Ok(())
    }

    fn detach(&mut self) -> FissionResult<()> {
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;

        unsafe {
            DebugActiveProcessStop(pid).map_err(|e| {
                FissionError::debug(format!("Failed to detach from process {}: {:?}", pid, e))
            })?;
        }

        if let Some(h) = self.process_handle.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
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

    fn continue_execution(&mut self) -> FissionResult<()> {
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;

        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| FissionError::debug(format!("Continue failed: {:?}", e)))?;
        }
        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn single_step(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, tid)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);
            GetThreadContext(h_thread, &mut ctx)
                .map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            // Record TTD snapshot before step (if recording is active)
            let registers = super::types::RegisterState {
                rax: ctx.Rax,
                rbx: ctx.Rbx,
                rcx: ctx.Rcx,
                rdx: ctx.Rdx,
                rsi: ctx.Rsi,
                rdi: ctx.Rdi,
                rbp: ctx.Rbp,
                rsp: ctx.Rsp,
                r8: ctx.R8,
                r9: ctx.R9,
                r10: ctx.R10,
                r11: ctx.R11,
                r12: ctx.R12,
                r13: ctx.R13,
                r14: ctx.R14,
                r15: ctx.R15,
                rip: ctx.Rip,
                rflags: ctx.EFlags as u64,
            };
            self.record_ttd_snapshot(tid, &registers);

            ctx.EFlags |= 0x100; // Set Trap Flag

            SetThreadContext(h_thread, &ctx)
                .map_err(|e| FissionError::debug(format!("SetThreadContext failed: {:?}", e)))?;

            let _ = CloseHandle(h_thread);
        }

        // Continue to let the CPU execute one instruction and hit the trap
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;
        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| FissionError::debug(format!("Continue for step failed: {:?}", e)))?;
        }

        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        // Read original byte
        let original_byte = self.read_memory(address, 1)?[0];
        if original_byte == 0xCC {
            return Err(FissionError::debug(
                "Breakpoint already exists at this address",
            ));
        }

        // Patch with INT3 (0xCC)
        self.write_memory(address, &[0xCC])?;

        let bp = super::types::Breakpoint {
            address,
            original_byte,
            enabled: true,
        };
        self.state.breakpoints.insert(address, bp);
        self.state.last_event = Some(format!("Breakpoint set 0x{:016x}", address));
        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let bp = self
            .state
            .breakpoints
            .get(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;

        // Restore original byte
        self.write_memory(address, &[bp.original_byte])?;

        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!("Breakpoint removed 0x{:016x}", address));
        Ok(())
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let h_process = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        unsafe {
            let mut buffer = vec![0u8; size];
            let mut bytes_read = 0;

            let res = ReadProcessMemory(
                h_process,
                address as *const c_void,
                buffer.as_mut_ptr() as *mut c_void,
                size,
                Some(&mut bytes_read),
            );

            res.map_err(|e| {
                FissionError::debug(format!(
                    "ReadProcessMemory failed at 0x{:x}: {:?}",
                    address, e
                ))
            })?;

            buffer.truncate(bytes_read);
            Ok(buffer)
        }
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        let h_process = self.ensure_process_handle()?;
        unsafe {
            // Change protection to allow writing
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            VirtualProtectEx(
                h_process,
                address as *const c_void,
                data.len(),
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            )
            .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;

            let mut bytes_written = 0;
            let res = WriteProcessMemory(
                h_process,
                address as *const c_void,
                data.as_ptr() as *const c_void,
                data.len(),
                Some(&mut bytes_written),
            );

            // Restore protection
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            let _ = VirtualProtectEx(
                h_process,
                address as *const c_void,
                data.len(),
                old_protect,
                &mut _unused,
            );

            res.map_err(|e| {
                FissionError::debug(format!(
                    "WriteProcessMemory failed at 0x{:x}: {:?}",
                    address, e
                ))
            })?;

            if bytes_written != data.len() {
                return Err(FissionError::debug(format!(
                    "Incomplete write at 0x{:x}: {}/{}",
                    address,
                    bytes_written,
                    data.len()
                )));
            }

            Ok(())
        }
    }

    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<super::types::RegisterState> {
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            let res = GetThreadContext(h_thread, &mut ctx);
            let _ = CloseHandle(h_thread);

            res.map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            // Map Windows CONTEXT to our RegisterState (x64)
            Ok(super::types::RegisterState {
                rax: ctx.Rax,
                rbx: ctx.Rbx,
                rcx: ctx.Rcx,
                rdx: ctx.Rdx,
                rsi: ctx.Rsi,
                rdi: ctx.Rdi,
                rbp: ctx.Rbp,
                rsp: ctx.Rsp,
                r8: ctx.R8,
                r9: ctx.R9,
                r10: ctx.R10,
                r11: ctx.R11,
                r12: ctx.R12,
                r13: ctx.R13,
                r14: ctx.R14,
                r15: ctx.R15,
                rip: ctx.Rip,
                rflags: ctx.EFlags as u64,
            })
        }
    }
}
