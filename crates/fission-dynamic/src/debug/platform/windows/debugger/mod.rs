//! Windows-specific debugger implementation using Win32 Debug API.

mod process;

pub use process::enumerate_processes;

use crate::debug::timeline::Timeline;
use crate::debug::traits::Debugger;
use crate::debug::types::{Breakpoint, DebugState, DebugStatus, ProcessInfo, RegisterState};
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
    LOAD_DLL_DEBUG_EVENT, ReadProcessMemory, SetThreadContext, UNLOAD_DLL_DEBUG_EVENT,
    WaitForDebugEvent, WriteProcessMemory,
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
    /// Execution timeline for auto-recording during debugging (shared with UI)
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

    /// Attach the UI-backed [`Timeline`] for step recording during debugging
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
    fn record_ttd_snapshot(&self, thread_id: u32, registers: &crate::debug::types::RegisterState) {
        if let Some(timeline_arc) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline_arc.lock() {
                if timeline.is_recording() {
                    timeline.record_step_internal(registers.clone(), thread_id);
                }
            }
        }
    }

    // ========================================================================
    // Integrated Debug Event Processing
    // ========================================================================

    /// Process a raw Win32 `DEBUG_EVENT` and update internal `DebugState`.
    ///
    /// Returns the translated [`DebugEvent`] (if any) and the `NTSTATUS`
    /// that should be passed to `ContinueDebugEvent`.
    pub fn process_debug_event(
        &mut self,
        debug_event: &DEBUG_EVENT,
    ) -> (Option<crate::debug::types::DebugEvent>, NTSTATUS) {
        let code = debug_event.dwDebugEventCode;
        let _proc_id = debug_event.dwProcessId;
        let thread_id = debug_event.dwThreadId;

        self.state.last_thread_id = Some(thread_id);
        self.state.event_count += 1;

        let (evt, status) = match code {
            CREATE_PROCESS_DEBUG_EVENT => {
                let info = unsafe { debug_event.u.CreateProcessInfo };
                self.state.main_thread_id = Some(thread_id);
                self.state.current_thread_id = Some(thread_id);
                // Register main thread
                self.state.threads.insert(
                    thread_id,
                    crate::debug::types::ThreadInfo {
                        thread_id,
                        start_address: info
                            .lpStartAddress
                            .map(|p| p as usize as u64)
                            .unwrap_or(0),
                        suspended: false,
                        is_main: true,
                    },
                );
                // Register main module
                let base = info.lpBaseOfImage as u64;
                let module_name = self.read_image_name_safe(
                    info.lpImageName as u64,
                    info.fUnicode.0 != 0,
                );
                let short = module_short_name(&module_name);
                self.state.modules.insert(
                    base,
                    crate::debug::types::ModuleInfo {
                        base_address: base,
                        size: 0,
                        path: module_name.clone(),
                        name: short,
                    },
                );
                (
                    Some(crate::debug::types::DebugEvent::ProcessCreated {
                        pid: _proc_id,
                        main_thread_id: thread_id,
                    }),
                    DBG_CONTINUE,
                )
            }
            EXIT_PROCESS_DEBUG_EVENT => {
                let exit_code = unsafe { debug_event.u.ExitProcess.dwExitCode };
                self.state.status = DebugStatus::Terminated;
                (
                    Some(crate::debug::types::DebugEvent::ProcessExited { exit_code }),
                    DBG_CONTINUE,
                )
            }
            CREATE_THREAD_DEBUG_EVENT => {
                let info = unsafe { debug_event.u.CreateThread };
                self.state.threads.insert(
                    thread_id,
                    crate::debug::types::ThreadInfo {
                        thread_id,
                        start_address: info
                            .lpStartAddress
                            .map(|p| p as usize as u64)
                            .unwrap_or(0),
                        suspended: false,
                        is_main: false,
                    },
                );
                (
                    Some(crate::debug::types::DebugEvent::ThreadCreated { thread_id }),
                    DBG_CONTINUE,
                )
            }
            EXIT_THREAD_DEBUG_EVENT => {
                self.state.threads.remove(&thread_id);
                // If the current thread exited, fall back to main
                if self.state.current_thread_id == Some(thread_id) {
                    self.state.current_thread_id = self.state.main_thread_id;
                }
                (
                    Some(crate::debug::types::DebugEvent::ThreadExited { thread_id }),
                    DBG_CONTINUE,
                )
            }
            LOAD_DLL_DEBUG_EVENT => {
                let info = unsafe { debug_event.u.LoadDll };
                let base = info.lpBaseOfDll as u64;
                let name = self.read_image_name_safe(
                    info.lpImageName as u64,
                    info.fUnicode.0 != 0,
                );
                let short = module_short_name(&name);
                self.state.modules.insert(
                    base,
                    crate::debug::types::ModuleInfo {
                        base_address: base,
                        size: 0,
                        path: name.clone(),
                        name: short.clone(),
                    },
                );
                // Close the DLL file handle if provided
                if !info.hFile.is_invalid() {
                    unsafe {
                        let _ = CloseHandle(info.hFile);
                    }
                }
                (
                    Some(crate::debug::types::DebugEvent::DllLoaded {
                        base_address: base,
                        name: short,
                    }),
                    DBG_CONTINUE,
                )
            }
            UNLOAD_DLL_DEBUG_EVENT => {
                let base = unsafe { debug_event.u.UnloadDll.lpBaseOfDll } as u64;
                self.state.modules.remove(&base);
                (
                    Some(crate::debug::types::DebugEvent::DllUnloaded { base_address: base }),
                    DBG_CONTINUE,
                )
            }
            EXCEPTION_DEBUG_EVENT => unsafe {
                let info = debug_event.u.Exception;
                let record = info.ExceptionRecord;
                let is_first = info.dwFirstChance != 0;
                let address = record.ExceptionAddress as u64;
                let code_raw: u32 = record.ExceptionCode.0 as u32;

                if code_raw == EXCEPTION_BREAKPOINT_CODE {
                    // System breakpoint (first ntdll break): consume silently
                    if !self.state.system_breakpoint_consumed {
                        self.state.system_breakpoint_consumed = true;
                        self.state.status = DebugStatus::Suspended;
                        self.state.last_event =
                            Some("System breakpoint (initial attach)".to_string());
                        (
                            Some(crate::debug::types::DebugEvent::BreakpointHit {
                                address,
                                thread_id,
                            }),
                            DBG_CONTINUE,
                        )
                    } else {
                        // User breakpoint or step-over temp BP
                        self.state.status = DebugStatus::Suspended;
                        // Clean up temporary breakpoints
                        if let Some(bp) = self.state.breakpoints.get(&address) {
                            if bp.temporary {
                                let orig = bp.original_byte;
                                let _ = self.write_memory(address, &[orig]);
                                self.state.breakpoints.remove(&address);
                            }
                        }
                        self.state.last_event =
                            Some(format!("Breakpoint hit at 0x{:016x}", address));
                        (
                            Some(crate::debug::types::DebugEvent::BreakpointHit {
                                address,
                                thread_id,
                            }),
                            DBG_CONTINUE,
                        )
                    }
                } else if code_raw == EXCEPTION_SINGLE_STEP_CODE {
                    self.state.status = DebugStatus::Suspended;
                    self.state.last_event =
                        Some(format!("Single step at thread {}", thread_id));
                    (
                        Some(crate::debug::types::DebugEvent::SingleStep { thread_id }),
                        DBG_CONTINUE,
                    )
                } else {
                    // Other exceptions: first-chance → pass to app; second-chance → break
                    let status = if is_first {
                        NTSTATUS(0x80010001u32 as i32) // DBG_EXCEPTION_NOT_HANDLED
                    } else {
                        self.state.status = DebugStatus::Suspended;
                        DBG_CONTINUE
                    };
                    self.state.last_event = Some(format!(
                        "Exception 0x{:08x} at 0x{:016x} ({})",
                        code_raw,
                        address,
                        if is_first { "first" } else { "second" }
                    ));
                    (
                        Some(crate::debug::types::DebugEvent::Exception {
                            code: code_raw,
                            address,
                            first_chance: is_first,
                        }),
                        status,
                    )
                }
            },
            _ => (None, DBG_CONTINUE),
        };

        (evt, status)
    }

    /// Step over a single instruction.
    ///
    /// If the current instruction is a `CALL` (opcode `0xE8` or `0xFF /2`),
    /// sets a temporary breakpoint at the next instruction and continues.
    /// Otherwise behaves like `single_step`.
    pub fn step_over(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for step over"))?;

        // Read current RIP
        let regs = self.fetch_registers(tid)?;
        let rip = regs.rip;

        // Read a few bytes at RIP to detect CALL
        let code_bytes = self.read_memory(rip, 16)?;
        let (is_call, insn_len) = crate::x86_decode::detect_call_instruction(&code_bytes);

        if is_call && insn_len > 0 {
            // Set a temporary BP at the return address (next instruction)
            let next_rip = rip + insn_len as u64;
            let original_byte = self.read_memory(next_rip, 1)?[0];
            if original_byte != 0xCC {
                self.write_memory(next_rip, &[0xCC])?;
                self.state.breakpoints.insert(
                    next_rip,
                    Breakpoint {
                        address: next_rip,
                        original_byte,
                        enabled: true,
                        temporary: true,
                    },
                );
            }
            self.continue_execution()
        } else {
            self.single_step()
        }
    }

    /// Read a module/DLL image name from the target process.
    ///
    /// The `name_ptr_addr` points to a pointer in the target address space
    /// that itself points to the name string. `is_unicode` indicates whether
    /// the string is UTF-16.
    fn read_image_name_safe(&self, name_ptr_addr: u64, is_unicode: bool) -> String {
        if name_ptr_addr == 0 {
            return "<unknown>".to_string();
        }
        // Read the pointer value first
        let ptr_data = match self.read_memory(name_ptr_addr, 8) {
            Ok(d) => d,
            Err(_) => return "<unknown>".to_string(),
        };
        if ptr_data.len() < 8 {
            return "<unknown>".to_string();
        }
        let name_addr = u64::from_le_bytes([
            ptr_data[0], ptr_data[1], ptr_data[2], ptr_data[3],
            ptr_data[4], ptr_data[5], ptr_data[6], ptr_data[7],
        ]);
        if name_addr == 0 {
            return "<unknown>".to_string();
        }
        // Read the name string (up to 512 bytes)
        let raw = match self.read_memory(name_addr, 512) {
            Ok(d) => d,
            Err(_) => return "<unknown>".to_string(),
        };
        if is_unicode {
            // UTF-16 LE → find null terminator
            let u16s: Vec<u16> = raw
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&ch| ch != 0)
                .collect();
            String::from_utf16_lossy(&u16s)
        } else {
            // ASCII/ANSI → find null
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            String::from_utf8_lossy(&raw[..end]).to_string()
        }
    }

    /// Get the list of currently active threads
    pub fn threads(&self) -> &std::collections::BTreeMap<u32, crate::debug::types::ThreadInfo> {
        &self.state.threads
    }

    /// Get the list of currently loaded modules
    pub fn modules(&self) -> &std::collections::BTreeMap<u64, crate::debug::types::ModuleInfo> {
        &self.state.modules
    }

    /// Switch the active thread for register/step operations
    pub fn set_current_thread(&mut self, thread_id: u32) -> FissionResult<()> {
        if self.state.threads.contains_key(&thread_id) {
            self.state.current_thread_id = Some(thread_id);
            Ok(())
        } else {
            Err(FissionError::debug(format!(
                "Thread {} not found in tracked threads",
                thread_id
            )))
        }
    }

    /// Poll for a debug event with a timeout (ms).
    ///
    /// Calls `WaitForDebugEvent`, processes the raw event through
    /// [`process_debug_event`](Self::process_debug_event), and calls
    /// `ContinueDebugEvent` with the appropriate status.
    ///
    /// Returns the translated event (if any).
    pub fn poll_event(
        &mut self,
        timeout_ms: u32,
    ) -> FissionResult<Option<crate::debug::types::DebugEvent>> {
        let mut raw = DEBUG_EVENT::default();
        let wait_ok = unsafe { WaitForDebugEvent(&mut raw, timeout_ms) };
        if wait_ok.is_err() {
            return Ok(None);
        }
        let pid = raw.dwProcessId;
        let tid = raw.dwThreadId;
        let (evt, status) = self.process_debug_event(&raw);
        unsafe {
            let _ = ContinueDebugEvent(pid, tid, status);
        }
        Ok(evt)
    }
}

/// Start debug event loop for the attached process (legacy channel-based API).
///
/// Spawns a background thread that polls for Win32 debug events, translates
/// them into [`DebugEvent`] values, and sends them through a crossbeam channel.
///
/// > **Prefer** [`WindowsDebugger::poll_event`] for new code — it keeps
/// > `DebugState` synchronized automatically.
pub fn start_event_loop(
    pid: u32,
    tx: Sender<crate::debug::types::DebugEvent>,
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
                            Some(crate::debug::types::DebugEvent::BreakpointHit {
                                address,
                                thread_id,
                            })
                        } else if code_raw == EXCEPTION_SINGLE_STEP_CODE {
                            Some(crate::debug::types::DebugEvent::SingleStep { thread_id })
                        } else {
                            Some(crate::debug::types::DebugEvent::Exception {
                                code: code_raw,
                                address,
                                first_chance: is_first,
                            })
                        }
                    },
                    CREATE_PROCESS_DEBUG_EVENT => {
                        Some(crate::debug::types::DebugEvent::ProcessCreated {
                            pid: proc_id,
                            main_thread_id: thread_id,
                        })
                    }
                    EXIT_PROCESS_DEBUG_EVENT => {
                        let exit_code = unsafe { debug_event.u.ExitProcess.dwExitCode };
                        Some(crate::debug::types::DebugEvent::ProcessExited { exit_code })
                    }
                    CREATE_THREAD_DEBUG_EVENT => {
                        Some(crate::debug::types::DebugEvent::ThreadCreated { thread_id })
                    }
                    EXIT_THREAD_DEBUG_EVENT => {
                        let _exit_code = unsafe { debug_event.u.ExitThread.dwExitCode };
                        Some(crate::debug::types::DebugEvent::ThreadExited { thread_id })
                    }
                    LOAD_DLL_DEBUG_EVENT => Some(crate::debug::types::DebugEvent::DllLoaded {
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

/// Extract a short module name from a full path.
fn module_short_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
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
            let registers = crate::debug::types::RegisterState {
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

        let bp = crate::debug::types::Breakpoint {
            address,
            original_byte,
            enabled: true,
            temporary: false,
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

    fn fetch_registers(
        &mut self,
        thread_id: u32,
    ) -> FissionResult<crate::debug::types::RegisterState> {
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            let res = GetThreadContext(h_thread, &mut ctx);
            let _ = CloseHandle(h_thread);

            res.map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            // Map Windows CONTEXT to our RegisterState (x64)
            Ok(crate::debug::types::RegisterState {
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
