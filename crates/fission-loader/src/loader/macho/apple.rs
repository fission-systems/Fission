use crate::loader::{FunctionInfo, LoadedBinary};
use crate::prelude::*;

/// Information about a Swift class field
#[derive(Debug, Clone)]
pub struct SwiftFieldInfo {
    pub name: String,
    pub type_name: String,
    pub offset: u32,
}

/// Information about a Swift class/struct type
#[derive(Debug, Clone)]
pub struct SwiftTypeInfo {
    pub name: String,
    pub mangled_name: String,
    pub kind: SwiftTypeKind,
    pub fields: Vec<SwiftFieldInfo>,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwiftTypeKind {
    Class,
    Struct,
    Enum,
    Unknown,
}

/// Analyzer for Apple-specific metadata (Objective-C and Swift) in Mach-O binaries.
pub struct AppleAnalyzer<'a> {
    binary: &'a LoadedBinary,
}

impl<'a> AppleAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        Self { binary }
    }

    pub fn analyze(&self) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();

        // 1. Objective-C Analysis
        if let Ok(objc_funcs) = self.analyze_objc() {
            functions.extend(objc_funcs);
        }

        // 2. Swift Type Analysis (logs results for now)
        if let Ok(swift_types) = self.analyze_swift_types() {
            for ty in &swift_types {
                tracing::info!(
                    "[SwiftAnalyzer] Found type: {} ({:?}) with {} fields",
                    ty.name,
                    ty.kind,
                    ty.fields.len()
                );
                for field in &ty.fields {
                    tracing::info!(
                        "  - {} : {} @ offset {}",
                        field.name,
                        field.type_name,
                        field.offset
                    );
                }
            }
        }

        Ok(functions)
    }

    /// Analyze Swift types from metadata sections
    pub fn analyze_swift_types(&self) -> Result<Vec<SwiftTypeInfo>> {
        let mut types = Vec::new();

        // Find required sections
        let fieldmd_section = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__swift5_fieldmd");
        let reflstr_section = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__swift5_reflstr");
        let typeref_section = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__swift5_typeref");

        // Parse field names from __swift5_reflstr (null-terminated strings)
        let field_names: Vec<String> = if let Some(section) = reflstr_section {
            self.parse_null_terminated_strings(
                section.virtual_address,
                section.virtual_size as usize,
            )
        } else {
            Vec::new()
        };

        // Parse type references from __swift5_typeref
        let type_refs: Vec<String> = if let Some(section) = typeref_section {
            self.parse_null_terminated_strings(
                section.virtual_address,
                section.virtual_size as usize,
            )
        } else {
            Vec::new()
        };

        // Parse field metadata from __swift5_fieldmd
        if let Some(section) = fieldmd_section {
            if let Ok(parsed_types) = self.parse_field_metadata(
                section.virtual_address,
                section.virtual_size as usize,
                &field_names,
                &type_refs,
            ) {
                types.extend(parsed_types);
            }
        }

        Ok(types)
    }

    /// Parse null-terminated strings from a section
    fn parse_null_terminated_strings(&self, va: u64, size: usize) -> Vec<String> {
        let mut strings = Vec::new();
        let Some(data) = self.binary.get_bytes(va, size) else {
            return strings;
        };

        let mut start = 0;
        for (i, &b) in data.iter().enumerate() {
            if b == 0 {
                if i > start {
                    if let Ok(s) = std::str::from_utf8(&data[start..i]) {
                        strings.push(s.to_string());
                    }
                }
                start = i + 1;
            }
        }

        strings
    }

    /// Parse Swift5 field metadata descriptor
    ///
    /// FieldDescriptor layout (Swift 5):
    /// - i32: MangledTypeName (relative offset)
    /// - i32: Superclass (relative offset)
    /// - u16: Kind (enum, struct, class, etc.)
    /// - u16: FieldRecordSize
    /// - u32: NumFields
    /// Then followed by NumFields * FieldRecord:
    /// - u32: Flags
    /// - i32: MangledFieldTypeName (relative offset)
    /// - i32: FieldName (relative offset to __swift5_reflstr)
    fn parse_field_metadata(
        &self,
        va: u64,
        size: usize,
        field_names: &[String],
        _type_refs: &[String],
    ) -> Result<Vec<SwiftTypeInfo>> {
        let mut types = Vec::new();
        let Some(data) = self.binary.get_bytes(va, size) else {
            return Ok(types);
        };

        // Parse FieldDescriptor header
        if data.len() < 12 {
            return Ok(types);
        }

        let _mangled_type_offset = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let _superclass_offset = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let kind_raw = u16::from_le_bytes([data[8], data[9]]);
        let field_record_size = u16::from_le_bytes([data[10], data[11]]) as usize;
        let num_fields = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

        let kind = match kind_raw & 0x1F {
            0 => SwiftTypeKind::Struct,
            1 => SwiftTypeKind::Class,
            2 => SwiftTypeKind::Enum,
            _ => SwiftTypeKind::Unknown,
        };

        // Parse field records
        let mut fields = Vec::new();
        let records_start = 16;

        // Each FieldRecord is typically 12 bytes: flags(4) + typeref(4) + name_offset(4)
        let record_size = if field_record_size > 0 {
            field_record_size
        } else {
            12
        };

        for i in 0..num_fields {
            let offset = records_start + i * record_size;
            if offset + 12 > data.len() {
                break;
            }

            let _flags = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let type_ref_offset = i32::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let name_ref_offset = i32::from_le_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]);

            // Resolve field name (relative offset from current position)
            let name_va = ((va as i64) + (offset as i64) + 8 + (name_ref_offset as i64)) as u64;
            let field_name = self
                .read_string_at(name_va)
                .unwrap_or_else(|| format!("field_{}", i));

            // Resolve type name
            let type_va = ((va as i64) + (offset as i64) + 4 + (type_ref_offset as i64)) as u64;
            let type_name = self
                .read_string_at(type_va)
                .unwrap_or_else(|| "Unknown".to_string());

            fields.push(SwiftFieldInfo {
                name: field_name,
                type_name,
                offset: (i * 8 + 16) as u32, // Rough estimate: 16 bytes for object header
            });
        }

        // If we found fields, try to get the type name from field_names list
        let type_name = if !field_names.is_empty() {
            // The type name often appears before field names in reflstr
            // For now, we'll derive it from the binary name
            "SwiftType".to_string()
        } else {
            "SwiftType".to_string()
        };

        if !fields.is_empty() {
            types.push(SwiftTypeInfo {
                name: type_name,
                mangled_name: String::new(),
                kind,
                fields,
                size: 0,
            });
        }

        Ok(types)
    }

    fn read_string_at(&self, addr: u64) -> Option<String> {
        let bytes = self.binary.get_bytes(addr, 256)?;
        let mut len = 0;
        for &b in &bytes {
            if b == 0 || !b.is_ascii() {
                break;
            }
            len += 1;
        }
        if len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes[..len]).to_string())
    }

    fn analyze_objc(&self) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();

        // Find __objc_classlist section
        let Some(classlist_section) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_classlist")
        else {
            return Ok(functions);
        };

        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let Some(data) = self.binary.get_bytes(
            classlist_section.virtual_address,
            classlist_section.virtual_size as usize,
        ) else {
            return Ok(functions);
        };

        for i in 0..(data.len() / ptr_size) {
            let class_ptr = self.read_ptr(&data, i * ptr_size, ptr_size);
            if let Ok(class_funcs) = self.parse_objc_class(class_ptr) {
                functions.extend(class_funcs);
            }
        }

        Ok(functions)
    }

    fn parse_objc_class(&self, addr: u64) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        let Some(class_data) = self.binary.get_bytes(addr, 40) else {
            return Ok(functions);
        };
        let ro_data_ptr = self.read_ptr(&class_data, 32, ptr_size);

        let Some(ro_data) = self.binary.get_bytes(ro_data_ptr, 64) else {
            return Ok(functions);
        };
        let class_name_ptr = self.read_ptr(&ro_data, 24, ptr_size);
        let class_name = self
            .read_string(class_name_ptr)
            .unwrap_or_else(|| "Unknown".to_string());

        let base_methods_ptr = self.read_ptr(&ro_data, 32, ptr_size);
        if base_methods_ptr != 0 {
            if let Ok(method_funcs) = self.parse_objc_method_list(base_methods_ptr, &class_name) {
                functions.extend(method_funcs);
            }
        }

        Ok(functions)
    }

    /// Analyze Objective-C classes and extract ivar (instance variable) information
    pub fn analyze_objc_ivars(&self) -> Vec<ObjCClassInfo> {
        let mut classes = Vec::new();

        // Find __objc_classlist section
        let Some(classlist_section) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_classlist")
        else {
            return classes;
        };

        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let Some(data) = self.binary.get_bytes(
            classlist_section.virtual_address,
            classlist_section.virtual_size as usize,
        ) else {
            return classes;
        };

        for i in 0..(data.len() / ptr_size) {
            let class_ptr = self.read_ptr(&data, i * ptr_size, ptr_size);
            if let Some(class_info) = self.parse_objc_class_ivars(class_ptr) {
                if !class_info.ivars.is_empty() {
                    tracing::info!(
                        "[ObjCAnalyzer] Found class: {} with {} ivars",
                        class_info.name,
                        class_info.ivars.len()
                    );
                    for ivar in &class_info.ivars {
                        tracing::info!(
                            "  - {} : {} @ offset {}",
                            ivar.name,
                            ivar.type_encoding,
                            ivar.offset
                        );
                    }
                    classes.push(class_info);
                }
            }
        }

        classes
    }

    fn parse_objc_class_ivars(&self, addr: u64) -> Option<ObjCClassInfo> {
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        // class_t structure:
        // 0: isa
        // 8: superclass
        // 16: cache
        // 24: vtable
        // 32: data (class_ro_t pointer)
        let class_data = self.binary.get_bytes(addr, 40)?;
        let ro_data_ptr = self.read_ptr(&class_data, 32, ptr_size);

        // Mask off the Swift bit if present (lower bits of pointer)
        let ro_data_ptr = ro_data_ptr & !0x7;

        let ro_data = self.binary.get_bytes(ro_data_ptr, 80)?;

        // class_ro_t structure (64-bit):
        // 0: flags (4 bytes)
        // 4: instanceStart (4 bytes)
        // 8: instanceSize (4 bytes)
        // 12: reserved (4 bytes)
        // 16: ivarLayout
        // 24: name
        // 32: baseMethods
        // 40: baseProtocols
        // 48: ivars
        // 56: weakIvarLayout
        // 64: baseProperties

        let class_name_ptr = self.read_ptr(&ro_data, 24, ptr_size);
        let class_name = self.read_string(class_name_ptr)?;

        let ivars_ptr = self.read_ptr(&ro_data, 48, ptr_size);

        let ivars = if ivars_ptr != 0 {
            self.parse_objc_ivar_list(ivars_ptr)
        } else {
            Vec::new()
        };

        Some(ObjCClassInfo {
            name: class_name,
            ivars,
        })
    }

    fn parse_objc_ivar_list(&self, addr: u64) -> Vec<ObjCIvarInfo> {
        let mut ivars = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        // ivar_list_t structure:
        // 0: entsize (4 bytes)
        // 4: count (4 bytes)
        // 8: first ivar
        let header = self.binary.get_bytes(addr, 8);
        let Some(header) = header else {
            return ivars;
        };

        let entsize = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) & 0xFFFC;
        let count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if count == 0 || count > 1000 || entsize < 8 {
            return ivars;
        }

        let ivar_data = self
            .binary
            .get_bytes(addr + 8, (count as usize) * (entsize as usize));
        let Some(ivar_data) = ivar_data else {
            return ivars;
        };

        // ivar_t structure (64-bit):
        // 0: offset (pointer to int32)
        // 8: name (pointer to string)
        // 16: type (pointer to type encoding string)
        // 24: alignment_raw (uint32)
        // 28: size (uint32)

        for i in 0..count as usize {
            let base = i * entsize as usize;
            if base + 24 > ivar_data.len() {
                break;
            }

            let offset_ptr = self.read_ptr(&ivar_data, base, ptr_size);
            let name_ptr = self.read_ptr(&ivar_data, base + ptr_size, ptr_size);
            let type_ptr = self.read_ptr(&ivar_data, base + ptr_size * 2, ptr_size);

            // Read the actual offset value (it's a pointer to an int32)
            let offset = if let Some(offset_bytes) = self.binary.get_bytes(offset_ptr, 4) {
                u32::from_le_bytes([
                    offset_bytes[0],
                    offset_bytes[1],
                    offset_bytes[2],
                    offset_bytes[3],
                ])
            } else {
                0
            };

            let name = self
                .read_string(name_ptr)
                .unwrap_or_else(|| format!("ivar_{}", i));
            let type_encoding = self
                .read_string(type_ptr)
                .unwrap_or_else(|| "?".to_string());

            // Decode type encoding to readable type name
            let type_name = decode_objc_type(&type_encoding);

            ivars.push(ObjCIvarInfo {
                name,
                type_encoding,
                type_name,
                offset,
            });
        }

        ivars
    }

    fn parse_objc_method_list(&self, addr: u64, class_name: &str) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();

        let Some(list_header) = self.binary.get_bytes(addr, 8) else {
            return Ok(functions);
        };
        let entsize = u32::from_le_bytes([
            list_header[0],
            list_header[1],
            list_header[2],
            list_header[3],
        ]) & 0xFFFC;
        let count = u32::from_le_bytes([
            list_header[4],
            list_header[5],
            list_header[6],
            list_header[7],
        ]);

        let Some(method_data) = self.binary.get_bytes(addr + 8, (count * entsize) as usize) else {
            return Ok(functions);
        };

        for i in 0..count as usize {
            let m_off = i * entsize as usize;

            if entsize >= 24 {
                let name_ptr = self.read_ptr(&method_data, m_off, 8);
                let imp_ptr = self.read_ptr(&method_data, m_off + 16, 8);

                if let Some(sel_name) = self.read_string(name_ptr) {
                    functions.push(FunctionInfo {
                        name: format!("-[{} {}]", class_name, sel_name),
                        address: imp_ptr,
                        size: 0,
                        is_export: false,
                        is_import: false,
                    });
                }
            }
        }

        Ok(functions)
    }

    /// Resolve ObjC `objc_msgSend` selector references.
    ///
    /// Ghidra's `ObjectiveC2_MessageAnalyzer` does the same: it finds all
    /// `__objc_selrefs` slots (each is a pointer into `__objc_methnames`)
    /// and annotates them with the resolved selector name so the decompiler
    /// can show `objc_msgSend(self, "viewDidLoad")` instead of raw pointers.
    ///
    /// Returns `selref_va → "sel_<name>"` for insertion into `global_symbols`.
    pub fn resolve_msg_send_selectors(&self) -> std::collections::HashMap<u64, String> {
        let mut result = std::collections::HashMap::new();

        let ptr_size = if self.binary.is_64bit { 8usize } else { 4usize };

        // ── Step 1: build methname_va → selector_name from __objc_methnames ──
        // The section is a blob of null-terminated C strings packed back-to-back.
        let Some(methnames_sec) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_methnames")
        else {
            return result;
        };

        let Some(methnames_data) = self.binary.get_bytes(
            methnames_sec.virtual_address,
            methnames_sec.virtual_size as usize,
        ) else {
            return result;
        };

        let mut name_map: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
        let mut offset = 0usize;
        while offset < methnames_data.len() {
            // Find the end of this null-terminated string
            let start = offset;
            while offset < methnames_data.len() && methnames_data[offset] != 0 {
                offset += 1;
            }
            if offset > start {
                if let Ok(name) = std::str::from_utf8(&methnames_data[start..offset]) {
                    let va = methnames_sec.virtual_address + start as u64;
                    name_map.insert(va, name.to_string());
                }
            }
            offset += 1; // skip the null terminator
        }

        if name_map.is_empty() {
            return result;
        }

        // ── Step 2: scan __objc_selrefs (array of pointers into methnames) ──
        let Some(selrefs_sec) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_selrefs")
        else {
            return result;
        };

        let Some(selrefs_data) = self.binary.get_bytes(
            selrefs_sec.virtual_address,
            selrefs_sec.virtual_size as usize,
        ) else {
            return result;
        };

        let num_entries = selrefs_data.len() / ptr_size;
        for i in 0..num_entries {
            let selref_va = selrefs_sec.virtual_address + (i * ptr_size) as u64;
            let target_va = self.read_ptr(&selrefs_data, i * ptr_size, ptr_size);
            if target_va == 0 {
                continue;
            }
            // Look for an exact match or any entry that starts at/before target_va
            // with the string spanning over it (handles partial offsets)
            if let Some(name) = name_map.get(&target_va) {
                let symbol_name = format!("sel_{}", name);
                result.insert(selref_va, symbol_name);
            } else {
                // Fall back: walk backwards to find the string that contains target_va
                // (target_va may point into the middle of a string — rare but possible)
                for (&str_va, name) in &name_map {
                    if str_va <= target_va && target_va < str_va + name.len() as u64 + 1 {
                        let symbol_name = format!("sel_{}", name);
                        result.insert(selref_va, symbol_name);
                        break;
                    }
                }
            }
        }

        tracing::debug!(
            "[ObjCAnalyzer] resolve_msgSend_selectors: {} selrefs → {} resolved",
            num_entries,
            result.len()
        );

        result
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
        let bytes = self.binary.get_bytes(addr, 512)?;
        let mut len = 0;
        for &b in &bytes {
            if b == 0 {
                break;
            }
            len += 1;
        }
        if len == 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes[..len]).to_string())
    }
}

/// Objective-C class information
#[derive(Debug, Clone)]
pub struct ObjCClassInfo {
    pub name: String,
    pub ivars: Vec<ObjCIvarInfo>,
}

/// Objective-C instance variable information
#[derive(Debug, Clone)]
pub struct ObjCIvarInfo {
    pub name: String,
    pub type_encoding: String,
    pub type_name: String,
    pub offset: u32,
}

/// Decode Objective-C type encoding to readable type name
fn decode_objc_type(encoding: &str) -> String {
    if encoding.is_empty() {
        return "id".to_string();
    }

    // Safe: We just checked encoding is not empty
    let first_char = encoding.chars().next().unwrap_or('@');
    match first_char {
        'c' => "char".to_string(),
        'i' => "int".to_string(),
        's' => "short".to_string(),
        'l' => "long".to_string(),
        'q' => "long long".to_string(),
        'C' => "unsigned char".to_string(),
        'I' => "unsigned int".to_string(),
        'S' => "unsigned short".to_string(),
        'L' => "unsigned long".to_string(),
        'Q' => "unsigned long long".to_string(),
        'f' => "float".to_string(),
        'd' => "double".to_string(),
        'B' => "BOOL".to_string(),
        'v' => "void".to_string(),
        '*' => "char*".to_string(),
        '@' => {
            // Object type - try to extract class name
            if encoding.len() > 2 && encoding.starts_with("@\"") {
                let end = encoding[2..].find('"').unwrap_or(encoding.len() - 2);
                encoding[2..2 + end].to_string() + "*"
            } else {
                "id".to_string()
            }
        }
        '#' => "Class".to_string(),
        ':' => "SEL".to_string(),
        '^' => {
            // Pointer type
            if encoding.len() > 1 {
                format!("{}*", decode_objc_type(&encoding[1..]))
            } else {
                "void*".to_string()
            }
        }
        '{' => {
            // Struct - extract name
            if let Some(end) = encoding.find('=') {
                encoding[1..end].to_string()
            } else if let Some(end) = encoding.find('}') {
                encoding[1..end].to_string()
            } else {
                "struct".to_string()
            }
        }
        _ => encoding.to_string(),
    }
}

impl ObjCClassInfo {
    /// Convert to InferredTypeInfo for integration with decompiler
    pub fn to_inferred_type(&self) -> crate::loader::types::InferredTypeInfo {
        crate::loader::types::InferredTypeInfo {
            name: self.name.clone(),
            mangled_name: String::new(),
            kind: "ObjCClass".to_string(),
            fields: self
                .ivars
                .iter()
                .map(|ivar| crate::loader::types::InferredFieldInfo {
                    name: ivar.name.clone(),
                    type_name: ivar.type_name.clone(),
                    offset: ivar.offset,
                    size: 0,
                })
                .collect(),
            size: 0,
            metadata_address: 0,
        }
    }
}
