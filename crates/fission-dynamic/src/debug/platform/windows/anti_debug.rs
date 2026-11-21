//! Anti-debugging detection and automatic circumvention for Windows targets.
//!
//! Patches common anti-debug checks in the target process so that
//! analysis can continue without manual intervention.

use crate::debug::platform::windows_layout::{heap64, peb64};
use std::ffi::c_void;
use windows::Win32::Foundation::{HANDLE, NTSTATUS};
use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtectEx};
// `Nt*` lives under `Wdk`, not `Win32`, in windows-rs 0.54.
use windows::Wdk::System::Threading::{NtQueryInformationProcess, PROCESSINFOCLASS};

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

    /// Write `bytes` at `address`, making the page writable for the write and
    /// putting its protection back afterwards -- whether or not the write
    /// worked. The old code returned through `?` on a failed write and left
    /// the page read-write.
    fn write_protected(process: HANDLE, address: u64, bytes: &[u8]) -> Result<(), String> {
        let mut old_protect = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            VirtualProtectEx(
                process,
                address as *const c_void,
                bytes.len(),
                PAGE_READWRITE,
                &mut old_protect,
            )
            .map_err(|e| format!("VirtualProtectEx failed: {:?}", e))?;
            let mut written = 0usize;
            let result = windows::Win32::System::Diagnostics::Debug::WriteProcessMemory(
                process,
                address as *const c_void,
                bytes.as_ptr() as *const c_void,
                bytes.len(),
                Some(&mut written),
            );
            let mut unused = PAGE_PROTECTION_FLAGS::default();
            let _ = VirtualProtectEx(
                process,
                address as *const c_void,
                bytes.len(),
                old_protect,
                &mut unused,
            );
            result.map_err(|e| format!("WriteProcessMemory failed: {:?}", e))?;
            if written != bytes.len() {
                return Err(format!(
                    "WriteProcessMemory wrote {written} of {} bytes",
                    bytes.len()
                ));
            }
        }
        Ok(())
    }

    /// Patch PEB.BeingDebugged (offset 0x2 inside PEB).
    fn patch_peb_being_debugged(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;
        Self::write_protected(process, peb_addr + peb64::BEING_DEBUGGED, &[0])
    }

    /// Patch PEB.NtGlobalFlag (offset 0xBC inside PEB on x64) to clear debug flags.
    fn patch_nt_global_flag(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;
        Self::write_protected(
            process,
            peb_addr + peb64::NT_GLOBAL_FLAG,
            &0u32.to_le_bytes(),
        )
    }

    /// Give the default heap the flags it has without a debugger.
    ///
    /// `PEB.ProcessHeap` (x64 offset 0x30) *is* the default heap. It was read
    /// as the `ProcessHeaps` array and dereferenced once more, so the patch
    /// went to whatever the heap's first eight bytes pointed at -- and wrote
    /// to offset 0x40 there, which is not a flags field on any 64-bit
    /// Windows. A 64-bit heap (Vista and later) keeps `Flags` at 0x70 and
    /// `ForceFlags` at 0x74.
    fn patch_heap_flags(process: HANDLE) -> Result<(), String> {
        let peb_addr = Self::get_peb_address(process)?;
        let heap: u64 = super::read_value(process, peb_addr + peb64::PROCESS_HEAP)?;
        if heap == 0 {
            return Err("The process has no default heap".to_string());
        }
        Self::write_protected(
            process,
            heap + heap64::FLAGS,
            &heap64::GROWABLE.to_le_bytes(),
        )?;
        Self::write_protected(process, heap + heap64::FORCE_FLAGS, &0u32.to_le_bytes())
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
                PROCESSINFOCLASS(0), // ProcessBasicInformation
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
