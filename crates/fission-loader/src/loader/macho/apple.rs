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
