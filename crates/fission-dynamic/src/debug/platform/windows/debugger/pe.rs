use super::WindowsDebugger;
use crate::debug::traits::Debugger;
use fission_core::{FissionError, Result as FissionResult};

impl Debugger for WindowsDebugger {
    fn get_module_exports(&self, base: u64) -> FissionResult<Vec<crate::debug::types::ExportInfo>> {
        let dos = self.read_memory(base, 64)?;
        let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
        let pe_sig = self.read_memory(base + e_lfanew, 4)?;
        if &pe_sig != b"PE\0\0" {
            return Err(FissionError::debug("Invalid PE signature"));
        }

        let coff = self.read_memory(base + e_lfanew + 4, 20)?;
        let machine = u16::from_le_bytes([coff[0], coff[1]]);
        let num_sections = u16::from_le_bytes([coff[2], coff[3]]);
        let size_optional = u16::from_le_bytes([coff[16], coff[17]]);

        let is_64 = if machine == 0x8664 && size_optional >= 240 { true }
            else if machine == 0x014c && size_optional >= 224 { false }
            else { return Err(FissionError::debug("Unsupported PE format")); };

        let opt_offset = base + e_lfanew + 24;
        let dd_offset = if is_64 { opt_offset + 112 } else { opt_offset + 96 };
        let export_dd = self.read_memory(dd_offset, 8)?;
        let export_rva = u32::from_le_bytes([export_dd[0], export_dd[1], export_dd[2], export_dd[3]]);
        let _export_size = u32::from_le_bytes([export_dd[4], export_dd[5], export_dd[6], export_dd[7]]);

        if export_rva == 0 {
            return Ok(Vec::new());
        }

        let ed_offset = base + export_rva as u64;
        let ed = self.read_memory(ed_offset, 40)?;
        let number_of_functions = u32::from_le_bytes([ed[20], ed[21], ed[22], ed[23]]);
        let number_of_names = u32::from_le_bytes([ed[24], ed[25], ed[26], ed[27]]);
        let addr_of_functions = u32::from_le_bytes([ed[28], ed[29], ed[30], ed[31]]);
        let addr_of_names = u32::from_le_bytes([ed[32], ed[33], ed[34], ed[35]]);
        let addr_of_name_ordinals = u32::from_le_bytes([ed[36], ed[37], ed[38], ed[39]]);

        let mut exports = Vec::new();
        if number_of_names > 0 && addr_of_names != 0 && addr_of_name_ordinals != 0 {
            let names_table = base + addr_of_names as u64;
            let ordinals_table = base + addr_of_name_ordinals as u64;
            let functions_table = base + addr_of_functions as u64;

            for idx in 0..number_of_names.min(10000) {
                let name_rva_bytes = self.read_memory(names_table + idx as u64 * 4, 4)?;
                let name_rva = u32::from_le_bytes([name_rva_bytes[0], name_rva_bytes[1], name_rva_bytes[2], name_rva_bytes[3]]);
                let ord_bytes = self.read_memory(ordinals_table + idx as u64 * 2, 2)?;
                let ordinal = u16::from_le_bytes([ord_bytes[0], ord_bytes[1]]);

                if name_rva != 0 {
                    let name = self.read_cstring(base + name_rva as u64, 256);
                    let func_idx = ordinal as u64;
                    if func_idx < number_of_functions as u64 {
                        let func_rva_bytes = self.read_memory(functions_table + func_idx * 4, 4)?;
                        let func_rva = u32::from_le_bytes([func_rva_bytes[0], func_rva_bytes[1], func_rva_bytes[2], func_rva_bytes[3]]);
                        if func_rva != 0 {
                            exports.push(crate::debug::types::ExportInfo {
                                name,
                                address: base + func_rva as u64,
                                ordinal: Some(ordinal as u32),
                            });
                        }
                    }
                }
            }
        }
        Ok(exports)
    }

    fn get_module_imports(&self, base: u64) -> FissionResult<Vec<crate::debug::types::ImportInfo>> {
        let dos = self.read_memory(base, 64)?;
        let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
        let pe_sig = self.read_memory(base + e_lfanew, 4)?;
        if &pe_sig != b"PE\0\0" {
            return Err(FissionError::debug("Invalid PE signature"));
        }

        let coff = self.read_memory(base + e_lfanew + 4, 20)?;
        let machine = u16::from_le_bytes([coff[0], coff[1]]);
        let size_optional = u16::from_le_bytes([coff[16], coff[17]]);

        let is_64 = if machine == 0x8664 && size_optional >= 240 { true }
            else if machine == 0x014c && size_optional >= 224 { false }
            else { return Err(FissionError::debug("Unsupported PE format")); };

        let opt_offset = base + e_lfanew + 24;
        let dd_offset = if is_64 { opt_offset + 112 } else { opt_offset + 96 };
        let import_dd = self.read_memory(dd_offset + 8, 8)?; // DataDirectory[1]
        let import_rva = u32::from_le_bytes([import_dd[0], import_dd[1], import_dd[2], import_dd[3]]);

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

            let thunk_rva = if orig_first_thunk != 0 { orig_first_thunk } else { first_thunk };
            let thunk_size = if is_64 { 8 } else { 4 };
            let mut thunk_offset = base + thunk_rva as u64;

            loop {
                let thunk_bytes = self.read_memory(thunk_offset, thunk_size)?;
                let thunk_val = if is_64 {
                    u64::from_le_bytes([
                        thunk_bytes[0], thunk_bytes[1], thunk_bytes[2], thunk_bytes[3],
                        thunk_bytes[4], thunk_bytes[5], thunk_bytes[6], thunk_bytes[7],
                    ])
                } else {
                    u32::from_le_bytes([thunk_bytes[0], thunk_bytes[1], thunk_bytes[2], thunk_bytes[3]]) as u64
                };

                if thunk_val == 0 {
                    break;
                }

                if (thunk_val & (1u64 << 63)) != 0 {
                    // Ordinal import
                    let ordinal = (thunk_val & 0xFFFF) as u16;
                    imports.push(crate::debug::types::ImportInfo {
                        module: dll_name.clone(),
                        name: None,
                        ordinal: Some(ordinal),
                        address: base + first_thunk as u64 + (thunk_offset - (base + thunk_rva as u64)),
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
                        address: base + first_thunk as u64 + (thunk_offset - (base + thunk_rva as u64)),
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
        bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
    }
}
