//! Windows-specific debugger implementation using Win32 Debug API.

mod process;
mod breakpoint;
mod execution;
mod memory;
mod pe;
mod register;
mod stack;

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
    DebugBreakProcess, EXCEPTION_DEBUG_EVENT, EXIT_PROCESS_DEBUG_EVENT, EXIT_THREAD_DEBUG_EVENT,
    GetThreadContext, LOAD_DLL_DEBUG_EVENT, OUTPUT_DEBUG_STRING_DEBUG_EVENT,
    OUTPUT_DEBUG_STRING_INFO, ReadProcessMemory, SetThreadContext, UNLOAD_DLL_DEBUG_EVENT,
    WaitForDebugEvent, WriteProcessMemory, WOW64_CONTEXT, WOW64_CONTEXT_ALL,
    Wow64GetThreadContext, Wow64SetThreadContext,
};
use windows::Win32::System::Memory::{
    PAGE_EXECUTE_READWRITE, PAGE_GUARD, PAGE_NOACCESS, PAGE_PROTECTION_FLAGS,
    PAGE_READONLY, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx, VirtualProtectEx,
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_RESERVE,
    VIRTUAL_ALLOCATION_TYPE, VIRTUAL_FREE_TYPE,
};
use windows::Win32::System::SystemInformation::{IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_I386};
use windows::Win32::System::Threading::{
    CreateProcessW, IsWow64Process2, OpenProcess, OpenThread, PROCESS_ALL_ACCESS,
    PROCESS_INFORMATION, STARTUPINFOW, THREAD_ALL_ACCESS, ResumeThread, SuspendThread,
    TerminateProcess,
};
use windows::core::PWSTR;

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

/// RAII guard that restores original page protection on drop.
struct ProtectGuard {
    process: HANDLE,
    address: u64,
    size: usize,
    old_protect: PAGE_PROTECTION_FLAGS,
    active: bool,
}

impl ProtectGuard {
    fn new(process: HANDLE, address: u64, size: usize, old_protect: PAGE_PROTECTION_FLAGS) -> Self {
        Self {
            process,
            address,
            size,
            old_protect,
            active: true,
        }
    }

    fn deactivate(mut self) {
        self.active = false;
    }
}

impl Drop for ProtectGuard {
    fn drop(&mut self) {
        if self.active {
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            unsafe {
                let _ = VirtualProtectEx(
                    self.process,
                    self.address as *const c_void,
                    self.size,
                    self.old_protect,
                    &mut _unused,
                );
            }
        }
    }
}

/// Windows debugger implementation
pub struct WindowsDebugger {
    /// Current debug state
    pub(crate) state: DebugState,
    /// Handle to the attached process
    pub(crate) process_handle: Option<HANDLE>,
    /// Execution timeline for auto-recording during debugging (shared with UI)
    pub ttd_timeline: Option<Arc<Mutex<Timeline>>>,
    /// Instruction decoder for step-over and disassembly
    pub(crate) decoder: Option<Box<dyn crate::decode::InstructionDecoder>>,
    /// Whether the attached process is a WOW64 (32-bit on 64-bit Windows) target.
    /// `None` when not yet determined (no process attached).
    pub(crate) is_wow64: Option<bool>,
    /// Active hardware breakpoints: address → DR slot index (0-3).
    pub(crate) hw_breakpoints: std::collections::BTreeMap<u64, u8>,
    /// Active memory breakpoints: address → (size, old_protect).
    pub(crate) memory_breakpoints: std::collections::BTreeMap<u64, (usize, PAGE_PROTECTION_FLAGS)>,
}

impl WindowsDebugger {
    /// Create a new Windows debugger instance.
    ///
    /// Automatically initialises a Sleigh decoder for x86-64.
    pub fn new() -> Self {
        let decoder = crate::decode::create_decoder("x86-64").ok();
        Self {
            state: DebugState::default(),
            process_handle: None,
            ttd_timeline: None,
            decoder,
            is_wow64: None,
            hw_breakpoints: std::collections::BTreeMap::new(),
            memory_breakpoints: std::collections::BTreeMap::new(),
        }
    }

    /// Attach the UI-backed [`Timeline`] for step recording during debugging
    pub fn set_ttd_timeline(&mut self, timeline: Arc<Mutex<Timeline>>) {
        self.ttd_timeline = Some(timeline);
    }

    /// Start TTD recording on the attached timeline (if any).
    pub fn start_ttd_recording(&self) {
        if let Some(arc) = &self.ttd_timeline {
            if let Ok(mut t) = arc.lock() {
                t.start_recording();
            }
        }
    }

    /// Stop TTD recording on the attached timeline (if any).
    pub fn stop_ttd_recording(&self) {
        if let Some(arc) = &self.ttd_timeline {
            if let Ok(mut t) = arc.lock() {
                t.stop_recording();
            }
        }
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
                let mod_size = self.get_module_size(base) as u64;
                self.state.modules.insert(
                    base,
                    crate::debug::types::ModuleInfo {
                        base_address: base,
                        size: mod_size,
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
                let mod_size = self.get_module_size(base) as u64;
                self.state.modules.insert(
                    base,
                    crate::debug::types::ModuleInfo {
                        base_address: base,
                        size: mod_size,
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
                    // Hardware breakpoints also raise STATUS_SINGLE_STEP; inspect Dr6.
                    if let Some(hw_addr) = self.check_hw_breakpoint_hit(thread_id) {
                        self.state.status = DebugStatus::Suspended;
                        self.state.last_event = Some(format!(
                            "Hardware breakpoint hit at 0x{:016x}",
                            hw_addr
                        ));
                        (
                            Some(crate::debug::types::DebugEvent::BreakpointHit {
                                address: hw_addr,
                                thread_id,
                            }),
                            DBG_CONTINUE,
                        )
                    } else {
                        self.state.status = DebugStatus::Suspended;
                        self.state.last_event =
                            Some(format!("Single step at thread {}", thread_id));
                        (
                            Some(crate::debug::types::DebugEvent::SingleStep { thread_id }),
                            DBG_CONTINUE,
                        )
                    }
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
            OUTPUT_DEBUG_STRING_DEBUG_EVENT => {
                let info = unsafe { debug_event.u.DebugString };
                let message = self.read_debug_string(&info);
                self.state.last_event = Some(format!("OutputDebugString: {}", message));
                (
                    Some(crate::debug::types::DebugEvent::OutputString { message }),
                    DBG_CONTINUE,
                )
            }
            _ => (None, DBG_CONTINUE),
        };

        (evt, status)
    }

    /// Attach an instruction decoder for disassembly and step-over.
    ///
    /// Replaces the current decoder (if any). Typically called after
    /// determining the target architecture from the PE header.
    pub fn set_decoder(&mut self, decoder: Box<dyn crate::decode::InstructionDecoder>) {
        self.decoder = Some(decoder);
    }

    /// Disassemble `count` instructions starting from `address`.
    ///
    /// Reads memory from the target process and decodes using the attached
    /// instruction decoder.
    pub fn disassemble_at(
        &self,
        address: u64,
        count: usize,
    ) -> FissionResult<Vec<crate::decode::DebugInstruction>> {
        let decoder = self.decoder.as_ref().ok_or_else(|| {
            FissionError::debug("No instruction decoder attached")
        })?;
        // Read enough bytes (estimate: 15 bytes/insn for x86, conservative)
        let read_size = (count * 15).min(4096);
        let bytes = self.read_memory(address, read_size)?;
        decoder
            .decode_window(&bytes, address, count)
            .map_err(|e| FissionError::debug(format!("Disassemble failed: {}", e)))
    }

    /// Disassemble around the current RIP for a context window.
    ///
    /// Returns up to `after` instructions starting from the current RIP.
    /// (Backward disassembly from an arbitrary address is unreliable for
    /// variable-length ISAs like x86; use function-start-based scanning
    /// if backward context is needed.)
    pub fn disassemble_around_rip(
        &self,
        after: usize,
    ) -> FissionResult<Vec<crate::decode::DebugInstruction>> {
        let rip = self
            .state
            .registers
            .as_ref()
            .map(|r| r.rip)
            .or_else(|| {
                self.state
                    .current_thread_id
                    .or(self.state.main_thread_id)
                    .and_then(|tid| self.fetch_registers(tid).ok())
                    .map(|r| r.rip)
            })
            .ok_or_else(|| FissionError::debug("No RIP available"))?;
        self.disassemble_at(rip, after)
    }

    /// Read a module/DLL image name from a process using a raw `HANDLE`.
    ///
    /// Standalone helper so both `WindowsDebugger` methods and the legacy
    /// `start_event_loop` can resolve image names without duplicating logic.
    fn read_image_name_from_process(
        h_process: HANDLE,
        name_ptr_addr: u64,
        is_unicode: bool,
    ) -> String {
        if name_ptr_addr == 0 {
            return "<unknown>".to_string();
        }
        let mut buf = [0u8; 8];
        let mut read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                h_process,
                name_ptr_addr as *const c_void,
                buf.as_mut_ptr() as *mut c_void,
                8,
                Some(&mut read),
            )
        };
        if ok.is_err() || read < 8 {
            return "<unknown>".to_string();
        }
        let name_addr = u64::from_le_bytes(buf);
        if name_addr == 0 {
            return "<unknown>".to_string();
        }
        let mut raw = [0u8; 512];
        let mut read2 = 0usize;
        let ok2 = unsafe {
            ReadProcessMemory(
                h_process,
                name_addr as *const c_void,
                raw.as_mut_ptr() as *mut c_void,
                512,
                Some(&mut read2),
            )
        };
        if ok2.is_err() || read2 == 0 {
            return "<unknown>".to_string();
        }
        if is_unicode {
            let u16s: Vec<u16> = raw[..read2]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&ch| ch != 0)
                .collect();
            String::from_utf16_lossy(&u16s)
        } else {
            let end = raw[..read2].iter().position(|&b| b == 0).unwrap_or(read2);
            String::from_utf8_lossy(&raw[..end]).to_string()
        }
    }

    /// Read a module/DLL image name from the target process.
    ///
    /// The `name_ptr_addr` points to a pointer in the target address space
    /// that itself points to the name string. `is_unicode` indicates whether
    /// the string is UTF-16.
    fn read_image_name_safe(&self, name_ptr_addr: u64, is_unicode: bool) -> String {
        match self.process_handle {
            Some(h) => Self::read_image_name_from_process(h, name_ptr_addr, is_unicode),
            None => "<unknown>".to_string(),
        }
    }

    /// Read an `OutputDebugString` message from the target process.
    ///
    /// The data is read directly from the target process address space using
    /// the address and length fields in the `OUTPUT_DEBUG_STRING_INFO` struct.
    fn read_debug_string(&self, info: &OUTPUT_DEBUG_STRING_INFO) -> String {
        let addr = info.lpDebugStringData as u64;
        let len = info.nDebugStringLength as usize;
        if addr == 0 || len == 0 {
            return String::new();
        }
        // nDebugStringLength includes the null terminator; read the full buffer
        match self.read_memory(addr, len.min(4096)) {
            Ok(raw) => {
                if info.fUnicode.0 != 0 {
                    let u16s: Vec<u16> = raw
                        .chunks_exact(2)
                        .map(|p| u16::from_le_bytes([p[0], p[1]]))
                        .take_while(|&c| c != 0)
                        .collect();
                    String::from_utf16_lossy(&u16s).to_string()
                } else {
                    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                    String::from_utf8_lossy(&raw[..end]).to_string()
                }
            }
            Err(_) => String::new(),
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

    /// Suspend a thread by ID. Returns the previous suspend count.
    pub fn suspend_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        unsafe {
            let h = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;
            let prev = SuspendThread(h);
            let _ = CloseHandle(h);
            if prev == u32::MAX {
                return Err(FissionError::debug(format!(
                    "SuspendThread failed for thread {}",
                    thread_id
                )));
            }
            if let Some(info) = self.state.threads.get_mut(&thread_id) {
                info.suspended = true;
            }
            Ok(prev)
        }
    }

    /// Resume a thread by ID. Returns the previous suspend count.
    pub fn resume_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        unsafe {
            let h = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;
            let prev = ResumeThread(h);
            let _ = CloseHandle(h);
            if prev == u32::MAX {
                return Err(FissionError::debug(format!(
                    "ResumeThread failed for thread {}",
                    thread_id
                )));
            }
            if prev <= 1 {
                if let Some(info) = self.state.threads.get_mut(&thread_id) {
                    info.suspended = false;
                }
            }
            Ok(prev)
        }
    }

    /// Determine the mapped size of a module by walking `VirtualQueryEx` pages
    /// starting at `base_address` until the region base changes.
    pub fn get_module_size(&self, base_address: u64) -> usize {
        let h_process = match self.process_handle {
            Some(h) => h,
            None => return 0,
        };
        let mut total: usize = 0;
        let mut addr = base_address;
        loop {
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            let ret = unsafe {
                VirtualQueryEx(
                    h_process,
                    Some(addr as *const c_void),
                    &mut mbi,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if ret == 0 {
                break;
            }
            if mbi.State != MEM_COMMIT {
                break;
            }
            if mbi.AllocationBase as u64 != base_address {
                break;
            }
            total += mbi.RegionSize;
            addr = match addr.checked_add(mbi.RegionSize as u64) {
                Some(a) => a,
                None => break,
            };
        }
        total
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

        // Auto-refresh register cache whenever the debuggee is suspended.
        if self.state.status == DebugStatus::Suspended {
            let thread_id = self.state.last_thread_id
                .or(self.state.current_thread_id)
                .or(self.state.main_thread_id);
            if let Some(tid_for_regs) = thread_id {
                if let Ok(regs) = self.fetch_registers(tid_for_regs) {
                    self.state.registers = Some(regs);
                }
            }
        }

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
                    LOAD_DLL_DEBUG_EVENT => {
                        let info = unsafe { debug_event.u.LoadDll };
                        let name = WindowsDebugger::read_image_name_from_process(
                            handle,
                            info.lpImageName as u64,
                            info.fUnicode.0 != 0,
                        );
                        Some(crate::debug::types::DebugEvent::DllLoaded {
                            base_address: info.lpBaseOfDll as u64,
                            name,
                        })
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debugger_new_not_attached() {
        let dbg = WindowsDebugger::new();
        assert!(!dbg.is_attached());
        assert_eq!(dbg.attached_pid(), None);
        assert_eq!(dbg.state().status, DebugStatus::Detached);
    }

    #[test]
    fn ttd_recording_no_timeline_is_noop() {
        let dbg = WindowsDebugger::new();
        // Should not panic when no timeline is attached
        dbg.start_ttd_recording();
        dbg.stop_ttd_recording();
    }

    #[test]
    fn ttd_recording_with_timeline_starts_and_stops() {
        let mut dbg = WindowsDebugger::new();
        let timeline = Arc::new(Mutex::new(Timeline::new()));
        dbg.set_ttd_timeline(timeline.clone());

        dbg.start_ttd_recording();
        assert!(timeline.lock().unwrap().is_recording());

        dbg.stop_ttd_recording();
        assert!(!timeline.lock().unwrap().is_recording());
    }

    #[test]
    fn get_module_size_no_handle_returns_zero() {
        let dbg = WindowsDebugger::new();
        assert_eq!(dbg.get_module_size(0x140000000), 0);
    }

    #[test]
    fn suspend_resume_unknown_thread_returns_error() {
        let mut dbg = WindowsDebugger::new();
        assert!(dbg.suspend_thread(0xdeadbeef).is_err());
        assert!(dbg.resume_thread(0xdeadbeef).is_err());
    }

    #[test]
    fn hw_breakpoint_slot_allocation_and_removal() {
        let mut dbg = WindowsDebugger::new();
        // Simulate 4 active slots without touching Windows APIs
        dbg.hw_breakpoints.insert(0x1000, 0);
        dbg.hw_breakpoints.insert(0x2000, 1);
        dbg.hw_breakpoints.insert(0x3000, 3);

        let used: std::collections::HashSet<u8> = dbg.hw_breakpoints.values().cloned().collect();
        let free_slot = (0..4u8).find(|i| !used.contains(i));
        assert_eq!(free_slot, Some(2));

        dbg.hw_breakpoints.insert(0x4000, 2);
        assert_eq!(dbg.hw_breakpoints.len(), 4);

        // Overflow
        let used2: std::collections::HashSet<u8> = dbg.hw_breakpoints.values().cloned().collect();
        let free_slot2 = (0..4u8).find(|i| !used2.contains(i));
        assert_eq!(free_slot2, None);

        // Removal frees slot
        dbg.hw_breakpoints.remove(&0x2000);
        let used3: std::collections::HashSet<u8> = dbg.hw_breakpoints.values().cloned().collect();
        let free_slot3 = (0..4u8).find(|i| !used3.contains(i));
        assert_eq!(free_slot3, Some(1));
    }

    #[test]
    fn process_debug_event_tracks_threads() {
        let mut dbg = WindowsDebugger::new();
        dbg.state.attached_pid = Some(1234);

        // CREATE_THREAD
        let mut evt = DEBUG_EVENT::default();
        evt.dwProcessId = 1234;
        evt.dwThreadId = 5678;
        evt.dwDebugEventCode = CREATE_THREAD_DEBUG_EVENT;
        let (de, _) = dbg.process_debug_event(&evt);
        assert!(matches!(de, Some(DebugEvent::ThreadCreated { thread_id: 5678 })));
        assert!(dbg.state.threads.contains_key(&5678));

        // EXIT_THREAD
        let mut evt2 = DEBUG_EVENT::default();
        evt2.dwProcessId = 1234;
        evt2.dwThreadId = 5678;
        evt2.dwDebugEventCode = EXIT_THREAD_DEBUG_EVENT;
        let (de2, _) = dbg.process_debug_event(&evt2);
        assert!(matches!(de2, Some(DebugEvent::ThreadExited { thread_id: 5678 })));
        assert!(!dbg.state.threads.contains_key(&5678));
    }

    #[test]
    fn process_debug_event_tracks_modules() {
        let mut dbg = WindowsDebugger::new();
        dbg.state.attached_pid = Some(1234);

        // LOAD_DLL
        let mut evt = DEBUG_EVENT::default();
        evt.dwProcessId = 1234;
        evt.dwThreadId = 1;
        evt.dwDebugEventCode = LOAD_DLL_DEBUG_EVENT;
        unsafe {
            evt.u.LoadDll.lpBaseOfDll = 0x7ff00000 as *mut c_void;
        }
        let (de, _) = dbg.process_debug_event(&evt);
        assert!(
            matches!(de, Some(DebugEvent::DllLoaded { base_address: 0x7ff00000, .. })),
            "expected DllLoaded, got {:?}", de
        );
        assert!(dbg.state.modules.contains_key(&0x7ff00000));

        // UNLOAD_DLL
        let mut evt2 = DEBUG_EVENT::default();
        evt2.dwProcessId = 1234;
        evt2.dwThreadId = 1;
        evt2.dwDebugEventCode = UNLOAD_DLL_DEBUG_EVENT;
        unsafe {
            evt2.u.UnloadDll.lpBaseOfDll = 0x7ff00000 as *mut c_void;
        }
        let (de2, _) = dbg.process_debug_event(&evt2);
        assert!(matches!(de2, Some(DebugEvent::DllUnloaded { base_address: 0x7ff00000 })));
        assert!(!dbg.state.modules.contains_key(&0x7ff00000));
    }

    #[test]
    fn process_debug_event_exception_sets_suspended() {
        let mut dbg = WindowsDebugger::new();
        dbg.state.attached_pid = Some(1234);
        dbg.state.status = DebugStatus::Running;

        let mut evt = DEBUG_EVENT::default();
        evt.dwProcessId = 1234;
        evt.dwThreadId = 1;
        evt.dwDebugEventCode = EXCEPTION_DEBUG_EVENT;
        unsafe {
            evt.u.Exception.ExceptionRecord.ExceptionCode = EXCEPTION_BREAKPOINT_CODE;
            evt.u.Exception.ExceptionRecord.ExceptionAddress = 0x401000 as *mut c_void;
        }
        let (de, _) = dbg.process_debug_event(&evt);
        assert!(
            matches!(de, Some(DebugEvent::BreakpointHit { address: 0x401000, thread_id: 1 })),
            "expected BreakpointHit, got {:?}", de
        );
        assert_eq!(dbg.state.status, DebugStatus::Suspended);
        assert_eq!(dbg.state.last_thread_id, Some(1));
    }

    #[test]
    fn read_debug_string_parses_ascii() {
        // Build an OUTPUT_DEBUG_STRING_INFO manually and test the parsing logic
        let raw = b"Hello world\0extra";
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let msg = String::from_utf8_lossy(&raw[..end]).to_string();
        assert_eq!(msg, "Hello world");
    }

    #[test]
    fn read_debug_string_parses_unicode() {
        let text = "Hello\0world";
        let u16s: Vec<u16> = text.encode_utf16().collect();
        let bytes: Vec<u8> = u16s
            .iter()
            .flat_map(|&c| c.to_le_bytes())
            .collect();
        let parsed: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .take_while(|&c| c != 0)
            .collect();
        assert_eq!(String::from_utf16_lossy(&parsed), "Hello");
    }
}
