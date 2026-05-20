use super::WindowsDebugger;
use crate::debug::traits::Debugger;
use fission_core::{FissionError, Result as FissionResult};

impl Debugger for WindowsDebugger {
    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
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

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let bp = self
            .state
            .breakpoints
            .get(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;

        // Restore original byte
        self.write_memory(address, &[bp.original_byte])?;

        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!("Breakpoint removed 0x{:016x}", address));
        Ok(())
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
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

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
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

    fn fetch_registers(
        &mut self,
        thread_id: u32,
    ) -> FissionResult<crate::debug::types::RegisterState> {
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            if self.is_wow64 == Some(true) {
                let mut ctx: WOW64_CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = WOW64_CONTEXT_ALL;
                let res = Wow64GetThreadContext(h_thread, &mut ctx);
                let _ = CloseHandle(h_thread);
                res.map_err(|e| {
                    FissionError::debug(format!("Wow64GetThreadContext failed: {:?}", e))
                })?;
                return Ok(crate::debug::types::RegisterState {
                    rax: ctx.Eax as u64,
                    rbx: ctx.Ebx as u64,
                    rcx: ctx.Ecx as u64,
                    rdx: ctx.Edx as u64,
                    rsi: ctx.Esi as u64,
                    rdi: ctx.Edi as u64,
                    rbp: ctx.Ebp as u64,
                    rsp: ctx.Esp as u64,
                    r8: 0,
                    r9: 0,
                    r10: 0,
                    r11: 0,
                    r12: 0,
                    r13: 0,
                    r14: 0,
                    r15: 0,
                    rip: ctx.Eip as u64,
                    rflags: ctx.EFlags as u64,
                });
            }

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            let res = GetThreadContext(h_thread, &mut ctx);
            let _ = CloseHandle(h_thread);

            res.map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            // Map Windows CONTEXT to our RegisterState (x64)
            Ok(crate::debug::types::RegisterState {
                rax: ctx.Rax,
                rbx: ctx.Rbx,
                rcx: ctx.Rcx,
                rdx: ctx.Rdx,
                rsi: ctx.Rsi,
                rdi: ctx.Rdi,
                rbp: ctx.Rbp,
                rsp: ctx.Rsp,
                r8: ctx.R8,
                r9: ctx.R9,
                r10: ctx.R10,
                r11: ctx.R11,
                r12: ctx.R12,
                r13: ctx.R13,
                r14: ctx.R14,
                r15: ctx.R15,
                rip: ctx.Rip,
                rflags: ctx.EFlags as u64,
            })
        }
    }

    /// Write CPU registers to a thread.
    ///
    /// Maps our [`RegisterState`] into a Win32 `CONTEXT` and calls
    /// `SetThreadContext`.  Requires the thread to be suspended.
    fn set_registers(
        &mut self,
        thread_id: u32,
        regs: &crate::debug::types::RegisterState,
    ) -> FissionResult<()> {
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            if self.is_wow64 == Some(true) {
                let mut ctx: WOW64_CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = WOW64_CONTEXT_ALL;
                ctx.Eax = regs.rax as u32;
                ctx.Ebx = regs.rbx as u32;
                ctx.Ecx = regs.rcx as u32;
                ctx.Edx = regs.rdx as u32;
                ctx.Esi = regs.rsi as u32;
                ctx.Edi = regs.rdi as u32;
                ctx.Ebp = regs.rbp as u32;
                ctx.Esp = regs.rsp as u32;
                ctx.Eip = regs.rip as u32;
                ctx.EFlags = regs.rflags as u32;
                let res = Wow64SetThreadContext(h_thread, &ctx);
                let _ = CloseHandle(h_thread);
                return res.map_err(|e| {
                    FissionError::debug(format!("Wow64SetThreadContext failed: {:?}", e))
                });
            }

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            ctx.Rax = regs.rax;
            ctx.Rbx = regs.rbx;
            ctx.Rcx = regs.rcx;
            ctx.Rdx = regs.rdx;
            ctx.Rsi = regs.rsi;
            ctx.Rdi = regs.rdi;
            ctx.Rbp = regs.rbp;
            ctx.Rsp = regs.rsp;
            ctx.R8 = regs.r8;
            ctx.R9 = regs.r9;
            ctx.R10 = regs.r10;
            ctx.R11 = regs.r11;
            ctx.R12 = regs.r12;
            ctx.R13 = regs.r13;
            ctx.R14 = regs.r14;
            ctx.R15 = regs.r15;
            ctx.Rip = regs.rip;
            ctx.EFlags = regs.rflags as u32;

            let res = SetThreadContext(h_thread, &ctx);
            let _ = CloseHandle(h_thread);

            res.map_err(|e| FissionError::debug(format!("SetThreadContext failed: {:?}", e)))
        }
    }

    /// Check whether a `STATUS_SINGLE_STEP` exception was actually a hardware
    /// breakpoint hit by inspecting `Dr6`.
    fn check_hw_breakpoint_hit(&mut self, thread_id: u32) -> Option<u64> {
        if self.hw_breakpoints.is_empty() {
            return None;
        }
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id).ok()?;
            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_DEBUG_REGISTERS);
            if GetThreadContext(h_thread, &mut ctx).is_err() {
                let _ = CloseHandle(h_thread);
                return None;
            }
            let _ = CloseHandle(h_thread);
            let dr6 = ctx.Dr6;
            for i in 0..4u8 {
                if (dr6 & (1u64 << i)) != 0 {
                    return match i {
                        0 => Some(ctx.Dr0),
                        1 => Some(ctx.Dr1),
                        2 => Some(ctx.Dr2),
                        3 => Some(ctx.Dr3),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    /// Set a hardware breakpoint (x86 debug register DR0-DR3).
    ///
    /// Only 4 slots are available.  `kind` maps to DR7 type/length bits.
    fn set_hw_breakpoint(
        &mut self,
        address: u64,
        kind: crate::debug::types::HwBreakpointKind,
    ) -> FissionResult<()> {
        if self.hw_breakpoints.len() >= 4 {
            return Err(FissionError::debug(
                "All 4 hardware breakpoint slots are in use",
            ));
        }
        let used: std::collections::HashSet<u8> =
            self.hw_breakpoints.values().cloned().collect();
        let slot = (0..4u8)
            .find(|i| !used.contains(i))
            .ok_or_else(|| FissionError::debug("No free hardware breakpoint slots"))?;

        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;

        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, tid)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_DEBUG_REGISTERS);
            GetThreadContext(h_thread, &mut ctx)
                .map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            match slot {
                0 => ctx.Dr0 = address,
                1 => ctx.Dr1 = address,
                2 => ctx.Dr2 = address,
                3 => ctx.Dr3 = address,
                _ => unreachable!(),
            }

            let type_len = match kind {
                crate::debug::types::HwBreakpointKind::Execute => 0b0000u64,
                crate::debug::types::HwBreakpointKind::Write => 0b0001u64,
                crate::debug::types::HwBreakpointKind::ReadWrite => 0b0011u64,
            };
            let enable_bit = 1u64 << (slot * 2);
            let shift = 16 + (slot * 4);
            ctx.Dr7 &= !(0b11u64 << (slot * 2));
            ctx.Dr7 &= !(0b1111u64 << shift);
            ctx.Dr7 |= enable_bit;
            ctx.Dr7 |= type_len << shift;

            SetThreadContext(h_thread, &ctx)
                .map_err(|e| FissionError::debug(format!("SetThreadContext failed: {:?}", e)))?;
            let _ = CloseHandle(h_thread);
        }

        self.hw_breakpoints.insert(address, slot);
        self.state.last_event = Some(format!(
            "Hardware breakpoint set at 0x{:016x} (slot {})",
            address, slot
        ));
        Ok(())
    }

    /// Remove a hardware breakpoint previously set with [`set_hw_breakpoint`].
    fn remove_hw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let slot = self
            .hw_breakpoints
            .remove(&address)
            .ok_or_else(|| FissionError::debug("Hardware breakpoint not found"))?;

        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;

        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, tid)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_DEBUG_REGISTERS);
            GetThreadContext(h_thread, &mut ctx)
                .map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            match slot {
                0 => ctx.Dr0 = 0,
                1 => ctx.Dr1 = 0,
                2 => ctx.Dr2 = 0,
                3 => ctx.Dr3 = 0,
                _ => unreachable!(),
            }

            let shift = 16 + (slot * 4);
            ctx.Dr7 &= !(0b11u64 << (slot * 2));
            ctx.Dr7 &= !(0b1111u64 << shift);

            SetThreadContext(h_thread, &ctx)
                .map_err(|e| FissionError::debug(format!("SetThreadContext failed: {:?}", e)))?;
            let _ = CloseHandle(h_thread);
        }

        self.state.last_event = Some(format!(
            "Hardware breakpoint removed at 0x{:016x}",
            address
        ));
        Ok(())
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_path: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let mut cmd_line = path.to_string();
        for a in args {
            cmd_line.push(' ');
            cmd_line.push_str(a);
        }
        let mut wide_cmd: Vec<u16> = OsStr::new(&cmd_line).encode_wide().chain(Some(0)).collect();

        let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

        let creation_flags = windows::Win32::System::Threading::DEBUG_PROCESS
            | windows::Win32::System::Threading::DEBUG_ONLY_THIS_PROCESS;

        unsafe {
            CreateProcessW(
                PWSTR(wide_path.as_ptr() as *mut u16),
                PWSTR(wide_cmd.as_mut_ptr()),
                std::ptr::null(),
                std::ptr::null(),
                false,
                creation_flags,
                std::ptr::null(),
                PWSTR(std::ptr::null_mut()),
                &si,
                &pi,
            )
            .map_err(|e| FissionError::debug(format!("CreateProcessW failed: {:?}", e)))?;
        }

        let pid = pi.dwProcessId;
        self.state.attached_pid = Some(pid);
        self.state.status = DebugStatus::Running;
        self.state.last_event = Some(format!("Launched PID {} ({})", pid, path));

        // Open process handle immediately
        let _ = self.ensure_process_handle();

        // Detect WOW64
        if let Some(h) = self.process_handle {
            let mut process_machine = IMAGE_FILE_MACHINE(0);
            let mut native_machine = IMAGE_FILE_MACHINE(0);
            if unsafe { IsWow64Process2(h, &mut process_machine, &mut native_machine) }.is_ok() {
                self.is_wow64 = Some(process_machine == IMAGE_FILE_MACHINE_I386);
            } else {
                self.is_wow64 = Some(false);
            }
        }

        // Auto-start TTD recording if a timeline is already attached
        self.start_ttd_recording();

        Ok(pid)
    }

    fn step_over(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for step over"))?;

        let regs = self.fetch_registers(tid)?;
        let rip = regs.rip;

        let code_bytes = self.read_memory(rip, 16)?;
        let decoder = self.decoder.as_ref().ok_or_else(|| {
            FissionError::debug("No instruction decoder attached for step over")
        })?;
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

    fn step_out(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for step out"))?;

        let regs = self.fetch_registers(tid)?;
        let ret_addr = if self.is_wow64 == Some(true) {
            let esp = regs.rsp as u32;
            let bytes = self.read_memory(esp as u64, 4)?;
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64
        } else {
            let rsp = regs.rsp;
            let bytes = self.read_memory(rsp, 8)?;
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
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

    fn pause(&mut self) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;
        unsafe {
            DebugBreakProcess(h)
                .map_err(|e| FissionError::debug(format!("DebugBreakProcess failed: {:?}", e)))?;
        }
        self.state.last_event = Some("Break requested".to_string());
        Ok(())
    }

    fn terminate(&mut self) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;
        unsafe {
            TerminateProcess(h, 1)
                .map_err(|e| FissionError::debug(format!("TerminateProcess failed: {:?}", e)))?;
        }
        self.state.status = DebugStatus::Stopped;
        self.state.last_event = Some("Process terminated".to_string());
        Ok(())
    }

    fn skip_instruction(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for skip"))?;

        let mut regs = self.fetch_registers(tid)?;
        let rip = regs.rip;

        let code_bytes = self.read_memory(rip, 16)?;
        let decoder = self.decoder.as_ref().ok_or_else(|| {
            FissionError::debug("No instruction decoder attached for skip")
        })?;
        let insn = decoder.decode_one(&code_bytes, rip)?;
        let insn_len = insn.length.max(1);

        if self.is_wow64 == Some(true) {
            regs.rip = (regs.rip as u32 + insn_len as u32) as u64;
        } else {
            regs.rip += insn_len as u64;
        }
        self.set_registers(tid, &regs)
    }

    fn set_memory_breakpoint(
        &mut self,
        address: u64,
        size: usize,
        kind: crate::debug::types::MemoryBpKind,
    ) -> FissionResult<()> {
        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;

        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        unsafe {
            VirtualQueryEx(
                h,
                address as *const c_void,
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
            .map_err(|e| FissionError::debug(format!("VirtualQueryEx failed: {:?}", e)))?;
        }

        let old_protect = mbi.Protect;
        let new_protect = PAGE_PROTECTION_FLAGS(old_protect.0 | PAGE_GUARD.0);

        unsafe {
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            VirtualProtectEx(
                h,
                address as *const c_void,
                size,
                new_protect,
                &mut _unused,
            )
            .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;
        }

        self.memory_breakpoints.insert(address, (size, old_protect));
        self.state.breakpoints.insert(
            address,
            crate::debug::types::Breakpoint {
                address,
                original_byte: 0,
                enabled: true,
                temporary: false,
                kind: crate::debug::types::BreakpointKind::Memory { size, kind },
                hits: 0,
                condition: None,
            },
        );
        self.state.last_event = Some(format!(
            "Memory breakpoint set at 0x{:016x} (size {}, kind {:?})",
            address, size, kind
        ));
        Ok(())
    }

    fn remove_memory_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let (size, old_protect) = self
            .memory_breakpoints
            .remove(&address)
            .ok_or_else(|| FissionError::debug("Memory breakpoint not found"))?;

        let h = self
            .process_handle
            .ok_or_else(|| FissionError::debug("Process handle not available"))?;

        unsafe {
            let mut _unused = PAGE_PROTECTION_FLAGS::default();
            VirtualProtectEx(
                h,
                address as *const c_void,
                size,
                old_protect,
                &mut _unused,
            )
            .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;
        }

        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!(
            "Memory breakpoint removed at 0x{:016x}",
            address
        ));
        Ok(())
    }

    fn set_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        self.state.breakpoints.insert(
            0,
            crate::debug::types::Breakpoint {
                address: 0,
                original_byte: 0,
                enabled: true,
                temporary: false,
                kind: crate::debug::types::BreakpointKind::Dll {
                    name: dll_name.to_string(),
                },
                hits: 0,
                condition: None,
            },
        );
        self.state.last_event = Some(format!(
            "DLL breakpoint set for '{}'",
            dll_name
        ));
        Ok(())
    }

    fn remove_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        let keys_to_remove: Vec<u64> = self
            .state
            .breakpoints
            .iter()
            .filter(|(_, bp)| {
                matches!(&bp.kind, crate::debug::types::BreakpointKind::Dll { name } if name == dll_name)
            })
            .map(|(addr, _)| *addr)
            .collect();

        if keys_to_remove.is_empty() {
            return Err(FissionError::debug("DLL breakpoint not found"));
        }

        for addr in keys_to_remove {
            self.state.breakpoints.remove(&addr);
        }
        self.state.last_event = Some(format!(
            "DLL breakpoint removed for '{}'",
            dll_name
        ));
        Ok(())
    }

    fn set_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let key = code as u64;
        self.state.breakpoints.insert(
            key,
            crate::debug::types::Breakpoint {
                address: key,
                original_byte: 0,
                enabled: true,
                temporary: false,
                kind: crate::debug::types::BreakpointKind::Exception { code },
                hits: 0,
                condition: None,
            },
        );
        self.state.last_event = Some(format!(
            "Exception breakpoint set for code 0x{:08x}",
            code
        ));
        Ok(())
    }

    fn remove_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let key = code as u64;
        if self.state.breakpoints.remove(&key).is_none() {
            return Err(FissionError::debug("Exception breakpoint not found"));
        }
        self.state.last_event = Some(format!(
            "Exception breakpoint removed for code 0x{:08x}",
            code
        ));
        Ok(())
    }

    fn enable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let bp = self
            .state
            .breakpoints
            .get_mut(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        if bp.enabled {
            return Ok(false);
        }
        match &bp.kind {
            crate::debug::types::BreakpointKind::Software => {
                let original_byte = self.read_memory(address, 1)?[0];
                bp.original_byte = original_byte;
                self.write_memory(address, &[0xCC])?;
            }
            crate::debug::types::BreakpointKind::Hardware(_) => {
                // Re-enable via DR7 handled by caller if needed
            }
            crate::debug::types::BreakpointKind::Memory { size, kind } => {
                let h = self
                    .process_handle
                    .ok_or_else(|| FissionError::debug("Process handle not available"))?;
                let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
                unsafe {
                    VirtualQueryEx(
                        h,
                        address as *const c_void,
                        &mut mbi,
                        std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                    )
                    .map_err(|e| FissionError::debug(format!("VirtualQueryEx failed: {:?}", e)))?;
                }
                let old_protect = mbi.Protect;
                let new_protect = PAGE_PROTECTION_FLAGS(old_protect.0 | PAGE_GUARD.0);
                unsafe {
                    let mut _unused = PAGE_PROTECTION_FLAGS::default();
                    VirtualProtectEx(
                        h,
                        address as *const c_void,
                        *size,
                        new_protect,
                        &mut _unused,
                    )
                    .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;
                }
                self.memory_breakpoints.insert(address, (*size, old_protect));
            }
            _ => {}
        }
        bp.enabled = true;
        Ok(true)
    }

    fn disable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let bp = self
            .state
            .breakpoints
            .get_mut(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;
        if !bp.enabled {
            return Ok(false);
        }
        match &bp.kind {
            crate::debug::types::BreakpointKind::Software => {
                self.write_memory(address, &[bp.original_byte])?;
            }
            crate::debug::types::BreakpointKind::Hardware(_) => {
                // Disable via DR7 handled by caller if needed
            }
            crate::debug::types::BreakpointKind::Memory { size, .. } => {
                let (stored_size, old_protect) = self
                    .memory_breakpoints
                    .remove(&address)
                    .ok_or_else(|| FissionError::debug("Memory breakpoint state not found"))?;
                let h = self
                    .process_handle
                    .ok_or_else(|| FissionError::debug("Process handle not available"))?;
                unsafe {
                    let mut _unused = PAGE_PROTECTION_FLAGS::default();
                    VirtualProtectEx(
                        h,
                        address as *const c_void,
                        stored_size,
                        old_protect,
                        &mut _unused,
                    )
                    .map_err(|e| FissionError::debug(format!("VirtualProtectEx failed: {:?}", e)))?;
                }
                let _ = self.memory_breakpoints.remove(&address);
            }
            _ => {}
        }
        bp.enabled = false;
        Ok(true)
    }

    fn list_breakpoints(&self) -> Vec<crate::debug::types::Breakpoint> {
        self.state.breakpoints.values().cloned().collect()
    }
}
