#[cfg(target_os = "windows")]
use super::pe;
#[cfg(target_os = "windows")]
use std::collections::HashMap;

#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::*, Win32::System::ProcessStatus::*, Win32::System::SystemServices::*,
    core::*,
};

#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub rva: u64,
    pub target_address: u64,
    pub module_name: String,
    pub function_name: Option<String>,
    pub ordinal: u32,
}

#[cfg(target_os = "windows")]
struct ModuleInfo {
    name: String,
    size: u32,
    exports: Option<Vec<pe::ExportedFunction>>,
}

pub struct ImportReconstructor {
    #[cfg(target_os = "windows")]
    process_handle: HANDLE,
    #[cfg(target_os = "windows")]
    module_cache: HashMap<u64, ModuleInfo>,
}

impl ImportReconstructor {
    #[cfg(target_os = "windows")]
    pub fn new(process_handle: HANDLE) -> Self {
        Self {
            process_handle,
            module_cache: HashMap::new(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn new(_process_handle: usize) -> Self {
        Self {}
    }

    #[cfg(target_os = "windows")]
    pub fn update_modules(&mut self) -> Result<(), String> {
        // Dynamically resize the module buffer using cb_needed:
        // if cb_needed > cb the buffer was too small — double and retry.
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

    #[cfg(not(target_os = "windows"))]
    pub fn update_modules(&mut self) -> Result<(), String> {
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn reconstruct_iat(
        &mut self,
        iat_start: u64,
        iat_size: usize,
    ) -> Result<Vec<ImportEntry>, String> {
        let mut imports = Vec::new();
        let mut current = iat_start;

        // Read the whole IAT block
        let data = super::memory::read_memory(self.process_handle, iat_start, iat_size)?;

        // Iterate 8 bytes at a time (x64)
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
                // Try to resolve
                if let Ok((module, func, ordinal)) = self.resolve_address(ptr) {
                    imports.push(ImportEntry {
                        rva: current, // Store the address in memory where this pointer was found
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

    #[cfg(not(target_os = "windows"))]
    pub fn reconstruct_iat(
        &mut self,
        _iat_start: u64,
        _iat_size: usize,
    ) -> Result<Vec<ImportEntry>, String> {
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
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
            // Check if we have exports cached
            let needs_parsing = self
                .module_cache
                .get(&base)
                .map(|info| info.exports.is_none())
                .unwrap_or(true);

            if needs_parsing {
                if let Ok(dos) = pe::read_dos_header(self.process_handle, base) {
                    if let Ok(nt) = pe::read_nt_headers64(self.process_handle, base, dos.e_lfanew) {
                        let export_rva = nt.OptionalHeader.DataDirectory[0].VirtualAddress;
                        let export_size = nt.OptionalHeader.DataDirectory[0].Size;

                        if export_rva != 0 && export_size != 0 {
                            if let Ok(export_dir) =
                                pe::read_export_directory(self.process_handle, base, export_rva)
                            {
                                if let Ok(exports) =
                                    pe::parse_exports(self.process_handle, base, &export_dir)
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

    #[cfg(not(target_os = "windows"))]
    pub fn resolve_address(
        &mut self,
        _address: u64,
    ) -> Result<(String, Option<String>, u32), String> {
        Err("Not supported".to_string())
    }
}
