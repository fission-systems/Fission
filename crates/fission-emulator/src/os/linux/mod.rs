pub mod loader;
pub mod libc;
pub mod syscall;
pub mod abi;
pub mod image_info;
pub mod signal;

pub use image_info::{ImageInfo, ProcessArgs};
pub use signal::{DeliverResult, SigAction, SignalState};

use anyhow::Result;
use crate::core::Emulator;
use crate::pcode::state::MachineState;
use crate::os::env::{HleResult, OsEnvironment};
use crate::os::procedure::SimOS;
use fission_loader::loader::LoadedBinary;

const MAGIC_BASE: u64 = 0xFFFFFFF100000000;
/// Synthetic return target after `__libc_start_main` → main; HLE as `exit(RAX)`.
const POST_MAIN_EXIT_STUB: u64 = 0xFFFFFFF1000000F8;

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
        // Dynamic CRT without ld.so: start_main → JumpTo(main); GOT already HLE-patched.
        simos.register_procedure("__libc_start_main", Box::new(libc::LibcStartMain));
        simos.register_procedure("__libc_csu_init", Box::new(libc::NopOk));
        simos.register_procedure("__libc_csu_fini", Box::new(libc::NopOk));
        simos.register_procedure("_init", Box::new(libc::NopOk));
        simos.register_procedure("_fini", Box::new(libc::NopOk));
        simos.register_procedure("__cxa_atexit", Box::new(libc::NopOk));
        simos.register_procedure("__cxa_finalize", Box::new(libc::NopOk));
        simos.register_procedure("atexit", Box::new(libc::NopOk));
        simos.register_procedure("__errno_location", Box::new(libc::NopOk));

        // Register Syscalls (x86-64 Linux numbers)
        simos.register_syscall(0, Box::new(syscall::SysRead));
        simos.register_syscall(1, Box::new(syscall::SysWrite));
        simos.register_syscall(2, Box::new(syscall::SysOpen));
        simos.register_syscall(3, Box::new(syscall::SysClose));
        simos.register_syscall(5, Box::new(syscall::SysFstat));
        simos.register_syscall(8, Box::new(syscall::SysLseek));
        simos.register_syscall(9, Box::new(syscall::SysMmap));
        simos.register_syscall(10, Box::new(syscall::SysMprotect));
        simos.register_syscall(11, Box::new(syscall::SysMunmap));
        simos.register_syscall(12, Box::new(syscall::SysBrk));
        simos.register_syscall(13, Box::new(syscall::SysRtSigaction));
        simos.register_syscall(14, Box::new(syscall::SysRtSigprocmask));
        simos.register_syscall(15, Box::new(syscall::SysRtSigreturn));
        simos.register_syscall(16, Box::new(syscall::SysIoctl));
        simos.register_syscall(62, Box::new(syscall::SysKill));
        simos.register_syscall(200, Box::new(syscall::SysTkill));
        simos.register_syscall(20, Box::new(syscall::SysWritev));
        simos.register_syscall(21, Box::new(syscall::SysAccess));
        simos.register_syscall(24, Box::new(syscall::SysSchedYield));
        simos.register_syscall(39, Box::new(syscall::SysGetpid));
        simos.register_syscall(60, Box::new(syscall::SysExit));
        simos.register_syscall(63, Box::new(syscall::SysUname));
        simos.register_syscall(96, Box::new(syscall::SysGettimeofday));
        simos.register_syscall(102, Box::new(syscall::SysGetuid));
        simos.register_syscall(104, Box::new(syscall::SysGetgid));
        simos.register_syscall(107, Box::new(syscall::SysGeteuid));
        simos.register_syscall(108, Box::new(syscall::SysGetegid));
        simos.register_syscall(158, Box::new(syscall::SysArchPrctl));
        simos.register_syscall(186, Box::new(syscall::SysGettid));
        simos.register_syscall(201, Box::new(syscall::SysTime));
        simos.register_syscall(202, Box::new(syscall::SysFutex));
        simos.register_syscall(218, Box::new(syscall::SysSetTidAddress));
        simos.register_syscall(228, Box::new(syscall::SysClockGettime));
        simos.register_syscall(231, Box::new(syscall::SysExit)); // exit_group
        simos.register_syscall(257, Box::new(syscall::SysOpenat));
        simos.register_syscall(262, Box::new(syscall::SysNewfstatat));
        simos.register_syscall(302, Box::new(syscall::SysPrlimit64));
        simos.register_syscall(318, Box::new(syscall::SysGetrandom));

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
        // Loader uses "ELF64" / "ELF32" / "ELF".
        if !binary.format.starts_with("ELF") {
            return Ok(());
        }
        let mut plt_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        plt_entries.sort_by_key(|&(&addr, _)| addr);
        for (i, (&addr, name)) in plt_entries.into_iter().enumerate() {
            let trampoline = MAGIC_BASE + (i as u64 * 8);
            tracing::debug!("PLT/GOT patch: {} @ 0x{:X} → trampoline 0x{:X}", name, addr, trampoline);
            state.write_space(state.ram_space(), addr, &trampoline.to_le_bytes())?;
        }
        Ok(())
    }

    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        if magic_addr == POST_MAIN_EXIT_STUB {
            return Some("__fission_post_main_exit".into());
        }
        if magic_addr < MAGIC_BASE {
            return None;
        }
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
            emu.metrics.note_syscall(sys_num);
            if let Some(proc) = self.simos.syscalls.get(&sys_num) {
                return proc.run(emu);
            } else {
                tracing::warn!("Unimplemented Linux x64 syscall: {}", sys_num);
                emu.write_register_u64("RAX", 0)?;
                return Ok(HleResult::Continue);
            }
        }

        // After main returns into our synthetic stub: exit code is in RAX.
        if func_name == "__fission_post_main_exit" {
            let code = emu.read_register_u64("RAX").unwrap_or(0) as u32;
            tracing::info!("post-main exit stub: code={}", code);
            return Ok(HleResult::Halt(code));
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
