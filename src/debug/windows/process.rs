//! Process enumeration using Windows API.

use super::super::types::ProcessInfo;

use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH};
use windows::Win32::System::ProcessStatus::{
    EnumProcesses, GetModuleBaseNameW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
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
        ).is_err() {
            return processes;
        }

        let num_processes = bytes_returned as usize / std::mem::size_of::<u32>();

        for &pid in pids.iter().take(num_processes) {
            if pid == 0 {
                continue;
            }

            // Try to open process
            let handle = match OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            ) {
                Ok(h) => h,
                Err(_) => continue, // Skip processes we can't access
            };

            // Get process name
            let name = get_process_name(handle).unwrap_or_else(|| format!("<PID {}>", pid));
            
            let _ = CloseHandle(handle);

            processes.push(ProcessInfo { pid, name });
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
