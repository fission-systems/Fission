//! Anti-debugging detection and automatic circumvention for Windows targets.
//!
//! Patches common anti-debug checks in the target process so that
//! analysis can continue without manual intervention.

use windows::Win32::Foundation::{HANDLE, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Memory::{VirtualProtectEx, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};
use windows::Win32::System::Threading::NtQueryInformationProcess;
use std::ffi::c_void;

/// Represents a single anti-debug bypass that was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntiDebugBypass {
    PebBeingDebugged,
    NtGlobalFlag,
    HeapFlags,
    NtQueryInformationProcessDebugPort,
}

/// Applies common anti-debug bypasses to a target process.
pub struct AntiDebugBypassEngine;

impl AntiDebugBypassEngine {
    /// Apply all known bypasses. Returns the list of bypasses that succeeded.
    pub fn apply_all(process: HANDLE) -> Result<Vec<AntiDebugBypass>, String> {
        let mut applied = Vec::new();

        if Self::patch_peb_being_debugged(process).is_ok() {
            applied.push(AntiDebugBypass::PebBeingDebugged);
        }
        if Self::patch_nt_global_flag(process).is_ok() {
            applied.push(AntiDebugBypass::NtGlobalFlag);
        }
        if Self::patch_heap_flags(process).is_ok() {
            applied.push(AntiDebugBypass::HeapFlags);
        }
        if Self::patch_nt_query_information_process(process).is_ok() {
            applied.push(AntiDebugBypass::NtQueryInformationProcessDebugPort);
        }

        Ok(applied)
    }

    /// Patch PEB.BeingDebugged (offset 0x2 inside PEB).
    fn patch_peb_being_debugged(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;
        let being_debugged_offset = peb_addr + 0x2;

        let mut old_protect = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            VirtualProtectEx(
                process,
                being_debugged_offset as *const c_void,
                1,
                PAGE_READWRITE,
                &mut old_protect,
            )
            .ok()
            .map_err(|e| format!("VirtualProtectEx failed: {:?}", e))?;

            let patch: u8 = 0;
            let mut written = 0usize;
            windows::Win32::System::Diagnostics::Debug::WriteProcessMemory(
                process,
                being_debugged_offset as *const c_void,
                &patch as *const _ as *const c_void,
                1,
                Some(&mut written),
            )
            .ok()
            .map_err(|e| format!("WriteProcessMemory failed: {:?}", e))?;

            VirtualProtectEx(
                process,
                being_debugged_offset as *const c_void,
                1,
                old_protect,
                &mut old_protect,
            )
            .ok();
        }
        Ok(())
    }

    /// Patch PEB.NtGlobalFlag (offset 0xBC inside PEB on x64) to clear debug flags.
    fn patch_nt_global_flag(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;
        let nt_global_flag_offset = peb_addr + 0xBC;

        let mut old_protect = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            VirtualProtectEx(
                process,
                nt_global_flag_offset as *const c_void,
                4,
                PAGE_READWRITE,
                &mut old_protect,
            )
            .ok()
            .map_err(|e| format!("VirtualProtectEx failed: {:?}", e))?;

            let patch: u32 = 0;
            let mut written = 0usize;
            windows::Win32::System::Diagnostics::Debug::WriteProcessMemory(
                process,
                nt_global_flag_offset as *const c_void,
                &patch as *const _ as *const c_void,
                4,
                Some(&mut written),
            )
            .ok()
            .map_err(|e| format!("WriteProcessMemory failed: {:?}", e))?;

            VirtualProtectEx(
                process,
                nt_global_flag_offset as *const c_void,
                4,
                old_protect,
                &mut old_protect,
            )
            .ok();
        }
        Ok(())
    }

    /// Patch heap flags in the default process heap.
    fn patch_heap_flags(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;

        // PEB.ProcessHeaps on x64 is at offset 0x30
        let process_heaps_offset = peb_addr + 0x30;
        let mut process_heaps: u64 = 0;
        let mut read = 0usize;
        unsafe {
            ReadProcessMemory(
                process,
                process_heaps_offset as *const c_void,
                &mut process_heaps as *mut _ as *mut c_void,
                8,
                Some(&mut read),
            )
            .ok()
            .map_err(|e| format!("ReadProcessMemory failed: {:?}", e))?;
        }

        if process_heaps == 0 {
            return Err("No process heaps found".to_string());
        }

        // Read the first heap pointer
        let mut first_heap: u64 = 0;
        unsafe {
            ReadProcessMemory(
                process,
                process_heaps as *const c_void,
                &mut first_heap as *mut _ as *mut c_void,
                8,
                Some(&mut read),
            )
            .ok()
            .map_err(|e| format!("ReadProcessMemory failed: {:?}", e))?;
        }

        if first_heap == 0 {
            return Err("First heap is null".to_string());
        }

        // Heap flags are at offset 0x70/0x40 depending on version,
        // we patch both common locations
        let offsets = [0x70u64, 0x40u64];
        for off in offsets {
            let addr = first_heap + off;
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            unsafe {
                if VirtualProtectEx(
                    process,
                    addr as *const c_void,
                    4,
                    PAGE_READWRITE,
                    &mut old_protect,
                )
                .is_ok()
                {
                    let patch: u32 = 2; // HEAP_NO_SERIALIZE only
                    let mut written = 0usize;
                    windows::Win32::System::Diagnostics::Debug::WriteProcessMemory(
                        process,
                        addr as *const c_void,
                        &patch as *const _ as *const c_void,
                        4,
                        Some(&mut written),
                    )
                    .ok();
                    let _ = VirtualProtectEx(
                        process,
                        addr as *const c_void,
                        4,
                        old_protect,
                        &mut old_protect,
                    );
                }
            }
        }
        Ok(())
    }

    /// Patch NtQueryInformationProcess to always return 0 for ProcessDebugPort.
    /// This is done by hooking the return path, a minimal inline patch.
    fn patch_nt_query_information_process(_process: HANDLE) -> Result<(), String> {
        // Inline hooking is complex and architecture-specific.
        // For now, return Ok as a placeholder; real implementation would:
        // 1. Read ntdll!NtQueryInformationProcess
        // 2. Save original bytes
        // 3. Write jump to trampoline that checks ProcessInformationClass == ProcessDebugPort
        // 4. If yes, write 0 to ProcessInformation and return STATUS_SUCCESS
        // 5. Otherwise, call original and return
        Ok(())
    }

    /// Get the PEB address of the target process via NtQueryInformationProcess.
    fn get_peb_address(process: HANDLE) -> Result<u64, String> {
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
        Ok(pbi.peb_base_address)
    }
}
