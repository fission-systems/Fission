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

/// Field offsets in the Objective-C 2 runtime structures this reads, which
/// depend on the pointer size.
///
/// ```text
/// class_t      isa, superclass, cache, vtable, data          (five pointers)
/// class_ro_t   flags, instanceStart, instanceSize            (uint32 each)
///              reserved                                      (uint32, 64-bit only)
///              ivarLayout, name, baseMethods, baseProtocols,
///              ivars, weakIvarLayout, baseProperties         (pointers)
/// ```
///
/// The 64-bit offsets used to be applied to every image, so a 32-bit one
/// (armv7, i386, arm64_32) had its class names, methods and ivars read from
/// the wrong fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObjcLayout {
    /// `class_t::data`.
    class_data: usize,
    /// `class_ro_t::name`.
    ro_name: usize,
    /// `class_ro_t::baseMethods`.
    ro_base_methods: usize,
    /// `class_ro_t::ivars`.
    ro_ivars: usize,
}

impl ObjcLayout {
    fn for_pointer_size(ptr_size: usize) -> Self {
        // The three uint32 fields, padded to a pointer boundary by `reserved`
        // where pointers are eight bytes.
        let ro_pointers = if ptr_size == 8 { 16 } else { 12 };
        Self {
            class_data: 4 * ptr_size,
            ro_name: ro_pointers + ptr_size,
            ro_base_methods: ro_pointers + 2 * ptr_size,
            ro_ivars: ro_pointers + 4 * ptr_size,
        }
    }
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
        _field_names: &[String],
        _type_refs: &[String],
    ) -> Result<Vec<SwiftTypeInfo>> {
        let mut types = Vec::new();
        let Some(data) = self.binary.get_bytes(va, size) else {
            return Ok(types);
        };

        let mut offset = 0usize;
        while offset + 16 <= data.len() {
            let desc_offset = offset;
            let mangled_type_offset = i32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let _superclass_offset = i32::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let kind_raw = u16::from_le_bytes([data[offset + 8], data[offset + 9]]);
            let field_record_size =
                u16::from_le_bytes([data[offset + 10], data[offset + 11]]) as usize;
            let num_fields = u32::from_le_bytes([
                data[offset + 12],
                data[offset + 13],
                data[offset + 14],
                data[offset + 15],
            ]) as usize;

            let kind = match kind_raw & 0x1F {
                0 => SwiftTypeKind::Struct,
                1 => SwiftTypeKind::Class,
                2 => SwiftTypeKind::Enum,
                _ => SwiftTypeKind::Unknown,
            };

            let record_size = if field_record_size > 0 {
                field_record_size
            } else {
                12
            };

            let fields_end = desc_offset + 16 + num_fields * record_size;
            if fields_end > data.len() {
                break;
            }

            let mut fields = Vec::new();
            let records_start = desc_offset + 16;

            for i in 0..num_fields {
                let rec_offset = records_start + i * record_size;
                if rec_offset + 12 > data.len() {
                    break;
                }

                let _flags = u32::from_le_bytes([
                    data[rec_offset],
                    data[rec_offset + 1],
                    data[rec_offset + 2],
                    data[rec_offset + 3],
                ]);
                let type_ref_offset = i32::from_le_bytes([
                    data[rec_offset + 4],
                    data[rec_offset + 5],
                    data[rec_offset + 6],
                    data[rec_offset + 7],
                ]);
                let name_ref_offset = i32::from_le_bytes([
                    data[rec_offset + 8],
                    data[rec_offset + 9],
                    data[rec_offset + 10],
                    data[rec_offset + 11],
                ]);

                // Resolve field name (relative offset from current position)
                let name_va = va
                    .wrapping_add(rec_offset as u64 + 8)
                    .wrapping_add_signed(i64::from(name_ref_offset));
                let field_name = self
                    .read_string_at(name_va)
                    .unwrap_or_else(|| format!("field_{}", i));

                // Resolve type name
                let type_va = va
                    .wrapping_add(rec_offset as u64 + 4)
                    .wrapping_add_signed(i64::from(type_ref_offset));
                let type_name = self
                    .read_string_at(type_va)
                    .unwrap_or_else(|| "Unknown".to_string());

                fields.push(SwiftFieldInfo {
                    name: field_name,
                    type_name,
                    offset: (i * 8 + 16) as u32,
                });
            }

            // Resolve nominal type name from mangled_type_offset
            let mut type_name = String::new();
            if mangled_type_offset != 0 {
                let mangled_type_va = va
                    .wrapping_add(desc_offset as u64)
                    .wrapping_add_signed(i64::from(mangled_type_offset));
                if let Some(mangled) = self.read_string_at(mangled_type_va) {
                    type_name = crate::loader::demangle::demangle(&mangled);
                }
            }

            if type_name.is_empty() {
                type_name = format!("SwiftType_{:x}", va.wrapping_add(desc_offset as u64));
            }

            types.push(SwiftTypeInfo {
                name: type_name,
                mangled_name: String::new(),
                kind,
                fields,
                size: 0,
            });

            offset = fields_end;
            // Pad offset to 4-byte boundary since FieldDescriptors are typically 4-byte aligned
            offset = (offset + 3) & !3;
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
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        // 1. Parse standard classes from __objc_classlist
        if let Some(classlist_section) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_classlist")
        {
            if let Some(data) = self.binary.get_bytes(
                classlist_section.virtual_address,
                classlist_section.virtual_size as usize,
            ) {
                for i in 0..(data.len() / ptr_size) {
                    let class_ptr = self.read_ptr(&data, i * ptr_size, ptr_size);
                    if let Ok(class_funcs) = self.parse_objc_class(class_ptr) {
                        functions.extend(class_funcs);
                    }
                }
            }
        }

        // 2. Parse categories from __objc_catlist
        if let Some(catlist_section) = self
            .binary
            .sections
            .iter()
            .find(|s| s.name == "__objc_catlist")
        {
            if let Some(data) = self.binary.get_bytes(
                catlist_section.virtual_address,
                catlist_section.virtual_size as usize,
            ) {
                for i in 0..(data.len() / ptr_size) {
                    let cat_ptr = self.read_ptr(&data, i * ptr_size, ptr_size);
                    if let Ok(cat_funcs) = self.parse_objc_category(cat_ptr) {
                        functions.extend(cat_funcs);
                    }
                }
            }
        }

        Ok(functions)
    }

    fn parse_objc_class(&self, addr: u64) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let layout = ObjcLayout::for_pointer_size(ptr_size);

        let Some(class_data) = self.binary.get_bytes(addr, 40) else {
            return Ok(functions);
        };
        let isa_ptr = self.read_ptr(&class_data, 0, ptr_size) & !0x7;
        let ro_data_ptr = self.read_ptr(&class_data, layout.class_data, ptr_size) & !0x7;

        let Some(ro_data) = self.binary.get_bytes(ro_data_ptr, 64) else {
            return Ok(functions);
        };
        let class_name_ptr = self.read_ptr(&ro_data, layout.ro_name, ptr_size);
        let class_name = self
            .read_string(class_name_ptr)
            .unwrap_or_else(|| "Unknown".to_string());

        // Parse instance methods
        let base_methods_ptr = self.read_ptr(&ro_data, layout.ro_base_methods, ptr_size);
        if base_methods_ptr != 0 {
            if let Ok(method_funcs) =
                self.parse_objc_method_list(base_methods_ptr, &class_name, false)
            {
                functions.extend(method_funcs);
            }
        }

        // Parse class methods from metaclass
        if isa_ptr != 0 && isa_ptr != addr {
            if let Some(metaclass_data) = self.binary.get_bytes(isa_ptr, 40) {
                let meta_ro_data_ptr =
                    self.read_ptr(&metaclass_data, layout.class_data, ptr_size) & !0x7;
                if meta_ro_data_ptr != 0 {
                    if let Some(meta_ro_data) = self.binary.get_bytes(meta_ro_data_ptr, 64) {
                        let meta_methods_ptr =
                            self.read_ptr(&meta_ro_data, layout.ro_base_methods, ptr_size);
                        if meta_methods_ptr != 0 {
                            if let Ok(method_funcs) =
                                self.parse_objc_method_list(meta_methods_ptr, &class_name, true)
                            {
                                functions.extend(method_funcs);
                            }
                        }
                    }
                }
            }
        }

        Ok(functions)
    }

    fn parse_objc_category(&self, addr: u64) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let layout = ObjcLayout::for_pointer_size(ptr_size);

        let Some(cat_data) = self.binary.get_bytes(addr, 48) else {
            return Ok(functions);
        };

        let cat_name_ptr = self.read_ptr(&cat_data, 0, ptr_size);
        let class_ptr = self.read_ptr(&cat_data, ptr_size, ptr_size);
        let instance_methods_ptr = self.read_ptr(&cat_data, ptr_size * 2, ptr_size);
        let class_methods_ptr = self.read_ptr(&cat_data, ptr_size * 3, ptr_size);

        // Get class name
        let mut class_name = String::new();
        if class_ptr != 0 {
            if let Some(class_data) = self.binary.get_bytes(class_ptr, 40) {
                let ro_data_ptr = self.read_ptr(&class_data, layout.class_data, ptr_size) & !0x7;
                if let Some(ro_data) = self.binary.get_bytes(ro_data_ptr, 64) {
                    let class_name_ptr = self.read_ptr(&ro_data, layout.ro_name, ptr_size);
                    if let Some(name) = self.read_string(class_name_ptr) {
                        class_name = name;
                    }
                }
            }
        }
        if class_name.is_empty() {
            class_name = "UnknownClass".to_string();
        }

        // Get category name
        let cat_name = self
            .read_string(cat_name_ptr)
            .unwrap_or_else(|| "Category".to_string());
        let class_and_cat = format!("{}({})", class_name, cat_name);

        if instance_methods_ptr != 0 {
            if let Ok(method_funcs) =
                self.parse_objc_method_list(instance_methods_ptr, &class_and_cat, false)
            {
                functions.extend(method_funcs);
            }
        }

        if class_methods_ptr != 0 {
            if let Ok(method_funcs) =
                self.parse_objc_method_list(class_methods_ptr, &class_and_cat, true)
            {
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
        let layout = ObjcLayout::for_pointer_size(ptr_size);

        // class_t is five pointers -- isa, superclass, cache, vtable, data --
        // so `data` (the class_ro_t pointer) is at 32 or at 16.
        let class_data = self.binary.get_bytes(addr, 40)?;
        let ro_data_ptr = self.read_ptr(&class_data, layout.class_data, ptr_size);

        // Mask off the Swift bit if present (lower bits of pointer)
        let ro_data_ptr = ro_data_ptr & !0x7;

        let ro_data = self.binary.get_bytes(ro_data_ptr, 80)?;

        // class_ro_t: see `ObjcLayout`.
        let class_name_ptr = self.read_ptr(&ro_data, layout.ro_name, ptr_size);
        let class_name = self.read_string(class_name_ptr)?;

        let ivars_ptr = self.read_ptr(&ro_data, layout.ro_ivars, ptr_size);

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

        let ivar_data = addr.checked_add(8).and_then(|first| {
            self.binary
                .get_bytes(first, (count as usize) * (entsize as usize))
        });
        let Some(ivar_data) = ivar_data else {
            return ivars;
        };

        // ivar_t is three pointers -- offset (to an int32), name, type --
        // then alignment_raw and size (uint32 each): 32 bytes, or 20. Only the
        // pointers are read, so that is all an entry must have room for; the
        // check used to demand 24 bytes whatever the pointer size, and the
        // last 20-byte entry of every 32-bit list failed it.
        for i in 0..count as usize {
            let base = i * entsize as usize;
            if base + ptr_size * 3 > ivar_data.len() {
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

    fn parse_objc_method_list(
        &self,
        addr: u64,
        class_name: &str,
        is_class_method: bool,
    ) -> Result<Vec<FunctionInfo>> {
        let mut functions = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };
        let prefix = if is_class_method { "+" } else { "-" };

        let Some(list_header) = self.binary.get_bytes(addr, 8) else {
            return Ok(functions);
        };
        let entsize_raw = u32::from_le_bytes([
            list_header[0],
            list_header[1],
            list_header[2],
            list_header[3],
        ]);
        let flags = entsize_raw & 0xFFFF_0000;
        let entsize = (entsize_raw & 0xFFFF) & 0xFFFC;
        let count = u32::from_le_bytes([
            list_header[4],
            list_header[5],
            list_header[6],
            list_header[7],
        ]);

        if count == 0 || count > 10000 || entsize == 0 {
            return Ok(functions);
        }

        let Some(first_method) = addr.checked_add(8) else {
            return Ok(functions);
        };
        let Some(method_data) = self
            .binary
            .get_bytes(first_method, (count * entsize) as usize)
        else {
            return Ok(functions);
        };

        // A relative ("small") method list is three 32-bit displacements and
        // says so in the top bit of the entsize word. Twelve bytes without the
        // flag is still taken as relative where pointers are eight bytes -- it
        // cannot be three of those -- but in a 32-bit image twelve bytes is
        // exactly three absolute pointers, and reading them as displacements
        // lost every method.
        let is_relative = (flags & 0x8000_0000) != 0 || (entsize == 12 && ptr_size == 8);

        for i in 0..count as usize {
            let m_off = i * entsize as usize;

            if is_relative {
                if m_off + 12 > method_data.len() {
                    break;
                }
                let name_rel = i32::from_le_bytes([
                    method_data[m_off],
                    method_data[m_off + 1],
                    method_data[m_off + 2],
                    method_data[m_off + 3],
                ]);
                let imp_rel = i32::from_le_bytes([
                    method_data[m_off + 8],
                    method_data[m_off + 9],
                    method_data[m_off + 10],
                    method_data[m_off + 11],
                ]);

                // Relative references: the field's own address plus a signed
                // 32-bit displacement, wrapping like the pointer arithmetic
                // it stands for.
                let entry_va = first_method.wrapping_add(m_off as u64);
                let name_va = entry_va.wrapping_add_signed(i64::from(name_rel));
                let imp_va = entry_va
                    .wrapping_add(8)
                    .wrapping_add_signed(i64::from(imp_rel));

                if let Some(sel_name) = self.read_string(name_va) {
                    functions.push(FunctionInfo {
                        name: format!("{}[{} {}]", prefix, class_name, sel_name),
                        address: imp_va,
                        size: 0,
                        is_export: false,
                        is_import: false,
                        ..Default::default()
                    });
                }
            } else if entsize >= ptr_size as u32 * 3 {
                if m_off + (ptr_size * 3) > method_data.len() {
                    break;
                }
                let name_ptr = self.read_ptr(&method_data, m_off, ptr_size);
                let imp_ptr = self.read_ptr(&method_data, m_off + ptr_size * 2, ptr_size);

                if let Some(sel_name) = self.read_string(name_ptr) {
                    functions.push(FunctionInfo {
                        name: format!("{}[{} {}]", prefix, class_name, sel_name),
                        address: imp_ptr,
                        size: 0,
                        is_export: false,
                        is_import: false,
                        ..Default::default()
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

        let mut name_map: std::collections::BTreeMap<u64, String> =
            std::collections::BTreeMap::new();
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
            } else if let Some((&str_va, name)) = name_map.range(..=target_va).next_back() {
                if target_va < str_va + name.len() as u64 + 1 {
                    let symbol_name = format!("sel_{}", name);
                    result.insert(selref_va, symbol_name);
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
            source: "objc".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    const BLOB: u64 = 0x1000;
    const CLASSLIST: u64 = 0x2000;

    /// One Objective-C class, `Widget`, laid out for the given pointer size:
    /// an instance method `draw` at 0x3000, a class method `shared` at
    /// 0x3010, and two `int` ivars, `_width` at offset 8 and `_height` at 12.
    fn objc_image(ptr_size: usize) -> LoadedBinary {
        const CLASS: u64 = 0x1000;
        const META: u64 = 0x1040;
        const RO: u64 = 0x1080;
        const META_RO: u64 = 0x10c0;
        const METHODS: u64 = 0x1100;
        const CLASS_METHODS: u64 = 0x1140;
        const IVARS: u64 = 0x1180;
        const NAME: u64 = 0x1300;
        const SEL_DRAW: u64 = 0x1310;
        const SEL_SHARED: u64 = 0x1320;
        const WIDTH_OFFSET: u64 = 0x1340;
        const HEIGHT_OFFSET: u64 = 0x1344;
        const IVAR_WIDTH: u64 = 0x1350;
        const IVAR_HEIGHT: u64 = 0x1358;
        const TYPE_INT: u64 = 0x1368;

        let mut data = vec![0u8; 0x2100];
        let ptr = |data: &mut Vec<u8>, at: u64, value: u64| {
            let at = at as usize;
            data[at..at + ptr_size].copy_from_slice(&value.to_le_bytes()[..ptr_size]);
        };
        let word = |data: &mut Vec<u8>, at: u64, value: u32| {
            let at = at as usize;
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        let text = |data: &mut Vec<u8>, at: u64, value: &str| {
            let at = at as usize;
            data[at..at + value.len()].copy_from_slice(value.as_bytes());
        };
        let p = ptr_size as u64;
        // class_ro_t: three uint32s (and `reserved` on 64-bit), then pointers.
        let ro_pointers = if ptr_size == 8 { 16 } else { 12 };

        // class_t: isa, superclass, cache, vtable, data.
        ptr(&mut data, CLASS, META);
        ptr(&mut data, CLASS + 4 * p, RO);
        ptr(&mut data, META + 4 * p, META_RO);
        // class_ro_t: ivarLayout, name, baseMethods, baseProtocols, ivars.
        ptr(&mut data, RO + ro_pointers + p, NAME);
        ptr(&mut data, RO + ro_pointers + 2 * p, METHODS);
        ptr(&mut data, RO + ro_pointers + 4 * p, IVARS);
        ptr(&mut data, META_RO + ro_pointers + p, NAME);
        ptr(&mut data, META_RO + ro_pointers + 2 * p, CLASS_METHODS);

        // method_list_t: entsize, count, then (name, types, imp).
        for (list, selector, imp) in [
            (METHODS, SEL_DRAW, 0x3000u64),
            (CLASS_METHODS, SEL_SHARED, 0x3010),
        ] {
            word(&mut data, list, 3 * ptr_size as u32);
            word(&mut data, list + 4, 1);
            ptr(&mut data, list + 8, selector);
            ptr(&mut data, list + 8 + 2 * p, imp);
        }

        // ivar_list_t: entsize, count, then (offset*, name, type, align, size).
        let ivar_size = 3 * p + 8;
        word(&mut data, IVARS, ivar_size as u32);
        word(&mut data, IVARS + 4, 2);
        for (index, (offset, name)) in [(WIDTH_OFFSET, IVAR_WIDTH), (HEIGHT_OFFSET, IVAR_HEIGHT)]
            .into_iter()
            .enumerate()
        {
            let entry = IVARS + 8 + index as u64 * ivar_size;
            ptr(&mut data, entry, offset);
            ptr(&mut data, entry + p, name);
            ptr(&mut data, entry + 2 * p, TYPE_INT);
        }
        word(&mut data, WIDTH_OFFSET, 8);
        word(&mut data, HEIGHT_OFFSET, 12);

        text(&mut data, NAME, "Widget");
        text(&mut data, SEL_DRAW, "draw");
        text(&mut data, SEL_SHARED, "shared");
        text(&mut data, IVAR_WIDTH, "_width");
        text(&mut data, IVAR_HEIGHT, "_height");
        text(&mut data, TYPE_INT, "i");

        ptr(&mut data, CLASSLIST, CLASS);

        let section = |name: &str, address: u64, size: u64| SectionInfo {
            name: name.to_string(),
            virtual_address: address,
            virtual_size: size,
            file_offset: address,
            file_size: size,
            is_executable: false,
            is_readable: true,
            is_writable: false,
        };
        LoadedBinaryBuilder::new("objc".to_string(), DataBuffer::Heap(data))
            .is_64bit(ptr_size == 8)
            // The ivar list ends where the section does, as it does when the
            // linker packs `__objc_const`: nothing after it to read by luck.
            .add_section(section(
                "__objc_const",
                BLOB,
                IVARS + 8 + 2 * ivar_size - BLOB,
            ))
            .add_section(section("__objc_strings", NAME, 0x100))
            .add_section(section("__objc_classlist", CLASSLIST, p))
            .build()
            .expect("objc image")
    }

    fn assert_widget(ptr_size: usize) {
        let binary = objc_image(ptr_size);
        let analyzer = AppleAnalyzer::new(&binary);

        let mut methods: Vec<(String, u64)> = analyzer
            .analyze()
            .expect("analyze")
            .into_iter()
            .map(|f| (f.name, f.address))
            .collect();
        methods.sort();
        assert_eq!(
            methods,
            vec![
                ("+[Widget shared]".to_string(), 0x3010),
                ("-[Widget draw]".to_string(), 0x3000),
            ],
            "{ptr_size}-byte pointers"
        );

        let classes = analyzer.analyze_objc_ivars();
        assert_eq!(classes.len(), 1, "{ptr_size}-byte pointers: {classes:?}");
        assert_eq!(classes[0].name, "Widget");
        let ivars: Vec<(&str, &str, u32)> = classes[0]
            .ivars
            .iter()
            .map(|ivar| (ivar.name.as_str(), ivar.type_name.as_str(), ivar.offset))
            .collect();
        assert_eq!(
            ivars,
            vec![("_width", "int", 8), ("_height", "int", 12)],
            "{ptr_size}-byte pointers"
        );
    }

    #[test]
    fn objc_class_metadata_is_read_with_64_bit_offsets() {
        assert_widget(8);
    }

    /// A 32-bit image (armv7, i386, arm64_32) keeps `class_t::data` at 16 and
    /// `class_ro_t`'s pointers from 12; the 64-bit offsets were used for both,
    /// and the last 20-byte ivar of a list failed a 24-byte bounds check.
    #[test]
    fn objc_class_metadata_is_read_with_32_bit_offsets() {
        assert_widget(4);
    }

    #[test]
    fn objc_layout_matches_the_runtime_structures() {
        assert_eq!(
            ObjcLayout::for_pointer_size(8),
            ObjcLayout {
                class_data: 32,
                ro_name: 24,
                ro_base_methods: 32,
                ro_ivars: 48,
            }
        );
        assert_eq!(
            ObjcLayout::for_pointer_size(4),
            ObjcLayout {
                class_data: 16,
                ro_name: 16,
                ro_base_methods: 20,
                ro_ivars: 28,
            }
        );
    }

    /// One Swift field descriptor -- a struct with a single field -- in a
    /// `__swift5_fieldmd` section at `fieldmd`, its names in a section that
    /// follows it.
    fn swift_image(fieldmd: u64) -> LoadedBinary {
        let mut data = vec![0u8; 0x80];
        // Descriptor: mangled name (+0x40), superclass, kind = struct,
        // record size 12, one field.
        data[0..4].copy_from_slice(&0x40i32.to_le_bytes());
        data[8..10].copy_from_slice(&0u16.to_le_bytes());
        data[10..12].copy_from_slice(&12u16.to_le_bytes());
        data[12..16].copy_from_slice(&1u32.to_le_bytes());
        // Field record at +16: flags, type name (from +20), field name (from +24).
        data[20..24].copy_from_slice(&(0x50i32 - 20).to_le_bytes());
        data[24..28].copy_from_slice(&(0x60i32 - 24).to_le_bytes());
        data[0x40..0x46].copy_from_slice(b"Widget");
        data[0x50..0x52].copy_from_slice(b"Si");
        data[0x60..0x65].copy_from_slice(b"width");

        let section = |name: &str, address: u64, offset: u64, size: u64| SectionInfo {
            name: name.to_string(),
            virtual_address: address,
            virtual_size: size,
            file_offset: offset,
            file_size: size,
            is_executable: false,
            is_readable: true,
            is_writable: false,
        };
        LoadedBinaryBuilder::new("swift".to_string(), DataBuffer::Heap(data))
            .is_64bit(true)
            .add_section(section("__swift5_fieldmd", fieldmd, 0, 0x1c))
            .add_section(section(
                "__swift5_reflstr",
                fieldmd.wrapping_add(0x40),
                0x40,
                0x40,
            ))
            .build()
            .expect("swift image")
    }

    fn assert_swift_field(fieldmd: u64) {
        let binary = swift_image(fieldmd);
        let types = AppleAnalyzer::new(&binary)
            .analyze_swift_types()
            .expect("swift types");
        assert_eq!(types.len(), 1, "{fieldmd:#x}: {types:?}");
        assert_eq!(types[0].kind, SwiftTypeKind::Struct);
        let fields: Vec<(&str, &str)> = types[0]
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.type_name.as_str()))
            .collect();
        assert_eq!(fields, vec![("width", "Si")], "{fieldmd:#x}");
    }

    #[test]
    fn swift_field_descriptor_resolves_its_relative_names() {
        assert_swift_field(0x1_0000_0000);
    }

    /// The relative references were resolved through `i64`, so a descriptor
    /// whose field records straddle 2^63 overflowed on the way.
    #[test]
    fn swift_relative_names_resolve_across_the_signed_boundary() {
        assert_swift_field(0x7fff_ffff_ffff_fff0);
    }
}
