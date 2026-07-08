use std::collections::HashMap;
use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::HleResult;

/// A single emulated procedure (e.g. a libc function or a syscall).
pub trait SimProcedure: Send + Sync {
    /// Executes the procedure logic.
    fn run(&self, emu: &mut Emulator) -> Result<HleResult>;
}

/// A registry of SimProcedures for a specific operating system.
pub struct SimOS {
    pub procedures: HashMap<String, Box<dyn SimProcedure>>,
    pub syscalls: HashMap<u64, Box<dyn SimProcedure>>,
}

impl SimOS {
    pub fn new() -> Self {
        Self {
            procedures: HashMap::new(),
            syscalls: HashMap::new(),
        }
    }

    pub fn register_procedure(&mut self, name: impl Into<String>, proc: Box<dyn SimProcedure>) {
        self.procedures.insert(name.into(), proc);
    }

    pub fn register_syscall(&mut self, num: u64, proc: Box<dyn SimProcedure>) {
        self.syscalls.insert(num, proc);
    }
}
