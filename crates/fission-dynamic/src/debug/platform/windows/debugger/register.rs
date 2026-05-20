use super::WindowsDebugger;
use crate::debug::traits::Debugger;
use fission_core::{FissionError, Result as FissionResult};

impl Debugger for WindowsDebugger {
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
