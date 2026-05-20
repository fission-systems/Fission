//! Read Windows OS structures (PEB, TEB) from a live process.
//!
//! Uses `NtQueryInformationProcess` / `NtQueryInformationThread`
//! to locate the structures, then `ReadProcessMemory` to extract fields.

use windows::Win32::Foundation::{HANDLE, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Threading::{NtQueryInformationProcess, NtQueryInformationThread};
use std::ffi::c_void;

/// Key fields from the Process Environment Block.
#[derive(Debug, Clone)]
pub struct PebInfo {
    pub peb_address: u64,
    pub image_base: u64,
    pub being_debugged: bool,
    pub nt_global_flag: u32,
    pub ldr_data: u64,
    pub process_parameters: u64,
}

/// Key fields from the Thread Environment Block.
#[derive(Debug, Clone)]
pub struct TebInfo {
    pub teb_address: u64,
    pub stack_base: u64,
    pub stack_limit: u64,
    pub exception_list: u64,
    pub self_ptr: u64,
    pub client_id_unique_process: u64,
    pub client_id_unique_thread: u64,
    pub peb_ptr: u64,
}

/// Read PEB from a target process.
pub fn read_peb(process: HANDLE) -> Result<PebInfo, String> {
    #[repr(C)]
    struct ProcessBasicInformation {
        exit_status: NTSTATUS,
        peb_base_address: u64,
        affinity_mask: u64,
        base_priority: i32,
        unique_process_id: u64,
        inherited_from_unique_process_id: u64,
    }

    let mut pbi = ProcessBasicInformation {
        exit_status: NTSTATUS(0),
        peb_base_address: 0,
        affinity_mask: 0,
        base_priority: 0,
        unique_process_id: 0,
        inherited_from_unique_process_id: 0,
    };
    let mut return_length: u32 = 0;
    unsafe {
        NtQueryInformationProcess(
            process,
            0, // ProcessBasicInformation
            &mut pbi as *mut _ as *mut c_void,
            std::mem::size_of::<ProcessBasicInformation>() as u32,
            &mut return_length,
        )
        .ok()
        .map_err(|e| format!("NtQueryInformationProcess failed: {:?}", e))?;
    }

    let peb = pbi.peb_base_address;
    if peb == 0 {
        return Err("PEB address is null".to_string());
    }

    // PEB offsets for x64:
    // 0x02 BeingDebugged (u8)
    // 0x08 ImageBaseAddress (u64)
    // 0x18 Ldr (u64)
    // 0x20 ProcessParameters (u64)
    // 0xBC NtGlobalFlag (u32)
    let mut read = 0usize;

    let mut being_debugged: u8 = 0;
    let mut image_base: u64 = 0;
    let mut ldr: u64 = 0;
    let mut process_params: u64 = 0;
    let mut nt_global: u32 = 0;

    unsafe {
        ReadProcessMemory(process, (peb + 0x02) as *const c_void, &mut being_debugged as *mut _ as *mut c_void, 1, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (peb + 0x08) as *const c_void, &mut image_base as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (peb + 0x18) as *const c_void, &mut ldr as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (peb + 0x20) as *const c_void, &mut process_params as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (peb + 0xBC) as *const c_void, &mut nt_global as *mut _ as *mut c_void, 4, Some(&mut read)).ok()?;
    }

    Ok(PebInfo {
        peb_address: peb,
        image_base,
        being_debugged: being_debugged != 0,
        nt_global_flag: nt_global,
        ldr_data: ldr,
        process_parameters: process_params,
    })
}

/// Read TEB for a specific thread.
pub fn read_teb(process: HANDLE, thread_id: u32) -> Result<TebInfo, String> {
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

    // TEB offsets for x64:
    // 0x00 NtTib.ExceptionList (u64)
    // 0x08 NtTib.StackBase (u64)
    // 0x10 NtTib.StackLimit (u64)
    // 0x30 Self (u64)
    // 0x40 ClientId.UniqueProcess (u64)
    // 0x48 ClientId.UniqueThread (u64)
    // 0x60 Peb (u64)
    let mut read = 0usize;
    let mut exception_list: u64 = 0;
    let mut stack_base: u64 = 0;
    let mut stack_limit: u64 = 0;
    let mut self_ptr: u64 = 0;
    let mut client_id_proc: u64 = 0;
    let mut client_id_thread: u64 = 0;
    let mut peb_ptr: u64 = 0;

    unsafe {
        ReadProcessMemory(process, teb as *const c_void, &mut exception_list as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x08) as *const c_void, &mut stack_base as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x10) as *const c_void, &mut stack_limit as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x30) as *const c_void, &mut self_ptr as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x40) as *const c_void, &mut client_id_proc as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x48) as *const c_void, &mut client_id_thread as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
        ReadProcessMemory(process, (teb + 0x60) as *const c_void, &mut peb_ptr as *mut _ as *mut c_void, 8, Some(&mut read)).ok()?;
    }

    Ok(TebInfo {
        teb_address: teb,
        stack_base,
        stack_limit,
        exception_list,
        self_ptr,
        client_id_unique_process: client_id_proc,
        client_id_unique_thread: client_id_thread,
        peb_ptr,
    })
}
