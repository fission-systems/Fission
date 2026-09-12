use super::WindowsDebugger;
use crate::debug::traits::ExecutionBackend;
use fission_core::{FissionError, Result as FissionResult};

impl ExecutionBackend for WindowsDebugger {
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
                // A 32-bit process, named in 32-bit registers. `R8`..`R15`
                // used to be reported here as eight zeroes; a register the
                // process does not have is now simply absent.
                return Ok(crate::debug::types::RegisterState::at(ctx.Eip as u64)
                    .with("EAX", ctx.Eax as u64)
                    .with("EBX", ctx.Ebx as u64)
                    .with("ECX", ctx.Ecx as u64)
                    .with("EDX", ctx.Edx as u64)
                    .with("ESI", ctx.Esi as u64)
                    .with("EDI", ctx.Edi as u64)
                    .with("EBP", ctx.Ebp as u64)
                    .with("ESP", ctx.Esp as u64)
                    .with("EIP", ctx.Eip as u64)
                    .with("EFLAGS", ctx.EFlags as u64));
            }

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            let res = GetThreadContext(h_thread, &mut ctx);
            let _ = CloseHandle(h_thread);

            res.map_err(|e| FissionError::debug(format!("GetThreadContext failed: {:?}", e)))?;

            // Map Windows CONTEXT to our RegisterState (x64)
            Ok(crate::debug::types::RegisterState::at(ctx.Rip)
                .with("RAX", ctx.Rax)
                .with("RBX", ctx.Rbx)
                .with("RCX", ctx.Rcx)
                .with("RDX", ctx.Rdx)
                .with("RSI", ctx.Rsi)
                .with("RDI", ctx.Rdi)
                .with("RBP", ctx.Rbp)
                .with("RSP", ctx.Rsp)
                .with("R8", ctx.R8)
                .with("R9", ctx.R9)
                .with("R10", ctx.R10)
                .with("R11", ctx.R11)
                .with("R12", ctx.R12)
                .with("R13", ctx.R13)
                .with("R14", ctx.R14)
                .with("R15", ctx.R15)
                .with("RIP", ctx.Rip)
                .with("RFLAGS", ctx.EFlags as u64))
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
                // Either naming: `EAX` from this backend's own read-back,
                // `RAX` from a state recorded on an x86-64 machine.
                let get = |wide: &str, narrow: &str| regs.get(narrow).or_else(|| regs.get(wide));
                ctx.Eax = get("RAX", "EAX").unwrap_or(0) as u32;
                ctx.Ebx = get("RBX", "EBX").unwrap_or(0) as u32;
                ctx.Ecx = get("RCX", "ECX").unwrap_or(0) as u32;
                ctx.Edx = get("RDX", "EDX").unwrap_or(0) as u32;
                ctx.Esi = get("RSI", "ESI").unwrap_or(0) as u32;
                ctx.Edi = get("RDI", "EDI").unwrap_or(0) as u32;
                ctx.Ebp = get("RBP", "EBP").unwrap_or(0) as u32;
                ctx.Esp = get("RSP", "ESP").unwrap_or(0) as u32;
                ctx.Eip = regs.pc as u32;
                ctx.EFlags = regs.get("EFLAGS").unwrap_or(0) as u32;
                let res = Wow64SetThreadContext(h_thread, &ctx);
                let _ = CloseHandle(h_thread);
                return res.map_err(|e| {
                    FissionError::debug(format!("Wow64SetThreadContext failed: {:?}", e))
                });
            }

            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);

            ctx.Rax = regs.get("RAX").unwrap_or(0);
            ctx.Rbx = regs.get("RBX").unwrap_or(0);
            ctx.Rcx = regs.get("RCX").unwrap_or(0);
            ctx.Rdx = regs.get("RDX").unwrap_or(0);
            ctx.Rsi = regs.get("RSI").unwrap_or(0);
            ctx.Rdi = regs.get("RDI").unwrap_or(0);
            ctx.Rbp = regs.get("RBP").unwrap_or(0);
            ctx.Rsp = regs.get("RSP").unwrap_or(0);
            ctx.R8 = regs.get("R8").unwrap_or(0);
            ctx.R9 = regs.get("R9").unwrap_or(0);
            ctx.R10 = regs.get("R10").unwrap_or(0);
            ctx.R11 = regs.get("R11").unwrap_or(0);
            ctx.R12 = regs.get("R12").unwrap_or(0);
            ctx.R13 = regs.get("R13").unwrap_or(0);
            ctx.R14 = regs.get("R14").unwrap_or(0);
            ctx.R15 = regs.get("R15").unwrap_or(0);
            ctx.Rip = regs.pc;
            ctx.EFlags = regs.get("RFLAGS").unwrap_or(0) as u32;

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
        let used: std::collections::HashSet<u8> = self.hw_breakpoints.values().cloned().collect();
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

        self.state.last_event = Some(format!("Hardware breakpoint removed at 0x{:016x}", address));
        Ok(())
    }
}
