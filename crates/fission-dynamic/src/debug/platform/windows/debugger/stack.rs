use super::WindowsDebugger;
use crate::debug::traits::ExecutionBackend;
use fission_core::{FissionError, Result as FissionResult};

impl ExecutionBackend for WindowsDebugger {
    fn stack_peek(&self, offset: isize) -> FissionResult<u64> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for stack peek"))?;
        let regs = self.fetch_registers(tid)?;
        let ptr = if self.is_wow64 == Some(true) {
            (regs.rsp as u32).wrapping_add((offset * 4) as u32) as u64
        } else {
            regs.rsp.wrapping_add((offset * 8) as u64)
        };
        let bytes = self.read_memory(ptr, if self.is_wow64 == Some(true) { 4 } else { 8 })?;
        if self.is_wow64 == Some(true) {
            Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64)
        } else {
            Ok(u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]))
        }
    }

    fn stack_pop(&mut self) -> FissionResult<u64> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for stack pop"))?;
        let mut regs = self.fetch_registers(tid)?;
        let ptr = regs.rsp;
        let bytes = self.read_memory(ptr, if self.is_wow64 == Some(true) { 4 } else { 8 })?;
        let value = if self.is_wow64 == Some(true) {
            regs.rsp = (regs.rsp as u32 + 4) as u64;
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64
        } else {
            regs.rsp += 8;
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ])
        };
        self.set_registers(tid, &regs)?;
        Ok(value)
    }

    fn stack_push(&mut self, value: u64) -> FissionResult<()> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for stack push"))?;
        let mut regs = self.fetch_registers(tid)?;
        if self.is_wow64 == Some(true) {
            regs.rsp = (regs.rsp as u32 - 4) as u64;
            let bytes = (value as u32).to_le_bytes();
            self.write_memory(regs.rsp, &bytes)?;
        } else {
            regs.rsp -= 8;
            let bytes = value.to_le_bytes();
            self.write_memory(regs.rsp, &bytes)?;
        }
        self.set_registers(tid, &regs)
    }
}
