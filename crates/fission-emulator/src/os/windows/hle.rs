use crate::pcode::state::MachineState;
use anyhow::{Result};
use crate::core::Emulator;

pub fn dispatch(emu: &mut Emulator, magic_addr: u64) -> Result<bool> {
    let index = ((magic_addr - 0xFFFFFFF000000000) / 8) as usize;
    let mut iat_entries: Vec<_> = emu.binary.inner().iat_symbols.iter().collect();
    iat_entries.sort_by_key(|&(&addr, _)| addr);
    let full_name = iat_entries.into_iter().nth(index).map(|(_, s)| s.as_str()).unwrap_or("Unknown");
    let name = full_name.split('!').last().unwrap_or(full_name);
    
    tracing::info!("HLE Intercept: {}", name);
    
    let mut continue_execution = true;
    
    match name {
        "LoadLibraryA" => handle_load_library_a(emu)?,
        "GetProcAddress" => handle_get_proc_address(emu)?,
        "VirtualAlloc" => handle_virtual_alloc(emu)?,
        "ExitProcess" => {
            tracing::info!("ExitProcess called. Emulation finished.");
            continue_execution = false;
        }
        _ => {
            tracing::warn!("Unimplemented Win32 API: {}. Returning 0.", name);
            write_return_64(emu, 0)?;
        }
    }
    
    // Simulate return (pop RIP from stack, assuming standard x86/x64 call)
    simulate_return(emu)?;
    Ok(continue_execution)
}

fn simulate_return(emu: &mut Emulator) -> Result<()> {
    let is_64bit = emu.binary.inner().is_64bit;
    if is_64bit {
        let rsp = emu.read_register_u64("rsp")?;
        let ret_addr_bytes = emu.state.read_space(3, rsp, 8)?;
        let mut ret_addr = 0u64;
        for (i, &b) in ret_addr_bytes.iter().enumerate() {
            ret_addr |= (b as u64) << (i * 8);
        }
        emu.rip = ret_addr;
        emu.write_register_u64("rsp", rsp + 8)?;
    } else {
        let esp = emu.read_register_u64("esp")?;
        let ret_addr_bytes = emu.state.read_space(3, esp, 4)?;
        let mut ret_addr = 0u64;
        for (i, &b) in ret_addr_bytes.iter().enumerate() {
            ret_addr |= (b as u64) << (i * 8);
        }
        emu.rip = ret_addr;
        emu.write_register_u64("esp", esp + 4)?;
    }
    Ok(())
}

fn read_arg_64(emu: &mut Emulator, index: usize) -> Result<u64> {
    match index {
        0 => emu.read_register_u64("rcx"),
        1 => emu.read_register_u64("rdx"),
        2 => emu.read_register_u64("r8"),
        3 => emu.read_register_u64("r9"),
        _ => {
            // Stack arguments: [rsp + 40 + (index - 4) * 8]
            let rsp = emu.read_register_u64("rsp")?;
            let offset = rsp + 40 + ((index - 4) * 8) as u64;
            let bytes = emu.state.read_space(3, offset, 8)?;
            let mut val = 0u64;
            for (i, &b) in bytes.iter().enumerate() {
                val |= (b as u64) << (i * 8);
            }
            Ok(val)
        }
    }
}

fn write_return_64(emu: &mut Emulator, value: u64) -> Result<()> {
    emu.write_register_u64("rax", value)
}

fn read_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut string_bytes = Vec::new();
    let mut current_addr = addr;
    loop {
        let b = emu.state.read_space(3, current_addr, 1)?[0];
        if b == 0 {
            break;
        }
        string_bytes.push(b);
        current_addr += 1;
        if string_bytes.len() > 4096 {
            break; // Sanity check
        }
    }
    Ok(String::from_utf8_lossy(&string_bytes).into_owned())
}

fn handle_load_library_a(emu: &mut Emulator) -> Result<()> {
    let is_64bit = emu.binary.inner().is_64bit;
    if is_64bit {
        let lp_lib_file_name_addr = read_arg_64(emu, 0)?;
        let lib_name = read_string(emu, lp_lib_file_name_addr)?;
        tracing::info!("Emulating LoadLibraryA(RCX=0x{:X}) -> \"{}\"", lp_lib_file_name_addr, lib_name);
        write_return_64(emu, 0x10000000)?; // return dummy HMODULE
    } else {
        tracing::warn!("LoadLibraryA 32-bit not fully parsed yet");
    }
    Ok(())
}

fn handle_get_proc_address(emu: &mut Emulator) -> Result<()> {
    let is_64bit = emu.binary.inner().is_64bit;
    if is_64bit {
        let h_module = read_arg_64(emu, 0)?;
        let lp_proc_name_addr = read_arg_64(emu, 1)?;
        
        let proc_name = if lp_proc_name_addr < 0xFFFF {
            format!("Ordinal({})", lp_proc_name_addr)
        } else {
            read_string(emu, lp_proc_name_addr)?
        };
        
        tracing::info!("Emulating GetProcAddress(0x{:X}, \"{}\")", h_module, proc_name);
        write_return_64(emu, 0x20000000)?; // return dummy FARPROC
    } else {
        tracing::warn!("GetProcAddress 32-bit not fully parsed yet");
    }
    Ok(())
}

fn handle_virtual_alloc(emu: &mut Emulator) -> Result<()> {
    let is_64bit = emu.binary.inner().is_64bit;
    if is_64bit {
        let lp_address = read_arg_64(emu, 0)?;
        let dw_size = read_arg_64(emu, 1)?;
        let fl_allocation_type = read_arg_64(emu, 2)?;
        let fl_protect = read_arg_64(emu, 3)?;
        tracing::info!("Emulating VirtualAlloc(0x{:X}, 0x{:X}, 0x{:X}, 0x{:X})", lp_address, dw_size, fl_allocation_type, fl_protect);
        write_return_64(emu, 0x30000000)?; // return dummy allocated address
    } else {
        tracing::warn!("VirtualAlloc 32-bit not fully parsed yet");
    }
    Ok(())
}
