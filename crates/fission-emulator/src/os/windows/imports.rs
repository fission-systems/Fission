//! PE IAT trampoline table + dynamic GetProcAddress stubs.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use fission_loader::loader::LoadedBinary;

use crate::pcode::state::MachineState;

pub const MAGIC_BASE: u64 = 0xFFFFFFF000000000;

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
}

impl ImportTable {
    pub fn clear(&mut self) {
        self.by_magic.clear();
        self.next_index = 0;
    }

    pub fn resolve(&self, magic_addr: u64) -> Option<String> {
        self.by_magic.get(&magic_addr).cloned()
    }

    pub fn alloc_stub(&mut self, name: &str) -> u64 {
        let magic = MAGIC_BASE + self.next_index * 8;
        self.by_magic.insert(magic, bare_api_name(name));
        self.next_index += 1;
        magic
    }

    /// Patch IAT slots from loader facts; build magic → name map.
    pub fn patch_iat(&mut self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
        self.clear();
        if binary.format != "PE" {
            return Ok(());
        }
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        for (&addr, name) in iat_entries {
            let magic = self.alloc_stub(name);
            tracing::debug!("IAT patch: {} @ 0x{:X} → trampoline 0x{:X}", name, addr, magic);
            state.write_space(state.ram_space(), addr, &magic.to_le_bytes())?;
        }
        Ok(())
    }
}

/// Shared import table for WindowsEnv (interior mutability for GetProcAddress).
pub type SharedImportTable = Mutex<ImportTable>;
