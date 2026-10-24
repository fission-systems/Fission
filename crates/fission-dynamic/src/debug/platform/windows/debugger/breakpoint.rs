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
        #[cfg(test)]
        if self.process_handle.is_none() {
            if let Some(mem) = &self.test_memory {
                let mut buffer = Vec::with_capacity(size);
                for off in 0..size {
                    let addr = address.wrapping_add(off as u64);
                    if let Some(&b) = mem.get(&addr) {
                        buffer.push(b);
                    } else {
                        return Err(FissionError::debug(format!(
                            "Unmapped test address 0x{:x}",
                            addr
                        )));
                    }
                }
                return Ok(buffer);
            }
        }

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
        #[cfg(test)]
        if self.process_handle.is_none() {
            if let Some(mem) = &mut self.test_memory {
                for (i, &b) in data.iter().enumerate() {
                    mem.insert(address.wrapping_add(i as u64), b);
                }
                return Ok(());
            }
        }

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

        // If the instruction pointer is situated at a return instruction,
        // single-stepping executes the return and steps out cleanly.
        if let Ok(code_bytes) = self.read_memory(regs.pc, 16) {
            let is_ret = if let Some(decoder) = &self.decoder {
                decoder
                    .decode_one(&code_bytes, regs.pc)
                    .map(|insn| insn.is_return)
                    .unwrap_or(false)
            } else {
                matches!(code_bytes.first(), Some(0xc3 | 0xcb | 0xc2 | 0xca))
            };
            if is_ret {
                return self.single_step();
            }
        }

        let ret_addr = self.find_return_address(tid, &regs)?;

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

    /// Resolve the return address of the current function invocation
    /// using stack frame pointers or x64 RUNTIME_FUNCTION unwind info.
    pub(super) fn find_return_address(
        &self,
        _tid: u32,
        regs: &crate::debug::types::RegisterState,
    ) -> FissionResult<u64> {
        let stack_pointer = regs.get("RSP").or_else(|| regs.get("ESP")).unwrap_or(0);
        if stack_pointer == 0 {
            return Err(FissionError::debug("Invalid stack pointer"));
        }

        if self.is_wow64 == Some(true) {
            // In 32-bit x86, examine EBP-based frame pointer if valid
            let esp = stack_pointer as u32;
            let ebp = regs.get("EBP").unwrap_or(0) as u32;
            if ebp > esp && ebp.saturating_sub(esp) < 0x100000 {
                if let Ok(bytes) = self.read_memory((ebp as u64).wrapping_add(4), 4) {
                    let candidate =
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
                    if candidate > 0x10000 && self.read_memory(candidate, 1).is_ok() {
                        return Ok(candidate);
                    }
                }
            }
            // Fallback for leaf functions or omitted frame pointer
            let bytes = self.read_memory(esp as u64, 4)?;
            let ret = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
            return Ok(ret);
        }

        // In 64-bit x86_64, inspect RUNTIME_FUNCTION unwind info if available
        let rsp = stack_pointer;
        let rbp = regs.get("RBP").unwrap_or(0);
        if let Some(ret_addr) = self.find_x64_unwind_return_address(regs.pc, rsp, rbp) {
            if ret_addr != 0 && self.read_memory(ret_addr, 1).is_ok() {
                return Ok(ret_addr);
            }
        }

        // If RBP points above RSP, check standard [RBP + 8] frame return address
        if rbp > rsp && rbp.saturating_sub(rsp) < 0x100000 {
            if let Ok(bytes) = self.read_memory(rbp.wrapping_add(8), 8) {
                let candidate = u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]);
                if candidate > 0x10000 && self.read_memory(candidate, 1).is_ok() {
                    return Ok(candidate);
                }
            }
        }

        // Fallback: read directly from [RSP]
        let bytes = self.read_memory(rsp, 8)?;
        let ret = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(ret)
    }

    /// Walk x64 UNWIND_INFO in the module enclosing `pc` to calculate the frame size
    /// and read the saved return address slot.
    fn find_x64_unwind_return_address(&self, pc: u64, rsp: u64, rbp: u64) -> Option<u64> {
        let module_base = self.state.modules.values().find_map(|m| {
            if pc >= m.base_address && (m.size == 0 || pc < m.base_address + m.size as u64) {
                Some(m.base_address)
            } else {
                None
            }
        })?;

        let dos = self.read_memory(module_base, 64).ok()?;
        let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
        let pe_sig = self.read_memory(module_base + e_lfanew, 4).ok()?;
        if &pe_sig != b"PE\0\0" {
            return None;
        }

        let opt_offset = module_base + e_lfanew + 24;
        let dd_exception_offset = opt_offset + 112 + 3 * 8;
        let pdata_dd = self.read_memory(dd_exception_offset, 8).ok()?;
        let pdata_rva = u32::from_le_bytes([pdata_dd[0], pdata_dd[1], pdata_dd[2], pdata_dd[3]]);
        let pdata_size = u32::from_le_bytes([pdata_dd[4], pdata_dd[5], pdata_dd[6], pdata_dd[7]]);
        if pdata_rva == 0 || pdata_size < 12 {
            return None;
        }

        let pc_rva = u32::try_from(pc.checked_sub(module_base)?).ok()?;
        let num_entries = (pdata_size / 12) as usize;
        let mut low = 0usize;
        let mut high = num_entries;
        let mut found_rf = None;

        while low < high {
            let mid = (low + high) / 2;
            let entry_addr = module_base + pdata_rva as u64 + mid as u64 * 12;
            let entry_bytes = self.read_memory(entry_addr, 12).ok()?;
            let begin_rva = u32::from_le_bytes([
                entry_bytes[0],
                entry_bytes[1],
                entry_bytes[2],
                entry_bytes[3],
            ]);
            let end_rva = u32::from_le_bytes([
                entry_bytes[4],
                entry_bytes[5],
                entry_bytes[6],
                entry_bytes[7],
            ]);
            let unwind_info_rva = u32::from_le_bytes([
                entry_bytes[8],
                entry_bytes[9],
                entry_bytes[10],
                entry_bytes[11],
            ]);

            if pc_rva < begin_rva {
                high = mid;
            } else if pc_rva >= end_rva {
                low = mid + 1;
            } else {
                found_rf = Some((begin_rva, end_rva, unwind_info_rva));
                break;
            }
        }

        let (begin_rva, _, mut unwind_info_rva) = found_rf?;

        // Follow chained unwind info if present (UNW_FLAG_CHAININFO = 0x4)
        for _ in 0..8 {
            let header = self
                .read_memory(module_base + unwind_info_rva as u64, 4)
                .ok()?;
            let flags = header[0] >> 3;
            let count_of_codes = header[2] as usize;
            if (flags & 0x4) != 0 {
                let codes_bytes = count_of_codes * 2 + (count_of_codes % 2) * 2;
                let chained_addr = module_base + unwind_info_rva as u64 + 4 + codes_bytes as u64;
                let chained_rf = self.read_memory(chained_addr, 12).ok()?;
                unwind_info_rva = u32::from_le_bytes([
                    chained_rf[8],
                    chained_rf[9],
                    chained_rf[10],
                    chained_rf[11],
                ]);
            } else {
                break;
            }
        }

        let header = self
            .read_memory(module_base + unwind_info_rva as u64, 4)
            .ok()?;
        let prolog_size = header[1];
        let count_of_codes = header[2] as usize;
        let frame_reg_and_offset = header[3];

        let codes_len = count_of_codes * 2;
        let codes = self
            .read_memory(module_base + unwind_info_rva as u64 + 4, codes_len)
            .ok()?;

        let mut stack_offset = 0u64;
        let mut code_idx = 0;
        let offset_in_prolog = pc_rva.saturating_sub(begin_rva);
        let in_prolog = offset_in_prolog < prolog_size as u32;

        while code_idx < count_of_codes {
            let code_offset = codes[code_idx * 2];
            let op_and_info = codes[code_idx * 2 + 1];
            let op = op_and_info & 0x0F;
            let op_info = (op_and_info >> 4) as usize;

            let executed = !in_prolog || offset_in_prolog >= code_offset as u32;

            match op {
                0 => {
                    // UWOP_PUSH_NONVOL
                    if executed {
                        stack_offset += 8;
                    }
                    code_idx += 1;
                }
                1 => {
                    // UWOP_ALLOC_LARGE
                    if op_info == 0 {
                        if code_idx + 1 < count_of_codes {
                            let sz = u16::from_le_bytes([
                                codes[(code_idx + 1) * 2],
                                codes[(code_idx + 1) * 2 + 1],
                            ]) as u64
                                * 8;
                            if executed {
                                stack_offset += sz;
                            }
                        }
                        code_idx += 2;
                    } else {
                        if code_idx + 2 < count_of_codes {
                            let b0 = codes[(code_idx + 1) * 2];
                            let b1 = codes[(code_idx + 1) * 2 + 1];
                            let b2 = codes[(code_idx + 2) * 2];
                            let b3 = codes[(code_idx + 2) * 2 + 1];
                            let sz = u32::from_le_bytes([b0, b1, b2, b3]) as u64;
                            if executed {
                                stack_offset += sz;
                            }
                        }
                        code_idx += 3;
                    }
                }
                2 => {
                    // UWOP_ALLOC_SMALL
                    let sz = (op_info as u64 * 8) + 8;
                    if executed {
                        stack_offset += sz;
                    }
                    code_idx += 1;
                }
                3 => {
                    // UWOP_SET_FPREG
                    code_idx += 1;
                }
                4 | 8 => {
                    // UWOP_SAVE_NONVOL / UWOP_SAVE_XMM128
                    code_idx += 2;
                }
                5 | 9 => {
                    // UWOP_SAVE_NONVOL_FAR / UWOP_SAVE_XMM128_FAR
                    code_idx += 3;
                }
                10 => {
                    // UWOP_PUSH_MACHFRAME
                    if executed {
                        stack_offset += if op_info == 0 { 40 } else { 48 };
                    }
                    code_idx += 1;
                }
                _ => {
                    code_idx += 1;
                }
            }
        }

        let frame_reg = frame_reg_and_offset & 0x0F;
        let frame_offset_scaled = ((frame_reg_and_offset >> 4) as u64) * 16;

        let ret_slot = if frame_reg != 0 && !in_prolog && rbp != 0 {
            rbp.wrapping_sub(frame_offset_scaled)
                .wrapping_add(stack_offset)
        } else {
            rsp.wrapping_add(stack_offset)
        };

        let bytes = self.read_memory(ret_slot, 8).ok()?;
        Some(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
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
