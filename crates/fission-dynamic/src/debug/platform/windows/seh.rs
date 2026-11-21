//! Structured Exception Handler (SEH) chain reading for a live Windows process.
//!
//! Walks the `ExceptionList` linked list from the TEB to enumerate
//! registered SEH handlers for a given thread.

use std::ffi::c_void;
use windows::Wdk::System::Threading::{NtQueryInformationThread, THREADINFOCLASS};
use windows::Win32::Foundation::HANDLE;

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
/// TEB address, then walks the list `NtTib.ExceptionList` (TEB+0x00) points
/// at. A 64-bit thread's chain is normally empty: x64 code registers its
/// handlers in tables, not on this list. WOW64 threads' 32-bit chain is not
/// read here.
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
    let h_thread = super::OwnedHandle(h_thread);

    let mut tbi = ThreadBasicInformation {
        exit_status: 0,
        teb_base_address: 0,
        client_id: [0; 2],
        affinity_mask: 0,
        priority: 0,
        base_priority: 0,
    };
    let mut return_length: u32 = 0;
    let status = unsafe {
        NtQueryInformationThread(
            h_thread.0,
            THREADINFOCLASS(0), // ThreadBasicInformation
            &mut tbi as *mut _ as *mut c_void,
            std::mem::size_of::<ThreadBasicInformation>() as u32,
            &mut return_length,
        )
    };
    if status.0 < 0 {
        return Err(format!(
            "NtQueryInformationThread failed: NTSTATUS 0x{:08x}",
            status.0 as u32
        ));
    }

    let teb = tbi.teb_base_address;
    if teb == 0 {
        return Err("TEB address is null".to_string());
    }

    // The chain starts at the pointer stored in `NtTib.ExceptionList`
    // (TEB+0x00), not at the TEB: walking from the TEB itself read
    // `ExceptionList` as `Next` and `NtTib.StackBase` as a handler.
    let head: u64 = super::read_value(process, teb)?;
    let read_pointer = |address: u64| super::read_value::<u64>(process, address).ok();
    Ok(
        crate::debug::platform::windows_layout::walk_seh_chain(head, 8, 64, read_pointer)
            .into_iter()
            .map(|link| SehRecord {
                next: link.next,
                handler: link.handler,
                handler_module: None, // Would need module list to resolve
            })
            .collect(),
    )
}
