use anyhow::Result;
use crate::core::Emulator;
use crate::pcode::state::MachineState;
use crate::os::env::{HleResult, OsEnvironment};
use fission_loader::loader::LoadedBinary;

const MAGIC_BASE: u64 = 0xFFFFFFF100000000;

/// Linux ELF execution environment.
///
/// - Import patching: overwrites GOT slots for PLT-reachable symbols.
/// - HLE dispatch: emulates libc functions and syscalls by name.
pub struct LinuxEnv;

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
        tracing::info!("HLE Intercept (Linux): {}", func_name);
        match func_name {
            "exit" | "_exit" => {
                let code = emu.read_arg(0).unwrap_or(0) as u32;
                tracing::info!("exit({}). Emulation finished.", code);
                return Ok(HleResult::Halt(code));
            }
            "puts" => {
                let addr = emu.read_arg(0)?;
                let s = read_string(emu, addr)?;
                tracing::info!("puts(\"{}\")", s);
                emu.write_return_val(s.len() as u64 + 1)?;
            }
            "printf" => {
                let addr = emu.read_arg(0)?;
                let fmt = read_string(emu, addr)?;
                tracing::info!("printf(\"{}\")", fmt.escape_debug());
                emu.write_return_val(fmt.len() as u64)?;
            }
            "malloc" => {
                let size = emu.read_arg(0)?;
                tracing::info!("malloc(0x{:X})", size);
                emu.write_return_val(0x50000000)?; // dummy heap
            }
            "free" => {
                let ptr = emu.read_arg(0)?;
                tracing::info!("free(0x{:X})", ptr);
                emu.write_return_val(0)?;
            }
            "read" => {
                let fd = emu.read_arg(0)?;
                let buf = emu.read_arg(1)?;
                let count = emu.read_arg(2)?;
                tracing::info!("read({}, 0x{:X}, {})", fd, buf, count);
                // Dummy read, just return 0 (EOF) or simulate
                emu.write_return_val(0)?;
            }
            "write" => {
                let fd = emu.read_arg(0)?;
                let buf = emu.read_arg(1)?;
                let count = emu.read_arg(2)?;
                if fd == 1 || fd == 2 {
                    let data = emu.state.read_space(3, buf, count as usize)?;
                    print!("{}", String::from_utf8_lossy(&data));
                } else {
                    tracing::info!("write({}, 0x{:X}, {})", fd, buf, count);
                }
                emu.write_return_val(count)?;
            }
            "mmap" => {
                let addr = emu.read_arg(0)?;
                let length = emu.read_arg(1)?;
                let prot = emu.read_arg(2)?;
                let flags = emu.read_arg(3)?;
                let fd = emu.read_arg(4)?;
                let offset = emu.read_arg(5)?;
                tracing::info!("mmap(0x{:X}, 0x{:X}, {}, {}, {}, 0x{:X})", addr, length, prot, flags, fd, offset);
                // Return a dummy heap address
                emu.write_return_val(0x60000000)?;
            }
            "brk" => {
                let brk = emu.read_arg(0)?;
                tracing::info!("brk(0x{:X})", brk);
                // Return the new brk or the current brk if 0
                let new_brk = if brk == 0 { 0x50000000 } else { brk };
                emu.write_return_val(new_brk)?;
            }
            "syscall" => {
                // If intercepted via a PLT "syscall" wrapper, RAX has the syscall number
                let sys_num = emu.read_register_u64("RAX").unwrap_or(0);
                tracing::info!("syscall({})", sys_num);
                match sys_num {
                    0 => { // read
                        let fd = emu.read_register_u64("RDI").unwrap_or(0);
                        let buf = emu.read_register_u64("RSI").unwrap_or(0);
                        let count = emu.read_register_u64("RDX").unwrap_or(0);
                        tracing::info!("sys_read({}, 0x{:X}, {})", fd, buf, count);
                        emu.write_register_u64("RAX", 0)?;
                    }
                    1 => { // write
                        let fd = emu.read_register_u64("RDI").unwrap_or(0);
                        let buf = emu.read_register_u64("RSI").unwrap_or(0);
                        let count = emu.read_register_u64("RDX").unwrap_or(0);
                        if fd == 1 || fd == 2 {
                            let data = emu.state.read_space(3, buf, count as usize).unwrap_or_default();
                            print!("{}", String::from_utf8_lossy(&data));
                        }
                        emu.write_register_u64("RAX", count)?;
                    }
                    9 => { // mmap
                        let length = emu.read_register_u64("RSI").unwrap_or(0);
                        emu.write_register_u64("RAX", 0x60000000)?;
                        tracing::info!("sys_mmap(len={}) -> 0x60000000", length);
                    }
                    12 => { // brk
                        let brk = emu.read_register_u64("RDI").unwrap_or(0);
                        let new_brk = if brk == 0 { 0x50000000 } else { brk };
                        emu.write_register_u64("RAX", new_brk)?;
                        tracing::info!("sys_brk(0x{:X}) -> 0x{:X}", brk, new_brk);
                    }
                    60 | 231 => { // exit / exit_group
                        let code = emu.read_register_u64("RDI").unwrap_or(0) as u32;
                        tracing::info!("sys_exit({}). Emulation finished.", code);
                        return Ok(HleResult::Halt(code));
                    }
                    _ => {
                        tracing::warn!("Unimplemented Linux x64 syscall: {}", sys_num);
                        emu.write_register_u64("RAX", 0)?;
                    }
                }
            }
            _ => {
                tracing::warn!("Unimplemented libc function: {}. Returning 0.", func_name);
                emu.write_return_val(0)?;
            }
        }
        Ok(HleResult::Continue)
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

fn read_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut bytes = Vec::new();
    let mut cur = addr;
    loop {
        let b = emu.state.read_space(3, cur, 1)?[0];
        if b == 0 { break; }
        bytes.push(b);
        cur += 1;
        if bytes.len() > 4096 { break; }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
