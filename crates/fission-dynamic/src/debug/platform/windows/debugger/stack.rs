use super::WindowsDebugger;
use crate::debug::traits::ExecutionBackend;
use crate::debug::types::RegisterState;
use fission_core::{FissionError, Result as FissionResult};

/// The stack pointer, under whichever name this process's context uses:
/// `RSP` for a 64-bit target, `ESP` for a WOW64 one.
fn stack_pointer(regs: &RegisterState) -> u64 {
    regs.get("RSP").or_else(|| regs.get("ESP")).unwrap_or(0)
}

fn set_stack_pointer(regs: &mut RegisterState, value: u64) {
    if regs.get("RSP").is_some() {
        regs.set("RSP", value);
    } else {
        regs.set("ESP", value);
    }
}

impl ExecutionBackend for WindowsDebugger {
    fn stack_peek(&self, offset: isize) -> FissionResult<u64> {
        let tid = self
            .state
            .current_thread_id
            .or(self.state.last_thread_id)
            .or(self.state.main_thread_id)
            .ok_or_else(|| FissionError::debug("No thread id for stack peek"))?;
        let regs = self.fetch_registers(tid)?;
        let sp = stack_pointer(&regs);
        let ptr = if self.is_wow64 == Some(true) {
            (sp as u32).wrapping_add((offset * 4) as u32) as u64
        } else {
            sp.wrapping_add((offset * 8) as u64)
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
        let ptr = stack_pointer(&regs);
        let bytes = self.read_memory(ptr, if self.is_wow64 == Some(true) { 4 } else { 8 })?;
        let value = if self.is_wow64 == Some(true) {
            set_stack_pointer(&mut regs, (ptr as u32).wrapping_add(4) as u64);
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64
        } else {
            set_stack_pointer(&mut regs, ptr.wrapping_add(8));
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
        let sp = stack_pointer(&regs);
        if self.is_wow64 == Some(true) {
            let sp = (sp as u32).wrapping_sub(4) as u64;
            set_stack_pointer(&mut regs, sp);
            let bytes = (value as u32).to_le_bytes();
            self.write_memory(sp, &bytes)?;
        } else {
            let sp = sp.wrapping_sub(8);
            set_stack_pointer(&mut regs, sp);
            let bytes = value.to_le_bytes();
            self.write_memory(sp, &bytes)?;
        }
        self.set_registers(tid, &regs)
    }
}
