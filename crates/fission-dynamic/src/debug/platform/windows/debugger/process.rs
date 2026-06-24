//! Process enumeration using Windows API.

use crate::debug::types::ProcessInfo;

use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::System::ProcessStatus::{EnumProcesses, GetModuleBaseNameW};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_READ, QueryFullProcessImageNameW,
};

/// Enumerate all running processes
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    unsafe {
        // Dynamically grow the PID buffer until EnumProcesses has room for all PIDs.
        // The API fills the buffer and sets bytes_returned; if bytes_returned >= cb
        // the buffer may have been truncated — double the capacity and retry.
        let mut capacity = 512usize;
        let mut bytes_returned: u32 = 0;
        let pids: Vec<u32> = loop {
            let mut buf = vec![0u32; capacity];
            let cb = (capacity * std::mem::size_of::<u32>()) as u32;
            if EnumProcesses(buf.as_mut_ptr(), cb, &mut bytes_returned).is_err() {
                return processes;
            }
            if bytes_returned >= cb {
                capacity = capacity.saturating_mul(2);
                continue;
            }
            break buf;
        };

        let num_processes = bytes_returned as usize / std::mem::size_of::<u32>();

        for &pid in pids.iter().take(num_processes) {
            if pid == 0 {
                continue;
            }

            // Try to open process with query info rights
            let handle = match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            {
                Ok(h) => h,
                Err(_) => {
                    // Try with limited info
                    match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        Ok(h) => h,
                        Err(_) => continue,
                    }
                }
            };

            // Get process name
            let name = get_process_name(handle).unwrap_or_else(|| format!("<PID {}>", pid));

            // Get executable path
            let exe_path = get_process_exe_path(handle);

            let _ = CloseHandle(handle);

            processes.push(ProcessInfo {
                pid,
                name,
                exe_path,
            });
        }
    }

    // Sort by name
    processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    processes
}

/// Get process name from handle
fn get_process_name(handle: HANDLE) -> Option<String> {
    let mut name_buf = [0u16; MAX_PATH as usize];

    unsafe {
        let len = GetModuleBaseNameW(handle, None, &mut name_buf);

        if len == 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&name_buf[..len as usize]))
    }
}

/// Get the full executable path from handle
fn get_process_exe_path(handle: HANDLE) -> Option<String> {
    let mut path_buf = [0u16; MAX_PATH as usize * 2];
    let mut size = path_buf.len() as u32;

    unsafe {
        if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(path_buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok()
            && size > 0
        {
            Some(String::from_utf16_lossy(&path_buf[..size as usize]))
        } else {
            None
        }
    }
}

use super::WindowsDebugger;
use crate::debug::traits::Debugger;
use fission_core::{FissionError, Result as FissionResult};

impl Debugger for WindowsDebugger {
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

        // Detect WOW64 (32-bit process on 64-bit Windows)
        if let Some(h) = self.process_handle {
            let mut process_machine = IMAGE_FILE_MACHINE(0);
            let mut native_machine = IMAGE_FILE_MACHINE(0);
            if unsafe { IsWow64Process2(h, &mut process_machine, &mut native_machine) }.is_ok() {
                self.is_wow64 = Some(process_machine == IMAGE_FILE_MACHINE_I386);
            } else {
                self.is_wow64 = Some(false);
            }
        }

        // Auto-start TTD recording if a timeline is already attached
        self.start_ttd_recording();

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
        self.is_wow64 = None;

        // Stop TTD recording if active
        self.stop_ttd_recording();

        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.state.attached_pid.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        self.state.attached_pid
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_path: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let mut cmd_line = path.to_string();
        for a in args {
            cmd_line.push(' ');
            cmd_line.push_str(a);
        }
        let mut wide_cmd: Vec<u16> = OsStr::new(&cmd_line).encode_wide().chain(Some(0)).collect();

        let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

        let creation_flags = windows::Win32::System::Threading::DEBUG_PROCESS
            | windows::Win32::System::Threading::DEBUG_ONLY_THIS_PROCESS;

        unsafe {
            CreateProcessW(
                PWSTR(wide_path.as_ptr() as *mut u16),
                PWSTR(wide_cmd.as_mut_ptr()),
                std::ptr::null(),
                std::ptr::null(),
                false,
                creation_flags,
                std::ptr::null(),
                PWSTR(std::ptr::null_mut()),
                &si,
                &pi,
            )
            .map_err(|e| FissionError::debug(format!("CreateProcessW failed: {:?}", e)))?;
        }

        let pid = pi.dwProcessId;
        self.state.attached_pid = Some(pid);
        self.state.status = DebugStatus::Running;
        self.state.last_event = Some(format!("Launched PID {} ({})", pid, path));

        // Open process handle immediately
        let _ = self.ensure_process_handle();

        // Detect WOW64
        if let Some(h) = self.process_handle {
            let mut process_machine = IMAGE_FILE_MACHINE(0);
            let mut native_machine = IMAGE_FILE_MACHINE(0);
            if unsafe { IsWow64Process2(h, &mut process_machine, &mut native_machine) }.is_ok() {
                self.is_wow64 = Some(process_machine == IMAGE_FILE_MACHINE_I386);
            } else {
                self.is_wow64 = Some(false);
            }
        }

        // Auto-start TTD recording if a timeline is already attached
        self.start_ttd_recording();

        Ok(pid)
    }
    fn pause(&mut self) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;
        unsafe {
            DebugBreakProcess(h)
                .map_err(|e| FissionError::debug(format!("DebugBreakProcess failed: {:?}", e)))?;
        }
        self.state.last_event = Some("Break requested".to_string());
        Ok(())
    }
    fn terminate(&mut self) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;
        unsafe {
            TerminateProcess(h, 1)
                .map_err(|e| FissionError::debug(format!("TerminateProcess failed: {:?}", e)))?;
        }
        self.state.status = DebugStatus::Stopped;
        self.state.last_event = Some("Process terminated".to_string());
        Ok(())
    }
}
