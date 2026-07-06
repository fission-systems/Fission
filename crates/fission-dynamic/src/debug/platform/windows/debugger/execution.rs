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
                registers = crate::debug::types::RegisterState {
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
                };
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

                registers = crate::debug::types::RegisterState {
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
                };
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
