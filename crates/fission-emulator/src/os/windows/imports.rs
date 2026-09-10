//! PE IAT trampoline table + dynamic GetProcAddress stubs.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use fission_loader::loader::LoadedBinary;

use crate::os::windows::crt_data::CrtGlobals;
use crate::pcode::state::MachineState;

/// Where trampolines live in a 64-bit process: above everything a guest can
/// map, so a PC landing there can only be a call through a patched slot.
pub const MAGIC_BASE: u64 = 0xFFFFFFF000000000;

/// The same for a 32-bit process, where that address does not exist.
///
/// A 32-bit IAT slot is four bytes wide, so a 64-bit magic address written
/// into it keeps only its low half -- which for `MAGIC_BASE` is `index * 8`,
/// a jump to address 0xA0 or 0xC0. Every 32-bit PE in the dev corpus died
/// that way, at the first call through the IAT.
///
/// This region sits just above the PEB/TEB pages and well below the stack,
/// so it collides with nothing the loader maps. Windows itself keeps
/// `SharedUserData` next door at 0x7FFE0000.
pub const MAGIC_BASE_32: u64 = 0x7FFF_0000;

/// How far the trampoline region reaches. 8 bytes per stub either way, so
/// this is 8192 imports -- more than any real PE, and bounded so that a wild
/// jump lands outside it and is reported as a wild jump.
pub const MAGIC_SPAN: u64 = 0x1_0000;

/// Bare Win32 API name extracted from `Dll!Func` / ordinal forms.
pub fn bare_api_name(raw: &str) -> String {
    if let Some(idx) = raw.find('!') {
        return raw[idx + 1..].to_string();
    }
    // Loader ordinal form: `kernel32.dll:Ordinal_N` or `dll:Ordinal_N`
    if let Some(idx) = raw.rfind(':') {
        let rest = &raw[idx + 1..];
        if rest.starts_with("Ordinal_") || rest.starts_with("ordinal_") {
            return rest.to_string();
        }
    }
    raw.to_string()
}

#[derive(Default)]
pub struct ImportTable {
    /// Magic trampoline address → bare API name.
    by_magic: HashMap<u64, String>,
    /// Next free trampoline index (IAT + GetProcAddress).
    next_index: u64,
    /// Where this process's trampolines live, and how wide a slot is. Both
    /// follow the image: a 32-bit process cannot hold a 64-bit address.
    base: u64,
    ptr_size: usize,
}

impl ImportTable {
    pub fn clear(&mut self) {
        self.by_magic.clear();
        self.next_index = 0;
    }

    /// The trampoline region, as a half-open range. Before any image is
    /// patched this is the 64-bit one, which is also the right answer for a
    /// process that has no imports at all.
    pub fn magic_range(&self) -> (u64, u64) {
        let base = if self.base == 0 {
            MAGIC_BASE
        } else {
            self.base
        };
        (base, base + MAGIC_SPAN)
    }

    pub fn resolve(&self, magic_addr: u64) -> Option<String> {
        self.by_magic.get(&magic_addr).cloned()
    }

    pub fn alloc_stub(&mut self, name: &str) -> u64 {
        if self.base == 0 {
            self.base = MAGIC_BASE;
        }
        let magic = self.base + self.next_index * 8;
        self.by_magic.insert(magic, bare_api_name(name));
        self.next_index += 1;
        magic
    }

    /// Patch IAT slots from loader facts; build magic → name map.
    ///
    /// `globals` is where imported *data* lives: an IAT slot naming a msvcrt
    /// variable holds a pointer to the variable, and the program writes
    /// through it, so those slots get a cell rather than a trampoline.
    pub fn patch_iat(
        &mut self,
        state: &mut MachineState,
        binary: &LoadedBinary,
        globals: &CrtGlobals,
    ) -> Result<()> {
        self.clear();
        if binary.format != "PE" {
            return Ok(());
        }
        let is_64bit = binary.inner().is_64bit;
        self.base = if is_64bit { MAGIC_BASE } else { MAGIC_BASE_32 };
        self.ptr_size = if is_64bit { 8 } else { 4 };
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        let mut unknown_data = 0u64;
        for (&addr, name) in iat_entries {
            if let Some(cell) = globals.data_cell(&bare_api_name(name), unknown_data) {
                unknown_data += 1;
                tracing::debug!(
                    "IAT patch: {} @ 0x{:X} → data cell 0x{:X}",
                    name,
                    addr,
                    cell
                );
                let bytes = cell.to_le_bytes();
                state.write_space(state.ram_space(), addr, &bytes[..self.ptr_size])?;
                continue;
            }
            let magic = self.alloc_stub(name);
            tracing::debug!(
                "IAT patch: {} @ 0x{:X} → trampoline 0x{:X}",
                name,
                addr,
                magic
            );
            // Exactly one slot wide: eight bytes into a four-byte slot would
            // also overwrite the next import.
            let bytes = magic.to_le_bytes();
            state.write_space(state.ram_space(), addr, &bytes[..self.ptr_size])?;
        }
        Ok(())
    }
}

/// Shared import table for WindowsEnv (interior mutability for GetProcAddress).
pub type SharedImportTable = Mutex<ImportTable>;
