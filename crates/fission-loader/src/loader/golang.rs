use crate::loader::{FunctionInfo, LoadedBinary};
use crate::prelude::*;

const GO_1_2_MAGIC: u32 = 0xfffffffb;
const GO_1_16_MAGIC: u32 = 0xfffffffa;
const GO_1_18_MAGIC: u32 = 0xfffffff0;
const GO_1_20_MAGIC: u32 = 0xfffffff1;

/// Parser for Go's runtime.pclntab (Program Counter Line Table)
pub struct GoAnalyzer<'a> {
    binary: &'a LoadedBinary,
}

impl<'a> GoAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        Self { binary }
    }

    /// Try to analyze Go-specific metadata and return recovered functions
    pub fn analyze(&self) -> Result<Vec<FunctionInfo>> {
        let pcl_addr = self.find_pclntab_addr()?;
        let Some(data) = self.binary.get_bytes(pcl_addr, 128) else {
            return Err(err!(loader, "Failed to read pclntab header"));
        };

        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        match magic {
            GO_1_16_MAGIC | GO_1_18_MAGIC | GO_1_20_MAGIC => {
                self.parse_modern_pclntab(pcl_addr, magic)
            }
            GO_1_2_MAGIC => self.parse_legacy_pclntab(pcl_addr),
            _ => Err(err!(
                loader,
                "Unsupported or invalid Go pclntab magic: 0x{:x}",
                magic
            )),
        }
    }

    fn find_pclntab_addr(&self) -> Result<u64> {
        for section in &self.binary.sections {
            if section.name == ".gopclntab"
                || section.name == "__gopclntab"
                || section.name == "gopclntab"
            {
                return Ok(section.virtual_address);
            }
        }
        self.heuristic_search_pclntab()
    }

    fn heuristic_search_pclntab(&self) -> Result<u64> {
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        for section in &self.binary.sections {
            if section.is_executable || section.file_size == 0 {
                continue;
            }
            let Some(data) = self
                .binary
                .get_bytes(section.virtual_address, section.virtual_size as usize)
            else {
                continue;
            };
            for i in 0..(data.len().saturating_sub(8)) {
                let magic = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                if matches!(
                    magic,
                    GO_1_2_MAGIC | GO_1_16_MAGIC | GO_1_18_MAGIC | GO_1_20_MAGIC
                ) {
                    if data[i + 4] == 0 && data[i + 5] == 0 && data[i + 7] == ptr_size as u8 {
                        return Ok(section.virtual_address + i as u64);
                    }
                }
            }
        }
        Err(err!(loader, "Could not find Go pclntab"))
    }

    fn parse_modern_pclntab(&self, addr: u64, magic: u32) -> Result<Vec<FunctionInfo>> {
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let Some(header_data) = self.binary.get_bytes(addr, 128) else {
            return Err(err!(loader, "Failed to read pclntab header"));
        };

        let nfunc = self.read_ptr(&header_data, 8, ptr_size) as usize;
        let text_start = self.read_ptr(&header_data, 24, ptr_size);
        let funcname_offset = self.read_ptr(&header_data, 32, ptr_size);
        let functab_offset = if magic >= GO_1_18_MAGIC {
            // For Go 1.18+, pclnOffset is at offset 64
            self.read_ptr(&header_data, 64, ptr_size)
        } else {
            // Before 1.18, functab was at fixed offset after header
            (8 + 2 * ptr_size + 20) as u64
        };

        let functab_addr = addr + functab_offset;
        let entry_size = if magic >= GO_1_18_MAGIC {
            8
        } else {
            ptr_size * 2
        };

        let mut functions = Vec::with_capacity(nfunc);
        for i in 0..nfunc {
            let entry_ptr = functab_addr + (i * entry_size) as u64;
            let Some(ebytes) = self.binary.get_bytes(entry_ptr, entry_size) else {
                break;
            };

            let (pc_off, func_off) = if magic >= GO_1_18_MAGIC {
                (
                    u32::from_le_bytes([ebytes[0], ebytes[1], ebytes[2], ebytes[3]]) as u64,
                    u32::from_le_bytes([ebytes[4], ebytes[5], ebytes[6], ebytes[7]]) as u64,
                )
            } else {
                (
                    self.read_ptr(&ebytes, 0, ptr_size),
                    self.read_ptr(&ebytes, ptr_size, ptr_size),
                )
            };

            let func_pc = text_start + pc_off;

            // In Go 1.20+, func_off can be relative to the start of the functab area
            // or relative to the start of the pclntab depending on Go version and format.
            // We already confirmed that for Go 1.25 Mach-O, it's relative to functab_addr.
            let mut func_struct_addr = functab_addr + func_off;
            let mut fbytes = self.binary.get_bytes(func_struct_addr, 16);

            // Validation: First 4 bytes of _func should be entryOff (matching pc_off)
            if let Some(ref fb) = fbytes {
                let struct_entry_off = u32::from_le_bytes([fb[0], fb[1], fb[2], fb[3]]) as u64;
                if struct_entry_off != pc_off && i > 0 {
                    // Try relative to addr
                    let alt_addr = addr + func_off;
                    if let Some(alt_fb) = self.binary.get_bytes(alt_addr, 16) {
                        let alt_entry_off =
                            u32::from_le_bytes([alt_fb[0], alt_fb[1], alt_fb[2], alt_fb[3]]) as u64;
                        if alt_entry_off == pc_off {
                            func_struct_addr = alt_addr;
                            fbytes = Some(alt_fb);
                        }
                    }
                }
            }

            if let Some(fb) = fbytes {
                let name_off = u32::from_le_bytes([fb[4], fb[5], fb[6], fb[7]]) as u64;
                let name_addr = addr + funcname_offset + name_off;

                if let Some(name) = self.read_string(name_addr) {
                    functions.push(FunctionInfo {
                        name,
                        address: func_pc,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }
            }
        }

        Ok(functions)
    }

    fn parse_legacy_pclntab(&self, addr: u64) -> Result<Vec<FunctionInfo>> {
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let Some(header) = self.binary.get_bytes(addr, 16) else {
            return Err(err!(loader, "Failed to read pclntab header"));
        };

        let nfunc = self.read_ptr(&header, 8, ptr_size) as usize;
        let functab_addr = addr + 8 + ptr_size as u64;
        let mut functions = Vec::with_capacity(nfunc);

        for i in 0..nfunc {
            let entry_addr = functab_addr + (i * ptr_size * 2) as u64;
            let Some(ebytes) = self.binary.get_bytes(entry_addr, ptr_size * 2) else {
                break;
            };
            let pc = self.read_ptr(&ebytes, 0, ptr_size);
            let off = self.read_ptr(&ebytes, ptr_size, ptr_size);

            let Some(fbytes) = self.binary.get_bytes(addr + off, ptr_size + 8) else {
                continue;
            };
            let name_off = self.read_ptr(&fbytes, ptr_size, ptr_size);
            if let Some(name) = self.read_string(addr + name_off) {
                functions.push(FunctionInfo {
                    name,
                    address: pc,
                    size: 0,
                    is_export: false,
                    is_import: false,
                });
            }
        }
        Ok(functions)
    }

    fn read_ptr(&self, data: &[u8], offset: usize, size: usize) -> u64 {
        if offset + size > data.len() {
            return 0;
        }
        if size == 8 {
            u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ])
        } else {
            u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as u64
        }
    }

    fn read_string(&self, addr: u64) -> Option<String> {
        let bytes = self.binary.get_bytes(addr, 256)?;
        let mut len = 0;
        while len < bytes.len() && bytes[len] != 0 {
            len += 1;
        }
        if len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes[..len]).to_string())
    }
}
