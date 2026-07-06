use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;

const MAGIC_BASE: u64 = 0xFFFFFFF000000000;

/// Windows PE execution environment.
///
/// - Import patching: overwrites IAT entries with sequential magic trampolines.
/// - Stub resolution: maps magic address back to import name.
/// - HLE dispatch: emulates Win32 API functions by name.
pub struct WindowsEnv;

impl OsEnvironment for WindowsEnv {
    fn patch_imports(&self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
        if binary.format != "PE" {
            return Ok(());
        }
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        for (i, (&addr, name)) in iat_entries.into_iter().enumerate() {
            let trampoline = MAGIC_BASE + (i as u64 * 8);
            tracing::debug!("IAT patch: {} @ 0x{:X} → trampoline 0x{:X}", name, addr, trampoline);
            state.write_space(3, addr, &trampoline.to_le_bytes())?;
        }
        Ok(())
    }

    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        let index = ((magic_addr - MAGIC_BASE) / 8) as usize;
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        iat_entries
            .into_iter()
            .nth(index)
            .map(|(_, name)| name.split('!').last().unwrap_or(name).to_string())
    }

    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult> {
        tracing::info!("HLE Intercept: {}", func_name);
        match func_name {
            "LoadLibraryA"   => handle_load_library_a(emu)?,
            "LoadLibraryW"   => handle_load_library_w(emu)?,
            "GetProcAddress" => handle_get_proc_address(emu)?,
            "VirtualAlloc"   => handle_virtual_alloc(emu)?,
            "VirtualFree"    => { emu.write_return_val(1)?; } // always succeed
            "ExitProcess"    => {
                let code = emu.read_arg(0).unwrap_or(0) as u32;
                tracing::info!("ExitProcess({}). Emulation finished.", code);
                return Ok(HleResult::Halt(code));
            }
            _ => {
                tracing::warn!("Unimplemented Win32 API: {}. Returning 0.", func_name);
                emu.write_return_val(0)?;
            }
        }
        Ok(HleResult::Continue)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

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

fn read_wide_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut chars = Vec::new();
    let mut cur = addr;
    loop {
        let pair = emu.state.read_space(3, cur, 2)?;
        let wc = pair[0] as u16 | ((pair[1] as u16) << 8);
        if wc == 0 { break; }
        chars.push(wc);
        cur += 2;
        if chars.len() > 4096 { break; }
    }
    Ok(String::from_utf16_lossy(&chars))
}

// ── Win32 API handlers ────────────────────────────────────────────────────────

fn handle_load_library_a(emu: &mut Emulator) -> Result<()> {
    let addr = emu.read_arg(0)?;
    let name = if addr == 0 { String::from("<null>") } else { read_string(emu, addr)? };
    tracing::info!("LoadLibraryA(\"{}\")", name);
    emu.write_return_val(0x10000000)?; // dummy HMODULE
    Ok(())
}

fn handle_load_library_w(emu: &mut Emulator) -> Result<()> {
    let addr = emu.read_arg(0)?;
    let name = if addr == 0 { String::from("<null>") } else { read_wide_string(emu, addr)? };
    tracing::info!("LoadLibraryW(\"{}\")", name);
    emu.write_return_val(0x10000001)?; // dummy HMODULE
    Ok(())
}

fn handle_get_proc_address(emu: &mut Emulator) -> Result<()> {
    let h_module = emu.read_arg(0)?;
    let name_ptr = emu.read_arg(1)?;
    let proc_name = if name_ptr < 0xFFFF {
        format!("Ordinal({})", name_ptr)
    } else {
        read_string(emu, name_ptr)?
    };
    tracing::info!("GetProcAddress(0x{:X}, \"{}\")", h_module, proc_name);
    emu.write_return_val(0x20000000)?; // dummy FARPROC
    Ok(())
}

fn handle_virtual_alloc(emu: &mut Emulator) -> Result<()> {
    let lp_address = emu.read_arg(0)?;
    let dw_size    = emu.read_arg(1)?;
    let alloc_type = emu.read_arg(2)?;
    let protect    = emu.read_arg(3)?;
    tracing::info!("VirtualAlloc(0x{:X}, 0x{:X}, 0x{:X}, 0x{:X})", lp_address, dw_size, alloc_type, protect);
    emu.write_return_val(0x30000000)?; // dummy allocated address
    Ok(())
}
