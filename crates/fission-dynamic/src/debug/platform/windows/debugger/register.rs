use super::*;
use fission_core::{FissionError, Result as FissionResult};

impl WindowsDebugger {
    pub(super) fn fetch_registers(
        &self,
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
    pub(super) fn set_registers(
        &mut self,
        thread_id: u32,
        regs: &crate::debug::types::RegisterState,
    ) -> FissionResult<()> {
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, thread_id)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let result: FissionResult<()> = if self.is_wow64 == Some(true) {
                let mut ctx: WOW64_CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = WOW64_CONTEXT_ALL;
                match Wow64GetThreadContext(h_thread, &mut ctx) {
                    Ok(()) => {
                        // Either naming: `EAX` from this backend's own read-back,
                        // `RAX` from a state recorded on an x86-64 machine. Read
                        // first so registers this API does not expose survive a
                        // partial RegisterState write.
                        let get =
                            |wide: &str, narrow: &str| regs.get(narrow).or_else(|| regs.get(wide));
                        if let Some(value) = get("RAX", "EAX") {
                            ctx.Eax = value as u32;
                        }
                        if let Some(value) = get("RBX", "EBX") {
                            ctx.Ebx = value as u32;
                        }
                        if let Some(value) = get("RCX", "ECX") {
                            ctx.Ecx = value as u32;
                        }
                        if let Some(value) = get("RDX", "EDX") {
                            ctx.Edx = value as u32;
                        }
                        if let Some(value) = get("RSI", "ESI") {
                            ctx.Esi = value as u32;
                        }
                        if let Some(value) = get("RDI", "EDI") {
                            ctx.Edi = value as u32;
                        }
                        if let Some(value) = get("RBP", "EBP") {
                            ctx.Ebp = value as u32;
                        }
                        if let Some(value) = get("RSP", "ESP") {
                            ctx.Esp = value as u32;
                        }
                        ctx.Eip = regs.pc as u32;
                        if let Some(value) = regs.get("EFLAGS").or_else(|| regs.get("RFLAGS")) {
                            ctx.EFlags = value as u32;
                        }
                        Wow64SetThreadContext(h_thread, &ctx).map_err(|e| {
                            FissionError::debug(format!("Wow64SetThreadContext failed: {:?}", e))
                        })
                    }
                    Err(error) => Err(FissionError::debug(format!(
                        "Wow64GetThreadContext failed: {:?}",
                        error
                    ))),
                }
            } else {
                let mut ctx: CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);
                match GetThreadContext(h_thread, &mut ctx) {
                    Ok(()) => {
                        // The portable RegisterState intentionally omits FP,
                        // vector, segment, and debug state. Preserve those
                        // context fields rather than writing zeroes for them.
                        if let Some(value) = regs.get("RAX") {
                            ctx.Rax = value;
                        }
                        if let Some(value) = regs.get("RBX") {
                            ctx.Rbx = value;
                        }
                        if let Some(value) = regs.get("RCX") {
                            ctx.Rcx = value;
                        }
                        if let Some(value) = regs.get("RDX") {
                            ctx.Rdx = value;
                        }
                        if let Some(value) = regs.get("RSI") {
                            ctx.Rsi = value;
                        }
                        if let Some(value) = regs.get("RDI") {
                            ctx.Rdi = value;
                        }
                        if let Some(value) = regs.get("RBP") {
                            ctx.Rbp = value;
                        }
                        if let Some(value) = regs.get("RSP") {
                            ctx.Rsp = value;
                        }
                        if let Some(value) = regs.get("R8") {
                            ctx.R8 = value;
                        }
                        if let Some(value) = regs.get("R9") {
                            ctx.R9 = value;
                        }
                        if let Some(value) = regs.get("R10") {
                            ctx.R10 = value;
                        }
                        if let Some(value) = regs.get("R11") {
                            ctx.R11 = value;
                        }
                        if let Some(value) = regs.get("R12") {
                            ctx.R12 = value;
                        }
                        if let Some(value) = regs.get("R13") {
                            ctx.R13 = value;
                        }
                        if let Some(value) = regs.get("R14") {
                            ctx.R14 = value;
                        }
                        if let Some(value) = regs.get("R15") {
                            ctx.R15 = value;
                        }
                        ctx.Rip = regs.pc;
                        if let Some(value) = regs.get("RFLAGS") {
                            ctx.EFlags = value as u32;
                        }
                        SetThreadContext(h_thread, &ctx).map_err(|e| {
                            FissionError::debug(format!("SetThreadContext failed: {:?}", e))
                        })
                    }
                    Err(error) => Err(FissionError::debug(format!(
                        "GetThreadContext failed: {:?}",
                        error
                    ))),
                }
            };
            let _ = CloseHandle(h_thread);
            result
        }
    }

    /// Check whether a `STATUS_SINGLE_STEP` exception was actually a hardware
    /// breakpoint hit by inspecting `Dr6`.
    pub(super) fn check_hw_breakpoint_hit(&mut self, thread_id: u32) -> Option<u64> {
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
    // Kept internal until the shared debugger interface can expose hardware
    // breakpoint lifecycle operations; the CLI currently rejects them.
    #[allow(dead_code)]
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
    // See `set_hw_breakpoint`: this is not reachable through ExecutionBackend.
    #[allow(dead_code)]
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
