use super::*;
use fission_core::{FissionError, Result as FissionResult};

impl WindowsDebugger {
    /// Move a thread's instruction pointer, keeping the named register and the
    /// state's program counter in step.
    pub(super) fn set_thread_pc(&mut self, thread_id: u32, pc: u64) -> FissionResult<()> {
        let mut registers = self.fetch_registers(thread_id)?;
        registers.pc = pc;
        for name in ["RIP", "EIP"] {
            if registers.get(name).is_some() {
                registers.set(name, pc);
            }
        }
        self.set_registers(thread_id, &registers)
    }

    /// Update the trap flag so the thread stops after one instruction (`enabled = true`)
    /// or clears a pending single-step trap (`enabled = false`).
    fn set_trap_flag_state(&mut self, thread_id: u32, enabled: bool) -> FissionResult<()> {
        let mut registers = self.fetch_registers(thread_id)?;
        let name = if registers.get("RFLAGS").is_some() {
            "RFLAGS"
        } else {
            "EFLAGS"
        };
        let current = registers.get(name).unwrap_or(0x202);
        let flags = if enabled {
            current | 0x100
        } else {
            current & !0x100
        };
        registers.set(name, flags);
        self.set_registers(thread_id, &registers)
    }

    /// Arm the trap flag so the thread stops after one instruction.
    fn set_trap_flag(&mut self, thread_id: u32) -> FissionResult<()> {
        self.set_trap_flag_state(thread_id, true)
    }

    /// Clear the trap flag during rollback.
    fn clear_trap_flag(&mut self, thread_id: u32) -> FissionResult<()> {
        self.set_trap_flag_state(thread_id, false)
    }

    /// If the current thread stands on one of our persistent breakpoints, run
    /// its instruction with the original byte back in place and resume.
    ///
    /// Returns whether it did; the single-step exception that ends the step
    /// re-arms the breakpoint (see `process_debug_event`). Without this a
    /// thread resumed on a breakpoint executes the `INT3` again and stops
    /// where it stood, for ever.
    fn step_off_breakpoint(&mut self, report: bool) -> FissionResult<bool> {
        let Some(thread_id) = self.state.last_thread_id.or(self.state.main_thread_id) else {
            return Ok(false);
        };
        let Ok(registers) = self.fetch_registers(thread_id) else {
            return Ok(false);
        };
        let address = registers.pc;
        let Some(original_byte) = self
            .state
            .breakpoints
            .get(&address)
            .filter(|bp| {
                bp.enabled
                    && !bp.temporary
                    && bp.kind == crate::debug::types::BreakpointKind::Software
            })
            .map(|bp| bp.original_byte)
        else {
            return Ok(false);
        };
        self.write_memory(address, &[original_byte])?;
        if let Err(error) = self.set_trap_flag(thread_id) {
            let _ = self.write_memory(address, &[0xCC]);
            return Err(error);
        }
        self.step_off = Some(super::StepOffBreakpoint {
            thread_id,
            address,
            report,
        });
        if let Err(error) = self.continue_pending_debug_event() {
            self.step_off = None;
            let _ = self.clear_trap_flag(thread_id);
            let _ = self.write_memory(address, &[0xCC]);
            return Err(error);
        }
        self.state.status = DebugStatus::Running;
        Ok(true)
    }

    pub(super) fn continue_execution(&mut self) -> FissionResult<()> {
        if self.step_off_breakpoint(false)? {
            return Ok(());
        }
        self.continue_pending_debug_event()?;
        if self.state.status != DebugStatus::Terminated {
            self.state.status = DebugStatus::Running;
        }
        Ok(())
    }

    pub(super) fn single_step(&mut self) -> FissionResult<()> {
        if self.step_off_breakpoint(true)? {
            return Ok(());
        }
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;
        unsafe {
            let h_thread = super::super::OwnedHandle(
                OpenThread(THREAD_ALL_ACCESS, false, tid)
                    .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?,
            );

            let registers: crate::debug::types::RegisterState;

            if self.is_wow64 == Some(true) {
                let mut ctx: WOW64_CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = WOW64_CONTEXT_ALL;
                Wow64GetThreadContext(h_thread.0, &mut ctx).map_err(|e| {
                    FissionError::debug(format!("Wow64GetThreadContext failed: {:?}", e))
                })?;
                registers = crate::debug::types::RegisterState::at(ctx.Eip as u64)
                    .with("EAX", ctx.Eax as u64)
                    .with("EBX", ctx.Ebx as u64)
                    .with("ECX", ctx.Ecx as u64)
                    .with("EDX", ctx.Edx as u64)
                    .with("ESI", ctx.Esi as u64)
                    .with("EDI", ctx.Edi as u64)
                    .with("EBP", ctx.Ebp as u64)
                    .with("ESP", ctx.Esp as u64)
                    .with("EIP", ctx.Eip as u64)
                    .with("EFLAGS", ctx.EFlags as u64);
                self.record_ttd_snapshot(tid, &registers);
                ctx.EFlags |= 0x100; // Set Trap Flag
                Wow64SetThreadContext(h_thread.0, &ctx).map_err(|e| {
                    FissionError::debug(format!("Wow64SetThreadContext failed: {:?}", e))
                })?;
            } else {
                let mut ctx: CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);
                GetThreadContext(h_thread.0, &mut ctx).map_err(|e| {
                    FissionError::debug(format!("GetThreadContext failed: {:?}", e))
                })?;

                registers = crate::debug::types::RegisterState::at(ctx.Rip)
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
                    .with("RFLAGS", ctx.EFlags as u64);
                self.record_ttd_snapshot(tid, &registers);

                ctx.EFlags |= 0x100; // Set Trap Flag

                SetThreadContext(h_thread.0, &ctx).map_err(|e| {
                    FissionError::debug(format!("SetThreadContext failed: {:?}", e))
                })?;
            }
        }

        // Continue to let the CPU execute one instruction and hit the trap.
        self.continue_pending_debug_event()?;
        self.state.status = DebugStatus::Running;
        Ok(())
    }
}
