//! Process enumeration using Windows API.

use super::super::types::ProcessInfo;

use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::System::ProcessStatus::{EnumProcesses, GetModuleBaseNameW};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

/// Enumerate all running processes
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();
    let mut pids: [u32; 4096] = [0; 4096];
    let mut bytes_returned: u32 = 0;

    unsafe {
        // Get list of all PIDs
        if EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut bytes_returned,
        )
        .is_err()
        {
            return processes;
        }

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
