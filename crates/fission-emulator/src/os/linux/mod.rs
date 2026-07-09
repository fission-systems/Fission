pub mod loader;
pub mod libc;
pub mod syscall;
pub mod abi;
pub mod dynlink;
pub mod image_info;
pub mod signal;

pub use dynlink::{DynlinkInfo, DynlinkMode};
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
/// - Optional lazy PLT: GOT holds markers until first call binds the slot.
/// - HLE dispatch: emulates libc functions and syscalls using SimProcedures.
pub struct LinuxEnv {
    pub simos: SimOS,
    /// Deferred PLT/GOT binding table (when `FISSION_LAZY_BIND=1`).
    pub plt_lazy: std::sync::Mutex<Option<dynlink::PltLazyTable>>,
}

impl LinuxEnv {
    pub fn new() -> Self {
        let mut simos = SimOS::new();
        
        // Register Libc Procedures
        simos.register_procedure("malloc", Box::new(libc::Malloc));
        simos.register_procedure("calloc", Box::new(libc::Calloc));
        simos.register_procedure("free", Box::new(libc::Free));
        simos.register_procedure("strlen", Box::new(libc::Strlen));
        simos.register_procedure("strcmp", Box::new(libc::Strcmp));
        simos.register_procedure("strncmp", Box::new(libc::Strncmp));
        simos.register_procedure("memcpy", Box::new(libc::Memcpy));
        simos.register_procedure("memmove", Box::new(libc::Memmove));
        simos.register_procedure("memset", Box::new(libc::Memset));
        simos.register_procedure("mmap", Box::new(libc::Mmap));
        simos.register_procedure("puts", Box::new(libc::Puts));
        simos.register_procedure("printf", Box::new(libc::Printf));
        simos.register_procedure("snprintf", Box::new(libc::Snprintf));
        simos.register_procedure("__snprintf_chk", Box::new(libc::Snprintf));
        simos.register_procedure("stat", Box::new(libc::Stat));
        simos.register_procedure("__xstat", Box::new(libc::Stat));
        simos.register_procedure("__xstat64", Box::new(libc::Stat));
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

        Self {
            simos,
            plt_lazy: std::sync::Mutex::new(None),
        }
    }

    /// Install a lazy PLT table (called after shared-lib load when lazy bind is on).
    pub fn set_plt_lazy(&self, table: dynlink::PltLazyTable) {
        *self.plt_lazy.lock().unwrap_or_else(|e| e.into_inner()) = Some(table);
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
        // When a real interpreter is mapped, leave GOT for ld.so to fill.
        if dynlink::should_skip_got_hle(binary) {
            tracing::info!("dynlink: skipping GOT HLE patch (interpreter path active)");
            return Ok(());
        }

        let mut plt_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        plt_entries.sort_by_key(|&(&addr, _)| addr);

        if dynlink::lazy_bind_enabled() && !plt_entries.is_empty() {
            // Lazy PLT: install markers; first call binds via resolve_stub path.
            let mut table = dynlink::PltLazyTable::default();
            for (i, (addr, name)) in plt_entries.iter().enumerate() {
                let addr = **addr;
                let bare = name
                    .split('@')
                    .next()
                    .unwrap_or(name)
                    .split('!')
                    .last()
                    .unwrap_or(name)
                    .to_string();
                table.entries.push((addr, bare));
                let mark = dynlink::make_lazy_mark(i);
                tracing::debug!(
                    "PLT lazy patch: {} @ 0x{:X} → mark 0x{:X}",
                    name,
                    addr,
                    mark
                );
                state.write_space(state.ram_space(), addr, &mark.to_le_bytes())?;
            }
            self.set_plt_lazy(table);
            return Ok(());
        }

        for (i, (&addr, name)) in plt_entries.into_iter().enumerate() {
            // Preserve mini-dynlink BIND_NOW / SharedLibs resolutions: if the GOT
            // already holds a non-magic guest VA, leave it alone.
            if let Ok(cur) = state.read_space(state.ram_space(), addr, 8) {
                if cur.len() == 8 {
                    let val = u64::from_le_bytes(cur.try_into().unwrap_or([0; 8]));
                    if dynlink::is_resolved_got_target(val) {
                        tracing::debug!(
                            "PLT/GOT keep resolved: {} @ 0x{:X} → 0x{:X}",
                            name,
                            addr,
                            val
                        );
                        continue;
                    }
                }
            }
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
        // Lazy PLT marker: synthetic name consumed by dispatch_hle.
        if let Some(idx) = dynlink::lazy_mark_index(magic_addr) {
            return Some(format!("__plt_lazy_{idx}"));
        }
        if magic_addr == dynlink::PLT_RESOLVER_STUB {
            return Some("__plt_resolver".into());
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
        // Lazy PLT: bind GOT slot then jump to resolved target (no return).
        if let Some(idx_str) = func_name.strip_prefix("__plt_lazy_") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                let table = self.plt_lazy.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref tbl) = *table {
                    let extra = emu
                        .image_info
                        .as_ref()
                        .map(|i| i.dynlink.global_symbols.clone())
                        .unwrap_or_default();
                    if let Some(target) = tbl.bind_slot(&mut emu.state, idx, MAGIC_BASE, &extra) {
                        // If target is still in HLE magic range, dispatch that name instead.
                        if target >= MAGIC_BASE && dynlink::lazy_mark_index(target).is_none() {
                            drop(table);
                            let name = self
                                .resolve_stub(&emu.binary, target)
                                .unwrap_or_else(|| format!("Unknown@0x{target:X}"));
                            return self.dispatch_hle(emu, &name);
                        }
                        return Ok(HleResult::JumpTo(target));
                    }
                }
                tracing::warn!("plt lazy: missing table/slot {idx}");
                return Ok(HleResult::Halt(127));
            }
        }
        if func_name == "__plt_resolver" {
            // Classic resolver entry without index — treat as halt diagnostic.
            tracing::warn!("plt resolver stub hit without slot index");
            return Ok(HleResult::Halt(127));
        }

        if func_name == "syscall" {
            let sys_num = emu.read_register_u64("RAX").unwrap_or(0);
            emu.metrics.note_syscall(sys_num);
            if let Some(proc) = self.simos.syscalls.get(&sys_num) {
                return proc.run(emu);
            } else {
                tracing::warn!("Unimplemented Linux x64 syscall: {}", sys_num);
                emu.metrics.note_unknown_syscall(sys_num);
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
            emu.metrics.note_hle_miss(func_name);
            emu.write_return_val(0)?;
            Ok(HleResult::Continue)
        }
    }

    fn dispatch_userop(
        &self,
        emu: &mut Emulator,
        userop_name: &str,
        inputs: &[u64],
        _output_size: u32,
    ) -> Result<HleResult> {
        match userop_name {
            // Ghidra x86: `segment(FS, off)` / named `segment_fs` → linear address.
            "segment_fs" => {
                let offset = inputs.last().copied().unwrap_or(0);
                emu.callother_result = emu.fs_base.wrapping_add(offset);
                tracing::debug!(
                    "Linux HLE: segment_fs base=0x{:X} off=0x{:X} -> 0x{:X}",
                    emu.fs_base,
                    offset,
                    emu.callother_result
                );
            }
            "segment_gs" => {
                let offset = inputs.last().copied().unwrap_or(0);
                emu.callother_result = emu.gs_base.wrapping_add(offset);
            }
            "segment" => {
                // inputs: [seg_id_or_base, offset] — if first looks like FS selector
                // (0x63 / common) or we only have offset, use fs_base.
                let (base, offset) = match inputs.len() {
                    0 => (emu.fs_base, 0),
                    1 => (emu.fs_base, inputs[0]),
                    _ => {
                        let a = inputs[0];
                        let b = inputs[1];
                        // Heuristic: small first arg is selector → use FS/GS base.
                        if a <= 0x100 {
                            (emu.fs_base, b)
                        } else {
                            (a, b)
                        }
                    }
                };
                emu.callother_result = base.wrapping_add(offset);
                tracing::debug!(
                    "Linux HLE: segment base=0x{:X} off=0x{:X} -> 0x{:X}",
                    base,
                    offset,
                    emu.callother_result
                );
            }
            "lock" | "rep" | "repne" | "repe" => {
                tracing::debug!("Linux HLE: Prefix userop '{}'", userop_name);
            }
            "rdtsc" | "cpuid" | "syscall" | "sysenter" => {
                tracing::info!("Linux HLE: Instruct userop '{}' called", userop_name);
            }
            _ => {
                tracing::debug!(
                    "Linux HLE: Unhandled USEROP: {} (inputs: {:?})",
                    userop_name,
                    inputs
                );
            }
        }
        Ok(HleResult::Continue)
    }
}
