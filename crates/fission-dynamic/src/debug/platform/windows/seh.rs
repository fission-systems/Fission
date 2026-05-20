//! Structured Exception Handler (SEH) chain reading for a live Windows process.
//!
//! Walks the `ExceptionList` linked list from the TEB to enumerate
//! registered SEH handlers for a given thread.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Threading::NtQueryInformationThread;
use std::ffi::c_void;

/// One SEH record from the target thread.
#[derive(Debug, Clone)]
pub struct SehRecord {
    pub next: u64,
    pub handler: u64,
    /// Resolved module name + symbol when available.
    pub handler_module: Option<String>,
}

/// Read the SEH chain for `thread_id` in `process`.
///
/// Uses `NtQueryInformationThread` with `ThreadBasicInformation` to get the
/// TEB address, then walks `ExceptionList` (TEB+0x00 on x64 = GS:[0]).
pub fn read_seh_chain(process: HANDLE, thread_id: u32) -> Result<Vec<SehRecord>, String> {
    #[repr(C)]
    struct ThreadBasicInformation {
        exit_status: i32,
        teb_base_address: u64,
        client_id: [u64; 2],
        affinity_mask: u64,
        priority: i32,
        base_priority: i32,
    }

    let h_thread = unsafe {
        windows::Win32::System::Threading::OpenThread(
            windows::Win32::System::Threading::THREAD_QUERY_INFORMATION,
            false,
            thread_id,
        )
        .map_err(|e| format!("OpenThread failed: {:?}", e))?
    };

    let mut tbi = ThreadBasicInformation {
        exit_status: 0,
        teb_base_address: 0,
        client_id: [0; 2],
        affinity_mask: 0,
        priority: 0,
        base_priority: 0,
    };
    let mut return_length: u32 = 0;
    unsafe {
        NtQueryInformationThread(
            h_thread,
            0, // ThreadBasicInformation
            &mut tbi as *mut _ as *mut c_void,
            std::mem::size_of::<ThreadBasicInformation>() as u32,
            &mut return_length,
        )
        .ok()
        .map_err(|e| format!("NtQueryInformationThread failed: {:?}", e))?;
    }

    let teb = tbi.teb_base_address;
    if teb == 0 {
        return Err("TEB address is null".to_string());
    }

    // On x64, ExceptionList is at TEB+0x00 (NtTib.ExceptionList)
    let exception_list_addr = teb;
    let mut current = exception_list_addr;
    let mut records = Vec::new();
    let max_depth = 64;

    for _ in 0..max_depth {
        if current == 0 || current == 0xFFFFFFFF {
            break;
        }

        let mut next: u64 = 0;
        let mut handler: u64 = 0;
        let mut read = 0usize;

        unsafe {
            ReadProcessMemory(
                process,
                current as *const c_void,
                &mut next as *mut _ as *mut c_void,
                8,
                Some(&mut read),
            )
            .ok()
            .map_err(|e| format!("ReadProcessMemory failed: {:?}", e))?;

            ReadProcessMemory(
                process,
                (current + 8) as *const c_void,
                &mut handler as *mut _ as *mut c_void,
                8,
                Some(&mut read),
            )
            .ok()
            .map_err(|e| format!("ReadProcessMemory failed: {:?}", e))?;
        }

        records.push(SehRecord {
            next,
            handler,
            handler_module: None, // Would need module list to resolve
        });

        current = next;
    }

    Ok(records)
}
