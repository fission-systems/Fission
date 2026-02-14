use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub address: u64,
    pub original_byte: u8,
    pub enabled: bool,
}

pub struct BreakpointManager {
    pub breakpoints: HashMap<u64, Breakpoint>,
}

impl BreakpointManager {
    pub fn new() -> Self {
        Self {
            breakpoints: HashMap::new(),
        }
    }

    /// Sets a software breakpoint (INT 3) at the specified address.
    /// Returns true if successful.
    #[cfg(target_os = "windows")]
    pub fn set_breakpoint(
        &mut self,
        process_handle: windows::Win32::Foundation::HANDLE,
        address: u64,
    ) -> Result<(), String> {
        // 1. Check if already exists
        if let Some(bp) = self.breakpoints.get_mut(&address) {
            if bp.enabled {
                return Ok(()); // Already enabled
            }
            // Re-enable logic would go here (write 0xCC again)
        }

        // 2. Read original byte
        let original_data = super::memory::read_memory(process_handle, address, 1)?;
        if original_data.is_empty() {
            return Err("Failed to read memory at breakpoint address".to_string());
        }
        let original_byte = original_data[0];

        // 3. Write INT 3 (0xCC)
        let int3 = [0xCCu8];
        super::memory::write_memory(process_handle, address, &int3)?;

        // 4. Store record
        self.breakpoints.insert(
            address,
            Breakpoint {
                address,
                original_byte,
                enabled: true,
            },
        );

        Ok(())
    }

    /// Removes a software breakpoint, restoring the original byte.
    #[cfg(target_os = "windows")]
    pub fn remove_breakpoint(
        &mut self,
        process_handle: windows::Win32::Foundation::HANDLE,
        address: u64,
    ) -> Result<(), String> {
        if let Some(bp) = self.breakpoints.get(&address) {
            if bp.enabled {
                // Restore original byte
                let original = [bp.original_byte];
                super::memory::write_memory(process_handle, address, &original)?;
            }
            self.breakpoints.remove(&address);
            Ok(())
        } else {
            Err("Breakpoint not found".to_string())
        }
    }

    /// Checks if an address has an active breakpoint
    pub fn has_breakpoint(&self, address: u64) -> bool {
        self.breakpoints.contains_key(&address)
    }
}

#[cfg(not(target_os = "windows"))]
impl BreakpointManager {
    pub fn set_breakpoint(&mut self, _process_handle: usize, _address: u64) -> Result<(), String> {
        Err("Not supported on this OS".to_string())
    }
    pub fn remove_breakpoint(
        &mut self,
        _process_handle: usize,
        _address: u64,
    ) -> Result<(), String> {
        Err("Not supported on this OS".to_string())
    }
}
