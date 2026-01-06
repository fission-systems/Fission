use super::types::*;

#[cfg(target_os = "windows")]
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Diagnostics::Debug::*,
    Win32::System::Threading::*,
};

/// The core debugging engine, mimicking TitanEngine's DebugLoop.
pub struct TitanEngine {
    pub active_process: Option<ProcessInfo>,
    pub bp_manager: super::breakpoint::BreakpointManager,
    pub importer: Option<super::importer::ImportReconstructor>,
}

impl TitanEngine {
    pub fn new() -> Self {
        Self {
            active_process: None,
            bp_manager: super::breakpoint::BreakpointManager::new(),
            importer: None,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn attach(&mut self, pid: u32) -> Result<(), String> {
        unsafe {
            // Clean Room: TitanEngine calls DebugActiveProcess(pid).
            // We do the same via windows-rs.
            if DebugActiveProcess(pid).as_bool() {
                // Note: We don't get handles immediately, we must wait for CREATE_PROCESS_DEBUG_EVENT
                Ok(())
            } else {
                Err(format!(
                    "DebugActiveProcess failed: {:?}",
                    std::io::Error::last_os_error()
                ))
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn attach(&mut self, _pid: u32) -> Result<(), String> {
        Err("TitanEngine is only supported on Windows".to_string())
    }

    #[cfg(target_os = "windows")]
    pub fn run(&mut self, path: &str) -> Result<(), String> {
        unsafe {
            let mut si = STARTUPINFOW::default();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut pi = PROCESS_INFORMATION::default();

            // Convert path to wide string (UTF-16)
            let mut wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let command_line = PWSTR(wide_path.as_mut_ptr());

            // Clean Room: CreateProcessW with DEBUG_ONLY_THIS_PROCESS
            let success = CreateProcessW(
                None,
                command_line,
                None,
                None,
                false,
                DEBUG_ONLY_THIS_PROCESS,
                None,
                None,
                &si,
                &mut pi,
            );

            if success.as_bool() {
                self.active_process = Some(ProcessInfo {
                    process_id: pi.dwProcessId,
                    thread_id: pi.dwThreadId,
                    process_handle: pi.hProcess,
                    thread_handle: pi.hThread,
                    image_base: 0, // Will be filled by CREATE_PROCESS_DEBUG_EVENT
                    entry_point: 0,
                });
                Ok(())
            } else {
                Err(format!(
                    "CreateProcess failed: {:?}",
                    std::io::Error::last_os_error()
                ))
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn run(&mut self, _path: &str) -> Result<(), String> {
        Err("TitanEngine is only supported on Windows".to_string())
    }

    #[cfg(target_os = "windows")]
    pub fn wait_for_debug_event(&mut self, timeout_ms: u32) -> Result<Option<DebugEvent>, String> {
        unsafe {
            let mut debug_event = DEBUG_EVENT::default();
            if WaitForDebugEvent(&mut debug_event, timeout_ms).as_bool() {
                let event = match debug_event.dwDebugEventCode {
                    CREATE_PROCESS_DEBUG_EVENT => {
                        let info = debug_event.u.CreateProcessInfo;
                        let entry_point = info.lpStartAddress
                            .map(|p| p as usize as u64)
                            .unwrap_or(0);
                        
                        // Update our process info if needed
                        if let Some(proc) = &mut self.active_process {
                            proc.image_base = info.lpBaseOfImage as u64;
                            proc.entry_point = entry_point;
                        }

                        // Initialize Importer
                        self.importer =
                            Some(super::importer::ImportReconstructor::new(info.hProcess));

                        Some(DebugEvent::ProcessCreated(ProcessInfo {
                            process_id: debug_event.dwProcessId,
                            thread_id: debug_event.dwThreadId,
                            process_handle: info.hProcess,
                            thread_handle: info.hThread,
                            image_base: info.lpBaseOfImage as u64,
                            entry_point,
                        }))
                    }
                    EXIT_PROCESS_DEBUG_EVENT => Some(DebugEvent::ProcessExit),
                    EXCEPTION_DEBUG_EVENT => {
                        let record = debug_event.u.Exception.ExceptionRecord;
                        let code = record.ExceptionCode.0 as u32;
                        let address = record.ExceptionAddress as u64;
                        let first_chance = debug_event.u.Exception.dwFirstChance != 0;

                        // Handle Breakpoint (0x80000003)
                        if code == 0x80000003 {
                            // Check if it's one of our breakpoints
                            // Note: Windows moves RIP to Address + 1 after INT 3
                            // But ExceptionAddress points to the INT 3 instruction itself (unlike Linux ptrace sometimes)
                            let bp_address = address;

                            if self.bp_manager.has_breakpoint(bp_address) {
                                // It is our breakpoint!
                                // We should probably restore the original byte here or let the UI decide.
                                // For now, we just report it.
                                Some(DebugEvent::Breakpoint(bp_address))
                            } else {
                                // System breakpoint or hardcoded INT 3
                                Some(DebugEvent::Breakpoint(bp_address))
                            }
                        } else {
                            Some(DebugEvent::Exception {
                                code,
                                address,
                                first_chance,
                            })
                        }
                    }
                    _ => None, // Handle other events
                };

                Ok(event)
            } else {
                // Timeout or error
                if std::io::Error::last_os_error().raw_os_error() == Some(121) {
                    // ERROR_SEM_TIMEOUT
                    Ok(None)
                } else {
                    Err(format!("WaitForDebugEvent failed"))
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub fn continue_debug_event(
        &self,
        pid: u32,
        tid: u32,
        continue_status: u32,
    ) -> Result<(), String> {
        unsafe {
            // continue_status should be DBG_CONTINUE (0x00010002) or DBG_EXCEPTION_NOT_HANDLED (0x80010001)
            let status =
                windows::Win32::System::Diagnostics::Debug::NTSTATUS(continue_status as i32);
            if ContinueDebugEvent(pid, tid, status).as_bool() {
                Ok(())
            } else {
                Err("ContinueDebugEvent failed".to_string())
            }
        }
    }

    // --- Memory Access Wrappers ---

    #[cfg(target_os = "windows")]
    pub fn read_memory(&self, address: u64, size: usize) -> Result<Vec<u8>, String> {
        if let Some(proc) = &self.active_process {
            super::memory::read_memory(proc.process_handle, address, size)
        } else {
            Err("No active process".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn write_memory(&self, address: u64, data: &[u8]) -> Result<usize, String> {
        if let Some(proc) = &self.active_process {
            super::memory::write_memory(proc.process_handle, address, data)
        } else {
            Err("No active process".to_string())
        }
    }

    // --- Context Access Wrappers ---

    #[cfg(target_os = "windows")]
    pub fn get_context(&self) -> Result<super::context::ThreadContext, String> {
        if let Some(proc) = &self.active_process {
            // Assuming we want the context of the main thread or the thread that caused the event.
            // For now, using the main thread handle stored in active_process.
            // In a real debug loop, we should track the thread ID of the current debug event.
            super::context::get_thread_context(proc.thread_handle, true)
        } else {
            Err("No active process".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn set_context(&self, context: &super::context::ThreadContext) -> Result<(), String> {
        if let Some(proc) = &self.active_process {
            super::context::set_thread_context(proc.thread_handle, context, true)
        } else {
            Err("No active process".to_string())
        }
    }

    // --- Import Reconstruction ---

    #[cfg(target_os = "windows")]
    pub fn resolve_import(
        &mut self,
        address: u64,
    ) -> Result<(String, Option<String>, u32), String> {
        if let Some(importer) = &mut self.importer {
            importer.update_modules()?;
            importer.resolve_address(address)
        } else {
            Err("Importer not initialized".to_string())
        }
    }

    // --- PE & Dumping ---

    #[cfg(target_os = "windows")]
    pub fn dump_process(&self, output_path: &str) -> Result<(), String> {
        if let Some(proc) = &self.active_process {
            super::dumper::dump_process(proc.process_handle, proc.image_base, output_path)
        } else {
            Err("No active process".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn dump_and_fix(
        &self,
        output_path: &str,
        imports: &[super::importer::ImportEntry],
    ) -> Result<(), String> {
        if let Some(proc) = &self.active_process {
            // 1. Dump Process
            super::dumper::dump_process(proc.process_handle, proc.image_base, output_path)?;

            // 2. Rebuild Imports
            super::dumper::rebuild_imports(output_path, imports, proc.image_base)?;

            Ok(())
        } else {
            Err("No active process".to_string())
        }
    }

    // --- Breakpoint Management ---

    #[cfg(target_os = "windows")]
    pub fn set_breakpoint(&mut self, address: u64) -> Result<(), String> {
        if let Some(proc) = &self.active_process {
            self.bp_manager.set_breakpoint(proc.process_handle, address)
        } else {
            Err("No active process".to_string())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn remove_breakpoint(&mut self, address: u64) -> Result<(), String> {
        if let Some(proc) = &self.active_process {
            self.bp_manager
                .remove_breakpoint(proc.process_handle, address)
        } else {
            Err("No active process".to_string())
        }
    }
}
