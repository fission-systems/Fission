//! Process enumeration using Windows API.

use super::super::types::ProcessInfo;

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
