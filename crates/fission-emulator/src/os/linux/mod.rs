pub mod loader;
pub mod libc;
pub mod syscall;
pub mod abi;

use anyhow::Result;
use crate::core::Emulator;
use crate::pcode::state::MachineState;
use crate::os::env::{HleResult, OsEnvironment};
use crate::os::procedure::SimOS;
use fission_loader::loader::LoadedBinary;

const MAGIC_BASE: u64 = 0xFFFFFFF100000000;

/// Linux ELF execution environment.
///
/// - Import patching: overwrites GOT slots for PLT-reachable symbols.
/// - HLE dispatch: emulates libc functions and syscalls using SimProcedures.
pub struct LinuxEnv {
    pub simos: SimOS,
}

impl LinuxEnv {
    pub fn new() -> Self {
        let mut simos = SimOS::new();
        
        // Register Libc Procedures
        simos.register_procedure("malloc", Box::new(libc::Malloc));
        simos.register_procedure("free", Box::new(libc::Free));
        simos.register_procedure("puts", Box::new(libc::Puts));
        simos.register_procedure("printf", Box::new(libc::Printf));
        simos.register_procedure("read", Box::new(libc::Read));
        simos.register_procedure("write", Box::new(libc::Write));
        simos.register_procedure("exit", Box::new(libc::Exit));
        simos.register_procedure("_exit", Box::new(libc::Exit));

        // Register Syscalls
        simos.register_syscall(0, Box::new(syscall::SysRead));
        simos.register_syscall(1, Box::new(syscall::SysWrite));
        simos.register_syscall(2, Box::new(syscall::SysOpen));
        simos.register_syscall(3, Box::new(syscall::SysClose));
        simos.register_syscall(5, Box::new(syscall::SysFstat));
        simos.register_syscall(9, Box::new(syscall::SysMmap));
        simos.register_syscall(12, Box::new(syscall::SysBrk));
        simos.register_syscall(60, Box::new(syscall::SysExit));
        simos.register_syscall(231, Box::new(syscall::SysExit)); // exit_group

        Self { simos }
    }
}

impl Default for LinuxEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl OsEnvironment for LinuxEnv {
    fn patch_imports(&self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
        if binary.format != "ELF" {
            return Ok(());
        }
        let mut plt_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        plt_entries.sort_by_key(|&(&addr, _)| addr);
        for (i, (&addr, name)) in plt_entries.into_iter().enumerate() {
            let trampoline = MAGIC_BASE + (i as u64 * 8);
            tracing::debug!("PLT/GOT patch: {} @ 0x{:X} → trampoline 0x{:X}", name, addr, trampoline);
            state.write_space(3, addr, &trampoline.to_le_bytes())?;
        }
        Ok(())
    }

    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        let index = ((magic_addr - MAGIC_BASE) / 8) as usize;
        let mut plt_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        plt_entries.sort_by_key(|&(&addr, _)| addr);
        plt_entries
            .into_iter()
            .nth(index)
            .map(|(_, name)| name.split('@').next().unwrap_or(name).to_string())
    }

    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult> {
        if func_name == "syscall" {
            let sys_num = emu.read_register_u64("RAX").unwrap_or(0);
            if let Some(proc) = self.simos.syscalls.get(&sys_num) {
                return proc.run(emu);
            } else {
                tracing::warn!("Unimplemented Linux x64 syscall: {}", sys_num);
                emu.write_register_u64("RAX", 0)?;
                return Ok(HleResult::Continue);
            }
        }

        if let Some(proc) = self.simos.procedures.get(func_name) {
            proc.run(emu)
        } else {
            tracing::warn!("Unimplemented libc function or procedure: {}. Returning 0.", func_name);
            emu.write_return_val(0)?;
            Ok(HleResult::Continue)
        }
    }

    fn dispatch_userop(
        &self,
        _emu: &mut Emulator,
        userop_name: &str,
        inputs: &[u64],
        _output_size: u32,
    ) -> Result<HleResult> {
        match userop_name {
            "segment_fs" | "segment_gs" => {
                let offset = inputs.get(0).copied().unwrap_or(0);
                tracing::debug!("Linux HLE: {} (offset=0x{:X})", userop_name, offset);
                // TLS/Thread control block usually located at fs/gs in Linux.
            }
            "lock" | "rep" | "repne" | "repe" => {
                tracing::debug!("Linux HLE: Prefix userop '{}'", userop_name);
            }
            "rdtsc" | "cpuid" | "syscall" | "sysenter" => {
                tracing::info!("Linux HLE: Instruct userop '{}' called", userop_name);
            }
            _ => {
                tracing::debug!("Linux HLE: Unhandled USEROP: {} (inputs: {:?})", userop_name, inputs);
            }
        }
        Ok(HleResult::Continue)
    }
}
