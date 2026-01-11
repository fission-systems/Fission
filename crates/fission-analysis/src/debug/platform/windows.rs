//! Windows Memory Implementation
//!
//! Provides process memory operations using Windows API:
//! - `OpenProcess` for accessing target process
//! - `ReadProcessMemory` / `WriteProcessMemory` for memory I/O
//! - `VirtualQueryEx` for memory region enumeration

use super::{MemoryError, MemoryProtection, MemoryRegion, PlatformMemory};

/// Windows-specific memory manager
///
/// Uses Win32 API for process memory operations. Requires appropriate
/// privileges (SeDebugPrivilege) for accessing other processes.
pub struct WindowsMemory {
    /// Handle to the target process (HANDLE stored as isize)
    process_handle: Option<isize>,
}

impl WindowsMemory {
    /// Create a new Windows memory manager
    pub fn new() -> Self {
        Self {
            process_handle: None,
        }
    }

    /// Get the process handle if available
    fn get_handle(&self) -> Result<isize, MemoryError> {
        self.process_handle.ok_or(MemoryError::NoProcess)
    }
}

impl Default for WindowsMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformMemory for WindowsMemory {
    fn open_process(&mut self, pid: u32) -> Result<(), MemoryError> {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

        let handle = unsafe {
            OpenProcess(PROCESS_ALL_ACCESS, false, pid).map_err(|e| MemoryError::ReadFailed {
                address: 0,
                reason: format!("OpenProcess failed: {}", e),
            })?
        };

        self.process_handle = Some(handle.0 as isize);
        Ok(())
    }

    fn read_into(&self, address: u64, buffer: &mut [u8]) -> Result<usize, MemoryError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;

        let handle_val = self.get_handle()?;
        // SAFETY: HANDLE is repr(transparent) wrapper around isize
        let handle: HANDLE = unsafe { std::mem::transmute(handle_val) };
        let mut bytes_read = 0usize;

        unsafe {
            ReadProcessMemory(
                handle,
                address as *const std::ffi::c_void,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                buffer.len(),
                Some(&mut bytes_read),
            )
            .map_err(|e| MemoryError::ReadFailed {
                address,
                reason: e.to_string(),
            })?;
        }

        Ok(bytes_read)
    }

    fn write(&self, address: u64, data: &[u8]) -> Result<usize, MemoryError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;

        let handle_val = self.get_handle()?;
        // SAFETY: HANDLE is repr(transparent) wrapper around isize
        let handle: HANDLE = unsafe { std::mem::transmute(handle_val) };
        let mut bytes_written = 0usize;

        unsafe {
            WriteProcessMemory(
                handle,
                address as *const std::ffi::c_void,
                data.as_ptr() as *const std::ffi::c_void,
                data.len(),
                Some(&mut bytes_written),
            )
            .map_err(|e| MemoryError::WriteFailed {
                address,
                reason: e.to_string(),
            })?;
        }

        Ok(bytes_written)
    }

    fn query_regions(&self) -> Result<Vec<MemoryRegion>, MemoryError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Memory::{
            MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ,
            PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_READONLY, PAGE_READWRITE,
            PAGE_WRITECOPY, VirtualQueryEx,
        };

        let handle_val = self.get_handle()?;
        let handle: HANDLE = unsafe { std::mem::transmute(handle_val) };

        let mut regions = Vec::new();
        let mut address: usize = 0;
        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let mbi_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();

        loop {
            let result = unsafe {
                VirtualQueryEx(
                    handle,
                    Some(address as *const std::ffi::c_void),
                    &mut mbi,
                    mbi_size,
                )
            };

            if result == 0 {
                break;
            }

            // Only include committed memory
            if mbi.State == MEM_COMMIT {
                let protection = match mbi.Protect {
                    PAGE_EXECUTE => MemoryProtection {
                        read: false,
                        write: false,
                        execute: true,
                    },
                    PAGE_EXECUTE_READ => MemoryProtection::RX,
                    PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY => MemoryProtection::RWX,
                    PAGE_READONLY => MemoryProtection {
                        read: true,
                        write: false,
                        execute: false,
                    },
                    PAGE_READWRITE | PAGE_WRITECOPY => MemoryProtection::RW,
                    _ => MemoryProtection::NONE,
                };

                regions.push(MemoryRegion {
                    base_address: mbi.BaseAddress as u64,
                    size: mbi.RegionSize,
                    protection,
                    name: None, // Would need GetMappedFileName for this
                });
            }

            address = mbi.BaseAddress as usize + mbi.RegionSize;

            // Sanity check to avoid infinite loop
            if address == 0 || mbi.RegionSize == 0 {
                break;
            }
        }

        Ok(regions)
    }

    fn is_open(&self) -> bool {
        self.process_handle.is_some()
    }
}
