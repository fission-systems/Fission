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
            _ => {
                tracing::warn!("Unimplemented libc function: {}. Returning 0.", func_name);
                emu.write_return_val(0)?;
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
