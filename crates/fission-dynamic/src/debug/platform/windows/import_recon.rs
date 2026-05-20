//! Import Address Table (IAT) reconstruction from a live process.
//!
//! Scans process memory to rebuild the IAT, resolving function addresses
//! back to module+function names. Useful for analyzing packed/obfuscated
//! executables.

use std::collections::HashMap;
use windows::{
    Win32::Foundation::*, Win32::System::ProcessStatus::*, Win32::System::SystemServices::*,
};

use super::pe_raw::{self, ExportedFunction};

/// A single reconstructed import entry.
#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub rva: u64,
    pub target_address: u64,
    pub module_name: String,
    pub function_name: Option<String>,
    pub ordinal: u32,
}

struct ModuleInfo {
    name: String,
    size: u32,
    exports: Option<Vec<ExportedFunction>>,
}

/// Reconstructs imports by walking process memory.
pub struct ImportReconstructor {
    process_handle: HANDLE,
    module_cache: HashMap<u64, ModuleInfo>,
}

impl ImportReconstructor {
    pub fn new(process_handle: HANDLE) -> Self {
        Self {
            process_handle,
            module_cache: HashMap::new(),
        }
    }

    /// Refresh the module list from the target process.
    pub fn update_modules(&mut self) -> Result<(), String> {
        let mut capacity = 256usize;
        let mut cb_needed: u32 = 0;
        let modules: Vec<HMODULE> = loop {
            let mut buf = vec![HMODULE::default(); capacity];
            let cb = (capacity * std::mem::size_of::<HMODULE>()) as u32;
            let ok = unsafe {
                EnumProcessModules(self.process_handle, buf.as_mut_ptr(), cb, &mut cb_needed)
                    .as_bool()
            };
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

        unsafe {
            for i in 0..count {
                let h_mod = modules[i];
                let base_addr = h_mod.0 as u64;

                if !self.module_cache.contains_key(&base_addr) {
                    let mut name_buf = [0u16; 256];
                    let len = GetModuleBaseNameW(self.process_handle, h_mod, &mut name_buf);
                    let name = String::from_utf16_lossy(&name_buf[..len as usize]);

                    let mut mod_info = MODULEINFO::default();
                    if GetModuleInformation(
                        self.process_handle,
                        h_mod,
                        &mut mod_info,
                        std::mem::size_of::<MODULEINFO>() as u32,
                    )
                    .as_bool()
                    {
                        self.module_cache.insert(
                            base_addr,
                            ModuleInfo {
                                name,
                                size: mod_info.SizeOfImage,
                                exports: None,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Scan an IAT region and resolve each pointer to a symbol.
    pub fn reconstruct_iat(
        &mut self,
        iat_start: u64,
        iat_size: usize,
    ) -> Result<Vec<ImportEntry>, String> {
        let mut imports = Vec::new();
        let mut current = iat_start;
        let data = pe_raw::read_memory(self.process_handle, iat_start, iat_size)?;

        for chunk in data.chunks(8) {
            if chunk.len() < 8 {
                break;
            }
            let bytes: [u8; 8] = match chunk.try_into() {
                Ok(b) => b,
                Err(_) => continue,
            };
            let ptr = u64::from_le_bytes(bytes);

            if ptr != 0 {
                if let Ok((module, func, ordinal)) = self.resolve_address(ptr) {
                    imports.push(ImportEntry {
                        rva: current,
                        target_address: ptr,
                        module_name: module,
                        function_name: func,
                        ordinal,
                    });
                }
            }
            current += 8;
        }

        Ok(imports)
    }

    /// Resolve an address to (module_name, function_name, ordinal).
    pub fn resolve_address(
        &mut self,
        address: u64,
    ) -> Result<(String, Option<String>, u32), String> {
        let mut target_module: Option<(u64, String)> = None;

        for (base, info) in &self.module_cache {
            if address >= *base && address < *base + info.size as u64 {
                target_module = Some((*base, info.name.clone()));
                break;
            }
        }

        if let Some((base, mod_name)) = target_module {
            let needs_parsing = self
                .module_cache
                .get(&base)
                .map(|info| info.exports.is_none())
                .unwrap_or(true);

            if needs_parsing {
                if let Ok(dos) = pe_raw::read_dos_header(self.process_handle, base) {
                    if let Ok(nt) =
                        pe_raw::read_nt_headers64(self.process_handle, base, dos.e_lfanew)
                    {
                        let export_rva = nt.OptionalHeader.DataDirectory[0].VirtualAddress;
                        let export_size = nt.OptionalHeader.DataDirectory[0].Size;

                        if export_rva != 0 && export_size != 0 {
                            if let Ok(export_dir) =
                                pe_raw::read_export_directory(self.process_handle, base, export_rva)
                            {
                                if let Ok(exports) =
                                    pe_raw::parse_exports(self.process_handle, base, &export_dir)
                                {
                                    if let Some(info) = self.module_cache.get_mut(&base) {
                                        info.exports = Some(exports);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(info) = self.module_cache.get(&base) {
                if let Some(exports) = &info.exports {
                    let rva = (address - base) as u32;
                    for exp in exports {
                        if exp.rva == rva {
                            return Ok((mod_name, exp.name.clone(), exp.ordinal));
                        }
                    }
                    return Ok((mod_name, None, 0));
                }
            }

            return Ok((mod_name, None, 0));
        }

        Err("Address not in any loaded module".to_string())
    }
}
