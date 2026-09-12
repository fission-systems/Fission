use super::WindowsDebugger;
use crate::debug::traits::ExecutionBackend;
use fission_core::{FissionError, Result as FissionResult};

impl ExecutionBackend for WindowsDebugger {
    fn continue_execution(&mut self) -> FissionResult<()> {
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;

        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| FissionError::debug(format!("Continue failed: {:?}", e)))?;
        }
        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn single_step(&mut self) -> FissionResult<()> {
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;
        unsafe {
            let h_thread = OpenThread(THREAD_ALL_ACCESS, false, tid)
                .map_err(|e| FissionError::debug(format!("OpenThread failed: {:?}", e)))?;

            let mut registers: crate::debug::types::RegisterState;

            if self.is_wow64 == Some(true) {
                let mut ctx: WOW64_CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = WOW64_CONTEXT_ALL;
                Wow64GetThreadContext(h_thread, &mut ctx).map_err(|e| {
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
                Wow64SetThreadContext(h_thread, &ctx).map_err(|e| {
                    FissionError::debug(format!("Wow64SetThreadContext failed: {:?}", e))
                })?;
            } else {
                let mut ctx: CONTEXT = std::mem::zeroed();
                ctx.ContextFlags = CONTEXT_FLAGS(CONTEXT_ALL);
                GetThreadContext(h_thread, &mut ctx).map_err(|e| {
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

                SetThreadContext(h_thread, &ctx).map_err(|e| {
                    FissionError::debug(format!("SetThreadContext failed: {:?}", e))
                })?;
            }

            let _ = CloseHandle(h_thread);
        }

        // Continue to let the CPU execute one instruction and hit the trap
        let pid = self
            .state
            .attached_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        let tid = self
            .state
            .last_thread_id
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id"))?;
        unsafe {
            ContinueDebugEvent(pid, tid, DBG_CONTINUE)
                .map_err(|e| FissionError::debug(format!("Continue for step failed: {:?}", e)))?;
        }

        self.state.status = DebugStatus::Running;
        Ok(())
    }
}
