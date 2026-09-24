use super::*;
use fission_core::{FissionError, Result as FissionResult};

impl WindowsDebugger {
    pub(super) fn remote_alloc(&mut self, address: u64, size: usize) -> FissionResult<u64> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        let addr = unsafe {
            VirtualAllocEx(
                h,
                if address == 0 {
                    None
                } else {
                    Some(address as *const c_void)
                },
                size,
                VIRTUAL_ALLOCATION_TYPE(MEM_COMMIT.0 | MEM_RESERVE.0),
                PAGE_EXECUTE_READWRITE,
            )
        };
        if addr.is_null() {
            return Err(FissionError::debug("VirtualAllocEx failed"));
        }
        Ok(addr as u64)
    }

    pub(super) fn remote_free(&mut self, address: u64) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        unsafe {
            VirtualFreeEx(
                h,
                address as *mut c_void,
                0,
                VIRTUAL_FREE_TYPE(MEM_RELEASE.0),
            )
            .map_err(|e| FissionError::debug(format!("VirtualFreeEx failed: {:?}", e)))?;
        }
        Ok(())
    }

    pub(super) fn get_page_rights(&self, address: u64) -> FissionResult<u32> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let query_size = unsafe {
            VirtualQueryEx(
                h,
                Some(address as *const c_void),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if query_size == 0 {
            return Err(FissionError::debug("VirtualQueryEx failed"));
        }
        Ok(mbi.Protect.0)
    }

    pub(super) fn set_page_rights(
        &mut self,
        address: u64,
        size: usize,
        protect: u32,
    ) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        unsafe {
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            VirtualProtectEx(
                h,
                address as *const c_void,
                size,
                PAGE_PROTECTION_FLAGS(protect),
                &mut _unused,
            )
            .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;
        }
        Ok(())
    }
}
