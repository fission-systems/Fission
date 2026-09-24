use super::*;
use fission_core::{FissionError, Result as FissionResult};

impl WindowsDebugger {
    pub(super) fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        // Read original byte
        let original_byte = self.read_memory(address, 1)?[0];
        if original_byte == 0xCC {
            return Err(FissionError::debug(
                "Breakpoint already exists at this address",
            ));
        }

        // Patch with INT3 (0xCC)
        self.write_memory(address, &[0xCC])?;

        let bp = crate::debug::types::Breakpoint {
            address,
            original_byte,
            enabled: true,
            temporary: false,
            kind: crate::debug::types::BreakpointKind::Software,
            hits: 0,
            condition: None,
        };
        self.state.breakpoints.insert(address, bp);
        self.state.last_event = Some(format!("Breakpoint set 0x{:016x}", address));
        Ok(())
    }

    pub(super) fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let bp = self
            .state
            .breakpoints
            .get(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        if bp.kind != crate::debug::types::BreakpointKind::Software {
            return Err(FissionError::debug(
                "Only software breakpoints can be removed by address",
            ));
        }

        // Restore original byte
        self.write_memory(address, &[bp.original_byte])?;

        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!("Breakpoint removed 0x{:016x}", address));
        Ok(())
    }

    pub(super) fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let h_process = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;
        unsafe {
            let mut buffer = vec![0u8; size];
            let mut bytes_read = 0;

            let res = ReadProcessMemory(
                h_process,
                address as *const c_void,
                buffer.as_mut_ptr() as *mut c_void,
                size,
                Some(&mut bytes_read),
            );

            res.map_err(|e| {
                FissionError::debug(format!(
                    "ReadProcessMemory failed at 0x{:x}: {:?}",
                    address, e
                ))
            })?;

            buffer.truncate(bytes_read);
            Ok(buffer)
        }
    }

    pub(super) fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        let h_process = self.ensure_process_handle()?;
        unsafe {
            let mut old_protect = PAGE_PROTECTION_FLAGS::default();
            VirtualProtectEx(
                h_process,
                address as *const c_void,
                data.len(),
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            )
            .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;

            // RAII guard restores original protection even if write fails.
            let guard = ProtectGuard::new(h_process, address, data.len(), old_protect);

            let mut bytes_written = 0;
            let res = WriteProcessMemory(
                h_process,
                address as *const c_void,
                data.as_ptr() as *const c_void,
                data.len(),
                Some(&mut bytes_written),
            );

            guard.deactivate();

            // Restore protection (best-effort; failure does not override write error).
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            let _ = VirtualProtectEx(
                h_process,
                address as *const c_void,
                data.len(),
                old_protect,
                &mut _unused,
            );

            res.map_err(|e| {
                FissionError::debug(format!(
                    "WriteProcessMemory failed at 0x{:x}: {:?}",
                    address, e
                ))
            })?;

            if bytes_written != data.len() {
                return Err(FissionError::debug(format!(
                    "Incomplete write at 0x{:x}: {}/{}",
                    address,
                    bytes_written,
                    data.len()
                )));
            }

            Ok(())
        }
    }

    pub(super) fn step_over(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for step over"))?;

        let regs = self.fetch_registers(tid)?;
        let rip = regs.pc;

        let code_bytes = self.read_memory(rip, 16)?;
        let decoder = self
            .decoder
            .as_ref()
            .ok_or_else(|| FissionError::debug("No instruction decoder attached for step over"))?;
        let insn = decoder.decode_one(&code_bytes, rip)?;
        let is_call = insn.is_call;
        let insn_len = insn.length;

        if is_call && insn_len > 0 {
            let next_rip = rip + insn_len as u64;
            let original_byte = self.read_memory(next_rip, 1)?[0];
            if original_byte != 0xCC {
                self.write_memory(next_rip, &[0xCC])?;
                self.state.breakpoints.insert(
                    next_rip,
                    Breakpoint {
                        address: next_rip,
                        original_byte,
                        enabled: true,
                        temporary: true,
                        kind: crate::debug::types::BreakpointKind::Software,
                        hits: 0,
                        condition: None,
                    },
                );
            }
            self.continue_execution()
        } else {
            self.single_step()
        }
    }

    pub(super) fn step_out(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for step out"))?;

        let regs = self.fetch_registers(tid)?;
        // `ESP` in a 32-bit process, `RSP` in a 64-bit one -- the state is
        // named the way its own machine names things.
        let stack_pointer = regs.get("RSP").or_else(|| regs.get("ESP")).unwrap_or(0);
        let ret_addr = if self.is_wow64 == Some(true) {
            let esp = stack_pointer as u32;
            let bytes = self.read_memory(esp as u64, 4)?;
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64
        } else {
            let rsp = stack_pointer;
            let bytes = self.read_memory(rsp, 8)?;
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
        };

        let original_byte = self.read_memory(ret_addr, 1)?[0];
        if original_byte != 0xCC {
            self.write_memory(ret_addr, &[0xCC])?;
            self.state.breakpoints.insert(
                ret_addr,
                Breakpoint {
                    address: ret_addr,
                    original_byte,
                    enabled: true,
                    temporary: true,
                    kind: crate::debug::types::BreakpointKind::Software,
                    hits: 0,
                    condition: None,
                },
            );
        }
        self.continue_execution()
    }

    pub(super) fn skip_instruction(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for skip"))?;

        let mut regs = self.fetch_registers(tid)?;
        let rip = regs.pc;

        let code_bytes = self.read_memory(rip, 16)?;
        let decoder = self
            .decoder
            .as_ref()
            .ok_or_else(|| FissionError::debug("No instruction decoder attached for skip"))?;
        let insn = decoder.decode_one(&code_bytes, rip)?;
        let insn_len = insn.length.max(1);

        regs.pc = if self.is_wow64 == Some(true) {
            (rip as u32).wrapping_add(insn_len as u32) as u64
        } else {
            rip.wrapping_add(insn_len as u64)
        };
        // The program counter is also a named register in the state the
        // backend writes back, so both have to move.
        if regs.get("RIP").is_some() {
            regs.set("RIP", regs.pc);
        } else if regs.get("EIP").is_some() {
            regs.set("EIP", regs.pc);
        }
        self.set_registers(tid, &regs)
    }

    pub(super) fn enable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let bp = self
            .state
            .breakpoints
            .get(&address)
            .cloned()
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        if bp.kind != crate::debug::types::BreakpointKind::Software {
            return Err(FissionError::debug(
                "Only software breakpoints can be enabled by the native Windows backend",
            ));
        }
        if bp.enabled {
            return Ok(false);
        }
        let original_byte = self.read_memory(address, 1)?[0];
        self.write_memory(address, &[0xCC])?;
        let bp = self
            .state
            .breakpoints
            .get_mut(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        bp.original_byte = original_byte;
        bp.enabled = true;
        Ok(true)
    }

    pub(super) fn disable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let bp = self
            .state
            .breakpoints
            .get(&address)
            .cloned()
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        if bp.kind != crate::debug::types::BreakpointKind::Software {
            return Err(FissionError::debug(
                "Only software breakpoints can be disabled by the native Windows backend",
            ));
        }
        if !bp.enabled {
            return Ok(false);
        }
        self.write_memory(address, &[bp.original_byte])?;
        self.state
            .breakpoints
            .get_mut(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?
            .enabled = false;
        Ok(true)
    }

    pub(super) fn list_breakpoints(&self) -> Vec<crate::debug::types::Breakpoint> {
        self.state.breakpoints.values().cloned().collect()
    }
}
