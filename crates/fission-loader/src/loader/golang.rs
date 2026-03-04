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
                .view_bytes(section.virtual_address, section.virtual_size as usize)
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

        let mut functions = Vec::with_capacity(nfunc.min(100000)); // Sanity limit
        for i in 0..nfunc.min(100000) {
            let entry_ptr = functab_addr + (i * entry_size) as u64;
            let Some(ebytes) = self.binary.view_bytes(entry_ptr, entry_size) else {
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
            let mut fbytes = self
                .binary
                .view_bytes(func_struct_addr, 16)
                .map(|b| b.to_vec());

            // Validation: First 4 bytes of _func should be entryOff (matching pc_off)
            if let Some(ref fb) = fbytes {
                let struct_entry_off = u32::from_le_bytes([fb[0], fb[1], fb[2], fb[3]]) as u64;
                if struct_entry_off != pc_off && i > 0 {
                    // Try relative to addr
                    let alt_addr = addr + func_off;
                    if let Some(alt_fb) = self.binary.view_bytes(alt_addr, 16) {
                        let alt_entry_off =
                            u32::from_le_bytes([alt_fb[0], alt_fb[1], alt_fb[2], alt_fb[3]]) as u64;
                        if alt_entry_off == pc_off {
                            func_struct_addr = alt_addr;
                            fbytes = Some(alt_fb.to_vec());
                        }
                    }
                }
            }
            let _ = func_struct_addr;

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

    /// Analyze Go type information from runtime type descriptors
    /// Go preserves reflection data in .rodata/.data sections
    pub fn analyze_types(&self) -> Vec<GoTypeInfo> {
        let mut types = Vec::new();

        // Find .rodata or __rodata section
        let rodata_section = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == ".rodata" || s.name == "__rodata" || s.name == "__DATA_CONST");

        let Some(section) = rodata_section else {
            return types;
        };

        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let Some(data) = self
            .binary
            .view_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            return types;
        };

        // Search for type descriptors
        // Go type header: kind (1 byte) + align (1 byte) + fieldAlign (1 byte) + size (4 bytes) + ...
        // Struct type indicator: kind == 25 (reflect.Struct)
        const KIND_STRUCT: u8 = 25;

        let mut offset = 0;
        while offset + 64 < data.len() {
            // Look for potential struct type descriptor
            let kind = data[offset] & 0x1f; // Lower 5 bits are the kind

            if kind == KIND_STRUCT {
                if let Some(type_info) =
                    self.parse_go_struct_type(section.virtual_address + offset as u64, ptr_size)
                {
                    if !type_info.name.is_empty() && !type_info.fields.is_empty() {
                        types.push(type_info);
                    }
                }
            }
            offset += ptr_size;
        }

        tracing::info!("[GoAnalyzer] Found {} Go struct types", types.len());
        for ty in &types {
            tracing::info!("  - {} with {} fields", ty.name, ty.fields.len());
            for f in &ty.fields {
                tracing::info!("    - {} : {} @ offset {}", f.name, f.type_name, f.offset);
            }
        }

        types
    }

    fn parse_go_struct_type(&self, addr: u64, ptr_size: usize) -> Option<GoTypeInfo> {
        // Go type structure (simplified):
        // offset 0: kind (1 byte, masked with 0x1f)
        // offset 1: align (1 byte)
        // offset 2: fieldAlign (1 byte)
        // offset 4: size (4 bytes for 32-bit, 8 for 64-bit - depends)
        // After base type header, there's name pointer and more

        let header_size = 8 + ptr_size * 4; // Approximate header size
        let Some(data) = self.binary.view_bytes(addr, header_size + 256) else {
            return None;
        };

        let size = if ptr_size == 8 {
            u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]) as u32
        } else {
            u32::from_le_bytes([data[4], data[5], data[6], data[7]])
        };

        // Try to find struct name - it's usually in a name pointer field
        // The exact offset varies by Go version
        let name_ptr_offset = if ptr_size == 8 { 48 } else { 24 };
        if name_ptr_offset + ptr_size > data.len() {
            return None;
        }

        let name_ptr = self.read_ptr(&data, name_ptr_offset, ptr_size);
        let name = if name_ptr != 0 && name_ptr > self.binary.image_base {
            self.read_go_name(name_ptr).unwrap_or_default()
        } else {
            String::new()
        };

        // Parse struct fields
        // structType has a fields slice after the base type
        let fields = self.parse_go_struct_fields(addr, ptr_size);

        Some(GoTypeInfo { name, size, fields })
    }

    fn parse_go_struct_fields(&self, struct_type_addr: u64, ptr_size: usize) -> Vec<GoFieldInfo> {
        let mut fields = Vec::new();

        // structType layout:
        // - embedded type (rtype)
        // - pkgPath name
        // - fields slice (ptr, len, cap)

        let header_size = if ptr_size == 8 { 96 } else { 48 }; // Approximate
        let Some(header) = self.binary.view_bytes(struct_type_addr, header_size) else {
            return fields;
        };

        // Fields slice is after pkgPath
        let fields_ptr_offset = if ptr_size == 8 { 72 } else { 36 };
        if fields_ptr_offset + ptr_size * 3 > header.len() {
            return fields;
        }

        let fields_ptr = self.read_ptr(&header, fields_ptr_offset, ptr_size);
        let fields_len = self.read_ptr(&header, fields_ptr_offset + ptr_size, ptr_size) as usize;

        if fields_ptr == 0 || fields_len == 0 || fields_len > 100 {
            return fields;
        }

        // Each structField is: name, typ, offset (3 pointers worth)
        let field_size = ptr_size * 3;
        let Some(fields_data) = self.binary.get_bytes(fields_ptr, fields_len * field_size) else {
            return fields;
        };

        for i in 0..fields_len {
            let base = i * field_size;
            if base + field_size > fields_data.len() {
                break;
            }

            let name_ptr = self.read_ptr(&fields_data, base, ptr_size);
            let typ_ptr = self.read_ptr(&fields_data, base + ptr_size, ptr_size);
            let offset_val = self.read_ptr(&fields_data, base + ptr_size * 2, ptr_size);

            let name = if name_ptr != 0 {
                self.read_go_name(name_ptr)
                    .unwrap_or_else(|| format!("field_{}", i))
            } else {
                format!("field_{}", i)
            };

            let type_name = if typ_ptr != 0 {
                self.get_go_type_name(typ_ptr)
                    .unwrap_or_else(|| "unknown".to_string())
            } else {
                "unknown".to_string()
            };

            fields.push(GoFieldInfo {
                name,
                type_name,
                offset: offset_val as u32,
            });
        }

        fields
    }

    fn read_go_name(&self, addr: u64) -> Option<String> {
        // Go names are prefixed with length bytes
        let data = self.binary.get_bytes(addr, 256)?;
        if data.is_empty() {
            return None;
        }

        // First byte might be flags, second byte is length
        let name_start = if data[0] & 0x04 != 0 { 3 } else { 1 };
        let len = data[name_start - 1] as usize;

        if name_start + len > data.len() || len == 0 {
            return None;
        }

        Some(String::from_utf8_lossy(&data[name_start..name_start + len]).to_string())
    }

    fn get_go_type_name(&self, typ_ptr: u64) -> Option<String> {
        // Read the type's kind to determine type name
        let data = self.binary.get_bytes(typ_ptr, 64)?;
        let kind = data[0] & 0x1f;

        match kind {
            1 => Some("bool".to_string()),
            2 => Some("int".to_string()),
            3 => Some("int8".to_string()),
            4 => Some("int16".to_string()),
            5 => Some("int32".to_string()),
            6 => Some("int64".to_string()),
            7 => Some("uint".to_string()),
            8 => Some("uint8".to_string()),
            9 => Some("uint16".to_string()),
            10 => Some("uint32".to_string()),
            11 => Some("uint64".to_string()),
            12 => Some("uintptr".to_string()),
            13 => Some("float32".to_string()),
            14 => Some("float64".to_string()),
            15 => Some("complex64".to_string()),
            16 => Some("complex128".to_string()),
            17 => Some("array".to_string()),
            18 => Some("chan".to_string()),
            19 => Some("func".to_string()),
            20 => Some("interface".to_string()),
            21 => Some("map".to_string()),
            22 => Some("*ptr".to_string()),
            23 => Some("slice".to_string()),
            24 => Some("string".to_string()),
            25 => Some("struct".to_string()),
            26 => Some("unsafeptr".to_string()),
            _ => None,
        }
    }

    /// Scan .rodata / __rodata for inline GoString structs {*data, len}.
    ///
    /// Equivalent to Ghidra's GolangStringAnalyzer which detects non-null-
    /// terminated Go strings so decompiled code shows string content instead
    /// of raw pointer/length pairs.
    ///
    /// Returns a map of struct_address → string_content for all detected
    /// GoString instances.  Callers should register these in `global_symbols`
    /// so the decompiler can emit readable names.
    pub fn scan_go_strings(&self) -> std::collections::HashMap<u64, String> {
        let mut results = std::collections::HashMap::new();
        let image_base = self.binary.image_base;
        let is_64bit = self.binary.is_64bit;
        let ptr_size = if is_64bit { 8usize } else { 4usize };
        let struct_size = ptr_size * 2; // {ptr, len}

        for section in &self.binary.sections {
            // Only scan readable, non-executable data sections
            if section.is_executable {
                continue;
            }
            let name = section.name.as_str();
            let is_rodata = name.contains("rodata")
                || name == ".rdata"
                || name == "__rodata"
                || name == ".data";
            if !is_rodata {
                continue;
            }

            let va = section.virtual_address;
            let vsize = section.virtual_size as usize;
            if vsize < struct_size {
                continue;
            }

            let Some(data) = self.binary.view_bytes(va, vsize) else {
                continue;
            };

            let mut offset = 0usize;
            while offset + struct_size <= data.len() {
                // Read pointer and length
                let (ptr_val, len_val): (u64, u64) = if is_64bit {
                    let p =
                        u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([0; 8]));
                    let l = u64::from_le_bytes(
                        data[offset + 8..offset + 16].try_into().unwrap_or([0; 8]),
                    );
                    (p, l)
                } else {
                    let p =
                        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap_or([0; 4]))
                            as u64;
                    let l = u32::from_le_bytes(
                        data[offset + 4..offset + 8].try_into().unwrap_or([0; 4]),
                    ) as u64;
                    (p, l)
                };

                // Sanity-check: ptr must point somewhere in the binary, len reasonable
                let ptr_valid =
                    ptr_val >= image_base && ptr_val < image_base + 0x1000_0000 && ptr_val != 0;
                let len_valid = len_val >= 4 && len_val < 4096;

                if ptr_valid && len_valid {
                    // Try to read the string content
                    if let Some(str_bytes) = self.binary.get_bytes(ptr_val, len_val as usize) {
                        if let Ok(s) = std::str::from_utf8(&str_bytes) {
                            // Valid UTF-8 GoString found
                            let struct_addr = va + offset as u64;
                            let label = format!("GoStr_{:x}", struct_addr);
                            // Store as "GoStr_<addr>":"<content>" entry
                            let display = format!("\"{}\"", s.escape_default());
                            results.insert(struct_addr, display);
                            let _ = label; // will be used by caller as key
                        }
                    }
                }

                offset += ptr_size; // slide by pointer-size for overlapping scan
            }
        }

        results
    }
}

/// Go type information
#[derive(Debug, Clone)]
pub struct GoTypeInfo {
    pub name: String,
    pub size: u32,
    pub fields: Vec<GoFieldInfo>,
}

/// Go struct field information
#[derive(Debug, Clone)]
pub struct GoFieldInfo {
    pub name: String,
    pub type_name: String,
    pub offset: u32,
}

impl GoTypeInfo {
    /// Convert to InferredTypeInfo for integration with decompiler
    pub fn to_inferred_type(&self) -> crate::loader::types::InferredTypeInfo {
        crate::loader::types::InferredTypeInfo {
            name: self.name.clone(),
            mangled_name: String::new(),
            kind: "Struct".to_string(),
            fields: self
                .fields
                .iter()
                .map(|f| crate::loader::types::InferredFieldInfo {
                    name: f.name.clone(),
                    type_name: f.type_name.clone(),
                    offset: f.offset,
                    size: 0,
                })
                .collect(),
            size: self.size,
            metadata_address: 0,
        }
    }
}
