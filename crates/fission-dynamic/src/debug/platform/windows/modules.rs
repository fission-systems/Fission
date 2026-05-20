//! Detailed loaded-module enumeration for a live Windows process.
//!
//! Wraps `EnumProcessModules` + `GetModuleInformation` into a unified
//! `ModuleInfo` list.  Also provides address-to-module resolution.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::ProcessStatus::{
    EnumProcessModules, GetModuleBaseNameW, GetModuleFileNameExW, GetModuleInformation,
};
use windows::Win32::System::SystemServices::HMODULE;
use std::collections::BTreeMap;

/// Information about a single loaded module (EXE or DLL).
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    pub base: u64,
    pub size: u32,
    pub entry_point: u64,
}

/// Enumerate all loaded modules in the target process.
pub fn enumerate_modules(process: HANDLE) -> Result<Vec<ModuleInfo>, String> {
    let mut capacity = 256usize;
    let mut cb_needed: u32 = 0;

    let modules: Vec<HMODULE> = loop {
        let mut buf = vec![HMODULE::default(); capacity];
        let cb = (capacity * std::mem::size_of::<HMODULE>()) as u32;
        let ok = unsafe { EnumProcessModules(process, buf.as_mut_ptr(), cb, &mut cb_needed).as_bool() };
        if !ok {
            return Err("EnumProcessModules failed".to_string());
        }
        if cb_needed > cb {
            capacity = (cb_needed as usize)
                .div_ceil(std::mem::size_of::<HMODULE>())
                .max(capacity * 2);
            continue;
        }
        break buf;
    };

    let count = cb_needed as usize / std::mem::size_of::<HMODULE>();
    let mut infos = Vec::with_capacity(count);

    for i in 0..count {
        let h_mod = modules[i];
        let base = h_mod.0 as u64;

        let mut name_buf = [0u16; 260];
        let name_len = unsafe { GetModuleBaseNameW(process, h_mod, &mut name_buf) };
        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);

        let mut path_buf = [0u16; 260];
        let path_len = unsafe { GetModuleFileNameExW(process, h_mod, &mut path_buf) };
        let path = String::from_utf16_lossy(&path_buf[..path_len as usize]);

        let mut mod_info = windows::Win32::System::ProcessStatus::MODULEINFO::default();
        unsafe {
            if GetModuleInformation(
                process,
                h_mod,
                &mut mod_info,
                std::mem::size_of::<windows::Win32::System::ProcessStatus::MODULEINFO>() as u32,
            )
            .as_bool()
            {
                infos.push(ModuleInfo {
                    name,
                    path,
                    base,
                    size: mod_info.SizeOfImage,
                    entry_point: mod_info.EntryPoint as u64,
                });
            }
        }
    }

    Ok(infos)
}

/// Build a base-address → module lookup table.
pub fn build_module_lookup(modules: &[ModuleInfo]) -> BTreeMap<u64, ModuleInfo> {
    modules.iter().map(|m| (m.base, m.clone())).collect()
}

/// Resolve an address to the module that contains it.
pub fn resolve_address_to_module(
    addr: u64,
    modules: &[ModuleInfo],
) -> Option<ModuleInfo> {
    for m in modules {
        if addr >= m.base && addr < m.base + m.size as u64 {
            return Some(m.clone());
        }
    }
    None
}
