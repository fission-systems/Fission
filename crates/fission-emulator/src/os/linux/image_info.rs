//! ELF user-mode process image metadata and initial stack/auxv setup.
//!
//! Cleanroom design inspired by QEMU linux-user `struct image_info` and
//! `create_elf_tables` — no vendor code dependency.

use crate::os::linux::dynlink::{self, DynlinkInfo, DynlinkMode};
use crate::pcode::page_map::{page_align_up, prot, PAGE_SIZE};
use crate::pcode::state::MachineState;
use anyhow::{bail, Context, Result};
use fission_loader::loader::LoadedBinary;
use serde::{Deserialize, Serialize};

/// AT_* auxiliary vector keys (Linux).
pub mod at {
    pub const NULL: u64 = 0;
    pub const PHDR: u64 = 3;
    pub const PHENT: u64 = 4;
    pub const PHNUM: u64 = 5;
    pub const PAGESZ: u64 = 6;
    pub const BASE: u64 = 7;
    pub const FLAGS: u64 = 8;
    pub const ENTRY: u64 = 9;
    pub const UID: u64 = 11;
    pub const EUID: u64 = 12;
    pub const GID: u64 = 13;
    pub const EGID: u64 = 14;
    pub const HWCAP: u64 = 16;
    pub const CLKTCK: u64 = 17;
    pub const RANDOM: u64 = 25;
    pub const HWCAP2: u64 = 26;
    pub const EXECFN: u64 = 31;
    pub const SYSINFO_EHDR: u64 = 33;
}

/// Process image facts after ELF load (user-mode).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ImageInfo {
    pub load_addr: u64,
    pub entry: u64,
    pub start_code: u64,
    pub end_code: u64,
    pub start_data: u64,
    pub end_data: u64,
    pub brk: u64,
    pub start_stack: u64,
    pub stack_limit: u64,
    pub phdr_addr: u64,
    pub phent: u64,
    pub phnum: u64,
    pub argc: u64,
    pub argv: u64,
    pub envc: u64,
    pub envp: u64,
    pub auxv: u64,
    pub auxv_len: u64,
    pub execfn: String,
    pub is_64bit: bool,
    /// Dynamic linker scaffolding result (static / hle_got / interpreter).
    pub dynlink: DynlinkInfo,
}

/// Options for initial process stack / argv.
#[derive(Clone, Debug)]
pub struct ProcessArgs {
    pub argv: Vec<String>,
    pub envp: Vec<String>,
}

impl Default for ProcessArgs {
    fn default() -> Self {
        Self {
            argv: vec!["fission-guest".into()],
            envp: vec![
                "PATH=/usr/bin:/bin".into(),
                "HOME=/home/user".into(),
                "PWD=/".into(),
            ],
        }
    }
}

/// Map ELF image sections, compute brk, and build initial stack with argc/argv/envp/auxv.
///
/// Returns [`ImageInfo`] and leaves `state.page_map.brk` aligned to the heap base.
pub fn load_elf_image(
    state: &mut MachineState,
    binary: &LoadedBinary,
    args: &ProcessArgs,
) -> Result<ImageInfo> {
    let inner = binary.inner();
    let is_64bit = inner.is_64bit;
    let image_base = inner.image_base;
    // Dynlink decision before final entry selection (interpreter may override).
    let dynlink = dynlink::prepare_dynlink(state, binary)?;
    let entry = match dynlink.mode {
        DynlinkMode::Interpreter if dynlink.interp_entry != 0 => dynlink.interp_entry,
        _ => inner.entry_point,
    };

    tracing::info!(
        "ELF image load: entry=0x{:X} base=0x{:X} 64bit={} dynlink={:?}",
        entry,
        image_base,
        is_64bit,
        dynlink.mode
    );

    let mut start_code = u64::MAX;
    let mut end_code = 0u64;
    let mut start_data = u64::MAX;
    let mut end_data = 0u64;
    let mut max_end = 0u64;

    for sec in &inner.sections {
        if sec.virtual_address == 0 || sec.virtual_size == 0 {
            continue;
        }
        let va = sec.virtual_address;
        let vsz = sec.virtual_size;
        let end = va.saturating_add(vsz);

        let mut page_prot = prot::VALID | prot::READ;
        if sec.is_writable {
            page_prot |= prot::WRITE;
        }
        if sec.is_executable {
            page_prot |= prot::EXEC;
        }
        if !sec.is_readable && !sec.is_writable && !sec.is_executable {
            page_prot |= prot::READ;
        }

        let sec_data = binary.view_bytes(va, vsz as usize).unwrap_or(&[]);
        let mut buf = vec![0u8; vsz as usize];
        let n = sec_data.len().min(buf.len());
        buf[..n].copy_from_slice(&sec_data[..n]);
        state
            .write_space(state.ram_space(), va, &buf)
            .with_context(|| format!("map section {} at 0x{:X}", sec.name, va))?;
        state.page_map.map_region(va, vsz, page_prot, false);

        if sec.is_executable {
            start_code = start_code.min(va);
            end_code = end_code.max(end);
        } else {
            start_data = start_data.min(va);
            end_data = end_data.max(end);
        }
        max_end = max_end.max(end);

        tracing::debug!(
            "  section {} 0x{:X}..0x{:X} prot=0x{:02X}",
            sec.name,
            va,
            end,
            page_prot
        );
    }

    if start_code == u64::MAX {
        start_code = image_base;
    }
    if start_data == u64::MAX {
        start_data = end_code;
    }

    // Program break: page after highest mapped image byte.
    let brk_base = page_align_up(max_end.max(image_base));
    state.page_map.set_brk_base(brk_base);

    // Stack region near top of user canonical space.
    let stack_size = 8 * 1024 * 1024u64; // 8 MiB
    let stack_top = if is_64bit {
        0x0000_7FFF_FFFF_F000u64
    } else {
        0xFFFF_E000u64
    };
    let stack_limit = stack_top.saturating_sub(stack_size);
    state
        .page_map
        .map_region(stack_limit, stack_size, prot::RW | prot::ANON, true);

    // Random 16 bytes for AT_RANDOM (deterministic).
    let mut random16 = [0u8; 16];
    for (i, b) in random16.iter_mut().enumerate() {
        *b = (0xA5u8).wrapping_add(i as u8).wrapping_mul(17);
    }

    let execfn = args
        .argv
        .first()
        .cloned()
        .unwrap_or_else(|| "fission-guest".into());

    // PHDR heuristics: image base or first RX section (loader may not expose phdr VA).
    let phdr_addr = image_base;
    let phent = if is_64bit { 56 } else { 32 };
    let phnum = inner.sections.len().min(64) as u64;

    let mut info = ImageInfo {
        load_addr: image_base,
        entry,
        start_code,
        end_code,
        start_data,
        end_data,
        brk: brk_base,
        start_stack: 0,
        stack_limit,
        phdr_addr,
        phent,
        phnum,
        argc: 0,
        argv: 0,
        envc: 0,
        envp: 0,
        auxv: 0,
        auxv_len: 0,
        execfn: execfn.clone(),
        is_64bit,
        dynlink,
    };

    let sp = create_elf_tables(state, &mut info, args, &random16)?;
    info.start_stack = sp;

    tracing::info!(
        "ELF image_info: brk=0x{:X} stack=0x{:X}..0x{:X} argc={} auxv=0x{:X}",
        info.brk,
        info.stack_limit,
        info.start_stack,
        info.argc,
        info.auxv
    );

    Ok(info)
}

/// Build argc / argv / envp / auxv on the guest stack (downward growth).
/// Returns the final SP (pointer to argc).
fn create_elf_tables(
    state: &mut MachineState,
    info: &mut ImageInfo,
    args: &ProcessArgs,
    random16: &[u8; 16],
) -> Result<u64> {
    let is64 = info.is_64bit;
    let ptr_size = if is64 { 8u64 } else { 4u64 };
    let mut sp = if is64 {
        0x0000_7FFF_FFFF_F000u64
    } else {
        0xFFFF_E000u64
    };

    // Push string table (execfn, env, argv) — stack grows down.
    let push_bytes = |state: &mut MachineState, sp: &mut u64, data: &[u8]| -> Result<u64> {
        let len = data.len() as u64;
        *sp = sp.saturating_sub(len);
        // Align after each push for simplicity on 64-bit.
        state.write_space(state.ram_space(), *sp, data)?;
        Ok(*sp)
    };

    let push_cstr = |state: &mut MachineState, sp: &mut u64, s: &str| -> Result<u64> {
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        push_bytes(state, sp, &bytes)
    };

    let execfn_ptr = push_cstr(state, &mut sp, &info.execfn)?;
    let random_ptr = {
        sp = sp.saturating_sub(16);
        state.write_space(state.ram_space(), sp, random16)?;
        sp
    };

    let mut env_ptrs = Vec::new();
    for e in args.envp.iter().rev() {
        env_ptrs.push(push_cstr(state, &mut sp, e)?);
    }
    env_ptrs.reverse();

    let mut arg_ptrs = Vec::new();
    for a in args.argv.iter().rev() {
        arg_ptrs.push(push_cstr(state, &mut sp, a)?);
    }
    arg_ptrs.reverse();

    // Align SP for pointer table (16-byte on x86-64 ABI before call; we just 16-align).
    sp &= !0xFu64;

    let put_ptr = |state: &mut MachineState, sp: &mut u64, val: u64| -> Result<()> {
        *sp = sp.saturating_sub(ptr_size);
        if is64 {
            state.write_space(state.ram_space(), *sp, &val.to_le_bytes())?;
        } else {
            state.write_space(state.ram_space(), *sp, &(val as u32).to_le_bytes())?;
        }
        Ok(())
    };

    // auxv pairs (must end with AT_NULL).
    // AT_BASE: interpreter base when using real ld.so; else 0 for PIE-like HLE.
    // AT_ENTRY: main binary's original entry (ld.so jumps there after relocate).
    let at_base = match info.dynlink.mode {
        DynlinkMode::Interpreter => info.dynlink.interp_base,
        _ => 0,
    };
    let at_entry = if info.dynlink.main_entry != 0 {
        info.dynlink.main_entry
    } else {
        info.entry
    };
    let aux: Vec<(u64, u64)> = vec![
        (at::PHDR, info.phdr_addr),
        (at::PHENT, info.phent),
        (at::PHNUM, info.phnum),
        (at::PAGESZ, PAGE_SIZE),
        (at::BASE, at_base),
        (at::FLAGS, 0),
        (at::ENTRY, at_entry),
        (at::UID, 1000),
        (at::EUID, 1000),
        (at::GID, 1000),
        (at::EGID, 1000),
        (at::HWCAP, 0),
        (at::CLKTCK, 100),
        (at::RANDOM, random_ptr),
        (at::EXECFN, execfn_ptr),
        (at::NULL, 0),
    ];

    // Push auxv in reverse so lowest address is first pair.
    for &(k, v) in aux.iter().rev() {
        put_ptr(state, &mut sp, v)?;
        put_ptr(state, &mut sp, k)?;
    }
    info.auxv = sp;
    info.auxv_len = (aux.len() as u64) * 2 * ptr_size;

    // NULL env terminator, then envp pointers (reverse push)
    put_ptr(state, &mut sp, 0)?;
    for p in env_ptrs.iter().rev() {
        put_ptr(state, &mut sp, *p)?;
    }
    info.envp = sp;
    info.envc = env_ptrs.len() as u64;

    // NULL argv terminator, then argv
    put_ptr(state, &mut sp, 0)?;
    for p in arg_ptrs.iter().rev() {
        put_ptr(state, &mut sp, *p)?;
    }
    info.argv = sp;
    info.argc = arg_ptrs.len() as u64;

    // argc
    put_ptr(state, &mut sp, info.argc)?;

    if sp < info.stack_limit {
        bail!(
            "initial stack overflow: sp=0x{:X} < limit=0x{:X}",
            sp,
            info.stack_limit
        );
    }

    Ok(sp)
}

/// Apply ImageInfo stack pointer into guest registers (RSP/ESP).
pub fn apply_stack_pointer(emu: &mut crate::core::Emulator, info: &ImageInfo) -> Result<()> {
    let sp_reg = emu.arch.sp_reg;
    emu.write_register_u64(sp_reg, info.start_stack)?;
    // ABI: entry expects argc at [RSP]; we already placed it.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::state::MachineState;

    #[test]
    fn auxv_stack_layout_smoke() {
        // Minimal synthetic: no binary sections — just tables.
        let mut state = MachineState::new();
        state.page_map.map_region(0x7FFF_0000_0000, 0x10_0000, prot::RW, true);
        let mut info = ImageInfo {
            load_addr: 0x400000,
            entry: 0x401000,
            brk: 0x600000,
            stack_limit: 0x7FFF_0000_0000,
            phdr_addr: 0x400040,
            phent: 56,
            phnum: 3,
            is_64bit: true,
            execfn: "test".into(),
            ..Default::default()
        };
        let args = ProcessArgs {
            argv: vec!["test".into(), "--help".into()],
            envp: vec!["FOO=bar".into()],
        };
        let random = [1u8; 16];
        let sp = create_elf_tables(&mut state, &mut info, &args, &random).unwrap();
        assert!(sp >= info.stack_limit);
        assert_eq!(info.argc, 2);
        assert_ne!(info.auxv, 0);
        // argc at SP
        let bytes = state.read_space(state.ram_space(), sp, 8).unwrap();
        let argc = u64::from_le_bytes(bytes.try_into().unwrap());
        assert_eq!(argc, 2);
    }
}
