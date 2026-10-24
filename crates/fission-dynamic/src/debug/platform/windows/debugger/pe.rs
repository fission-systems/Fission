use super::*;
use fission_core::{FissionError, Result as FissionResult};

impl WindowsDebugger {
    pub(super) fn get_module_exports(
        &self,
        base: u64,
    ) -> FissionResult<Vec<crate::debug::types::ExportInfo>> {
        let dos = self.read_memory(base, 64)?;
        let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
        let pe_sig = self.read_memory(base + e_lfanew, 4)?;
        if &pe_sig != b"PE\0\0" {
            return Err(FissionError::debug("Invalid PE signature"));
        }

        let coff = self.read_memory(base + e_lfanew + 4, 20)?;
        let machine = u16::from_le_bytes([coff[0], coff[1]]);
        let size_optional = u16::from_le_bytes([coff[16], coff[17]]);

        let is_64 = if machine == 0x8664 && size_optional >= 240 {
            true
        } else if machine == 0x014c && size_optional >= 224 {
            false
        } else {
            return Err(FissionError::debug("Unsupported PE format"));
        };

        let opt_offset = base + e_lfanew + 24;
        let dd_offset = if is_64 {
            opt_offset + 112
        } else {
            opt_offset + 96
        };
        let export_dd = self.read_memory(dd_offset, 8)?;
        let export_rva =
            u32::from_le_bytes([export_dd[0], export_dd[1], export_dd[2], export_dd[3]]);
        let export_size =
            u32::from_le_bytes([export_dd[4], export_dd[5], export_dd[6], export_dd[7]]);

        if export_rva == 0 {
            return Ok(Vec::new());
        }

        let ed_offset = base + export_rva as u64;
        let ed = self.read_memory(ed_offset, 40)?;
        let base_ordinal = u32::from_le_bytes([ed[16], ed[17], ed[18], ed[19]]);
        let number_of_functions = u32::from_le_bytes([ed[20], ed[21], ed[22], ed[23]]);
        let number_of_names = u32::from_le_bytes([ed[24], ed[25], ed[26], ed[27]]);
        let addr_of_functions = u32::from_le_bytes([ed[28], ed[29], ed[30], ed[31]]);
        let addr_of_names = u32::from_le_bytes([ed[32], ed[33], ed[34], ed[35]]);
        let addr_of_name_ordinals = u32::from_le_bytes([ed[36], ed[37], ed[38], ed[39]]);

        let mut exports = Vec::new();
        let mut named_func_indices = std::collections::HashSet::new();

        if addr_of_functions != 0 {
            let functions_table = base + addr_of_functions as u64;

            // 1. Process named exports
            if number_of_names > 0 && addr_of_names != 0 && addr_of_name_ordinals != 0 {
                let names_table = base + addr_of_names as u64;
                let ordinals_table = base + addr_of_name_ordinals as u64;

                for idx in 0..number_of_names.min(10000) {
                    let name_rva_bytes = self.read_memory(names_table + idx as u64 * 4, 4)?;
                    let name_rva = u32::from_le_bytes([
                        name_rva_bytes[0],
                        name_rva_bytes[1],
                        name_rva_bytes[2],
                        name_rva_bytes[3],
                    ]);
                    let ord_bytes = self.read_memory(ordinals_table + idx as u64 * 2, 2)?;
                    let ordinal_index = u16::from_le_bytes([ord_bytes[0], ord_bytes[1]]);

                    if name_rva != 0 {
                        let name = self.read_cstring(base + name_rva as u64, 256);
                        let func_idx = ordinal_index as u64;
                        if func_idx < number_of_functions as u64 {
                            named_func_indices.insert(func_idx);
                            let func_rva_bytes =
                                self.read_memory(functions_table + func_idx * 4, 4)?;
                            let func_rva = u32::from_le_bytes([
                                func_rva_bytes[0],
                                func_rva_bytes[1],
                                func_rva_bytes[2],
                                func_rva_bytes[3],
                            ]);
                            if func_rva != 0 {
                                let actual_ordinal = base_ordinal + ordinal_index as u32;
                                let is_forwarder = export_size > 0
                                    && func_rva >= export_rva
                                    && func_rva < export_rva.saturating_add(export_size);

                                let target_address = if is_forwarder {
                                    let forwarder_str =
                                        self.read_cstring(base + func_rva as u64, 256);
                                    self.resolve_forwarder_depth(&forwarder_str, 0).unwrap_or(0)
                                } else {
                                    base + func_rva as u64
                                };

                                if target_address != 0 {
                                    exports.push(crate::debug::types::ExportInfo {
                                        name,
                                        address: target_address,
                                        ordinal: Some(actual_ordinal),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 2. Process ordinal-only exports
            for func_idx in 0..(number_of_functions as u64).min(10000) {
                if !named_func_indices.contains(&func_idx) {
                    let func_rva_bytes = self.read_memory(functions_table + func_idx * 4, 4)?;
                    let func_rva = u32::from_le_bytes([
                        func_rva_bytes[0],
                        func_rva_bytes[1],
                        func_rva_bytes[2],
                        func_rva_bytes[3],
                    ]);
                    if func_rva != 0 {
                        let actual_ordinal = base_ordinal + func_idx as u32;
                        let is_forwarder = export_size > 0
                            && func_rva >= export_rva
                            && func_rva < export_rva.saturating_add(export_size);

                        let target_address = if is_forwarder {
                            let forwarder_str = self.read_cstring(base + func_rva as u64, 256);
                            self.resolve_forwarder_depth(&forwarder_str, 0).unwrap_or(0)
                        } else {
                            base + func_rva as u64
                        };

                        if target_address != 0 {
                            exports.push(crate::debug::types::ExportInfo {
                                name: format!("Ordinal_{actual_ordinal}"),
                                address: target_address,
                                ordinal: Some(actual_ordinal),
                            });
                        }
                    }
                }
            }
        }
        Ok(exports)
    }

    fn resolve_forwarder_depth(&self, forwarder: &str, depth: usize) -> Option<u64> {
        if depth > 4 {
            return None;
        }
        let (mod_part, func_part) = forwarder.split_once('.')?;
        let mod_name_lower = mod_part.to_ascii_lowercase();
        let mod_base = self.state.modules.values().find_map(|m| {
            let name_lower = m.name.to_ascii_lowercase();
            let stem = name_lower.strip_suffix(".dll").unwrap_or(&name_lower);
            if name_lower == mod_name_lower || stem == mod_name_lower {
                Some(m.base_address)
            } else {
                None
            }
        })?;

        if let Some(ord_str) = func_part.strip_prefix('#') {
            let ord = ord_str.parse::<u32>().ok()?;
            self.get_module_exports(mod_base)
                .ok()?
                .into_iter()
                .find_map(|e| {
                    if e.ordinal == Some(ord) {
                        Some(e.address)
                    } else {
                        None
                    }
                })
        } else {
            self.get_module_exports(mod_base)
                .ok()?
                .into_iter()
                .find_map(|e| {
                    if e.name.eq_ignore_ascii_case(func_part) {
                        Some(e.address)
                    } else {
                        None
                    }
                })
        }
    }

    pub(super) fn get_module_imports(
        &self,
        base: u64,
    ) -> FissionResult<Vec<crate::debug::types::ImportInfo>> {
        let dos = self.read_memory(base, 64)?;
        let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
        let pe_sig = self.read_memory(base + e_lfanew, 4)?;
        if &pe_sig != b"PE\0\0" {
            return Err(FissionError::debug("Invalid PE signature"));
        }

        let coff = self.read_memory(base + e_lfanew + 4, 20)?;
        let machine = u16::from_le_bytes([coff[0], coff[1]]);
        let size_optional = u16::from_le_bytes([coff[16], coff[17]]);

        let is_64 = if machine == 0x8664 && size_optional >= 240 {
            true
        } else if machine == 0x014c && size_optional >= 224 {
            false
        } else {
            return Err(FissionError::debug("Unsupported PE format"));
        };

        let opt_offset = base + e_lfanew + 24;
        let dd_offset = if is_64 {
            opt_offset + 112
        } else {
            opt_offset + 96
        };
        let import_dd = self.read_memory(dd_offset + 8, 8)?; // DataDirectory[1]
        let import_rva =
            u32::from_le_bytes([import_dd[0], import_dd[1], import_dd[2], import_dd[3]]);

        if import_rva == 0 {
            return Ok(Vec::new());
        }

        let mut imports = Vec::new();
        let mut descriptor_offset = base + import_rva as u64;
        loop {
            let desc = self.read_memory(descriptor_offset, 20)?;
            let orig_first_thunk = u32::from_le_bytes([desc[0], desc[1], desc[2], desc[3]]);
            let _time_date_stamp = u32::from_le_bytes([desc[4], desc[5], desc[6], desc[7]]);
            let _forwarder_chain = u32::from_le_bytes([desc[8], desc[9], desc[10], desc[11]]);
            let name_rva = u32::from_le_bytes([desc[12], desc[13], desc[14], desc[15]]);
            let first_thunk = u32::from_le_bytes([desc[16], desc[17], desc[18], desc[19]]);

            if orig_first_thunk == 0 && first_thunk == 0 {
                break;
            }

            let dll_name = if name_rva != 0 {
                self.read_cstring(base + name_rva as u64, 256)
            } else {
                String::new()
            };

            let thunk_rva = if orig_first_thunk != 0 {
                orig_first_thunk
            } else {
                first_thunk
            };
            let thunk_size = if is_64 { 8 } else { 4 };
            let mut thunk_offset = base + thunk_rva as u64;

            loop {
                let thunk_bytes = self.read_memory(thunk_offset, thunk_size)?;
                let thunk_val = if is_64 {
                    u64::from_le_bytes([
                        thunk_bytes[0],
                        thunk_bytes[1],
                        thunk_bytes[2],
                        thunk_bytes[3],
                        thunk_bytes[4],
                        thunk_bytes[5],
                        thunk_bytes[6],
                        thunk_bytes[7],
                    ])
                } else {
                    u32::from_le_bytes([
                        thunk_bytes[0],
                        thunk_bytes[1],
                        thunk_bytes[2],
                        thunk_bytes[3],
                    ]) as u64
                };

                if thunk_val == 0 {
                    break;
                }

                let ordinal_mask = if is_64 { 1u64 << 63 } else { 1u64 << 31 };
                if (thunk_val & ordinal_mask) != 0 {
                    // Ordinal import
                    let ordinal = (thunk_val & 0xFFFF) as u16;
                    imports.push(crate::debug::types::ImportInfo {
                        module: dll_name.clone(),
                        name: None,
                        ordinal: Some(ordinal),
                        address: base
                            + first_thunk as u64
                            + (thunk_offset - (base + thunk_rva as u64)),
                    });
                } else {
                    // Name import
                    let hint_name_addr = base + thunk_val;
                    let hint_name = self.read_memory(hint_name_addr, 256)?;
                    let name = Self::extract_null_terminated_string(&hint_name[2..]);
                    imports.push(crate::debug::types::ImportInfo {
                        module: dll_name.clone(),
                        name: if name.is_empty() { None } else { Some(name) },
                        ordinal: None,
                        address: base
                            + first_thunk as u64
                            + (thunk_offset - (base + thunk_rva as u64)),
                    });
                }

                thunk_offset += thunk_size as u64;
            }

            descriptor_offset += 20;
        }
        Ok(imports)
    }

    /// Read a null-terminated ASCII string from process memory.
    fn read_cstring(&self, addr: u64, max_len: usize) -> String {
        match self.read_memory(addr, max_len) {
            Ok(bytes) => Self::extract_null_terminated_string(&bytes),
            Err(_) => String::new(),
        }
    }

    fn extract_null_terminated_string(bytes: &[u8]) -> String {
        bytes
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordinal_import_masks_32_and_64() {
        let is_64_false = false;
        let ordinal_mask_32 = if is_64_false { 1u64 << 63 } else { 1u64 << 31 };
        assert_eq!(ordinal_mask_32, 0x8000_0000);

        // 32-bit ordinal import with ordinal 42
        let thunk_32: u64 = 0x8000_002A;
        assert_ne!(thunk_32 & ordinal_mask_32, 0);
        assert_eq!((thunk_32 & 0xFFFF) as u16, 42);

        // Name import (RVA < 0x8000_0000)
        let thunk_name_32: u64 = 0x0000_2040;
        assert_eq!(thunk_name_32 & ordinal_mask_32, 0);

        let is_64_true = true;
        let ordinal_mask_64 = if is_64_true { 1u64 << 63 } else { 1u64 << 31 };
        assert_eq!(ordinal_mask_64, 0x8000_0000_0000_0000);

        // 64-bit ordinal import with ordinal 99
        let thunk_64: u64 = 0x8000_0000_0000_0063;
        assert_ne!(thunk_64 & ordinal_mask_64, 0);
        assert_eq!((thunk_64 & 0xFFFF) as u16, 99);
    }

    #[test]
    fn test_export_forwarder_detection_and_base_ordinal() {
        let export_rva: u32 = 0x4000;
        let export_size: u32 = 0x500;
        let base_ordinal: u32 = 100;
        let ordinal_index: u16 = 5;

        let actual_ordinal = base_ordinal + ordinal_index as u32;
        assert_eq!(actual_ordinal, 105);

        // Function RVA pointing inside export section -> forwarder
        let forwarder_rva: u32 = 0x4120;
        let is_forwarder = export_size > 0
            && forwarder_rva >= export_rva
            && forwarder_rva < export_rva.saturating_add(export_size);
        assert!(is_forwarder);

        // Function RVA pointing outside export section -> code
        let code_rva: u32 = 0x1200;
        let is_code_forwarder = export_size > 0
            && code_rva >= export_rva
            && code_rva < export_rva.saturating_add(export_size);
        assert!(!is_code_forwarder);
    }
}
