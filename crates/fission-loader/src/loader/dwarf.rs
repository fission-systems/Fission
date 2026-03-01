//! DWARF Debug Information Parser
//!
//! Extracts type, function, parameter, and local variable information from
//! DWARF debug sections using the `gimli` crate.
//!
//! Supports ELF and Mach-O formats with `.debug_info`, `.debug_abbrev`,
//! `.debug_str`, `.debug_line`, and `.debug_ranges` sections.

use crate::loader::types::{
    DwarfFunctionInfo, DwarfLocalVar, DwarfLocation, DwarfParamInfo, InferredFieldInfo,
    InferredTypeInfo,
};
use crate::loader::LoadedBinary;
use gimli::{
    AttributeValue, DebuggingInformationEntry, DwAt, DwTag, EndianSlice, RunTimeEndian,
    UnitOffset,
};
use std::collections::HashMap;

// ============================================================================
// Helper: Section data loader from LoadedBinary
// ============================================================================

/// Wrapper that provides section data to gimli from LoadedBinary's sections.
struct SectionData {
    sections: HashMap<&'static str, (usize, usize)>, // (file_offset, file_size)
}

impl SectionData {
    fn new(binary: &LoadedBinary) -> Self {
        let mut sections = HashMap::new();
        let data_len = binary.data.as_slice().len();

        for section in &binary.sections {
            let name = section.name.as_str();
            // Map both ELF (.debug_*) and Mach-O (__debug_*) names to canonical keys
            let canonical = match name {
                ".debug_info" | "__debug_info" => Some(".debug_info"),
                ".debug_abbrev" | "__debug_abbrev" => Some(".debug_abbrev"),
                ".debug_str" | "__debug_str" => Some(".debug_str"),
                ".debug_line" | "__debug_line" => Some(".debug_line"),
                ".debug_ranges" | "__debug_ranges" => Some(".debug_ranges"),
                ".debug_rnglists" | "__debug_rnglists" => Some(".debug_rnglists"),
                ".debug_str_offsets" | "__debug_str_offs" => Some(".debug_str_offsets"),
                ".debug_addr" | "__debug_addr" => Some(".debug_addr"),
                ".debug_line_str" | "__debug_line_str" => Some(".debug_line_str"),
                ".debug_types" | "__debug_types" => Some(".debug_types"),
                _ => None,
            };

            if let Some(key) = canonical {
                let file_offset = section.file_offset as usize;
                let file_size = section.file_size as usize;
                if file_offset + file_size <= data_len {
                    sections.insert(key, (file_offset, file_size));
                }
            }
        }

        Self { sections }
    }

    fn get<'a>(&self, name: &str, data: &'a [u8]) -> &'a [u8] {
        if let Some(&(offset, size)) = self.sections.get(name) {
            &data[offset..offset + size]
        } else {
            &[]
        }
    }
}

// ============================================================================
// DWARF Type Information
// ============================================================================

/// DWARF type information (struct/class/union)
#[derive(Debug, Clone)]
pub struct DwarfTypeInfo {
    pub name: String,
    pub kind: String, // "struct", "class", "union", "enum"
    pub size: u32,
    pub members: Vec<DwarfMemberInfo>,
}

/// DWARF struct/class member information
#[derive(Debug, Clone)]
pub struct DwarfMemberInfo {
    pub name: String,
    pub type_name: String,
    pub offset: u32,
    pub size: u32,
}

impl DwarfTypeInfo {
    /// Convert to InferredTypeInfo for integration with decompiler
    pub fn to_inferred_type(&self) -> InferredTypeInfo {
        InferredTypeInfo {
            name: self.name.clone(),
            mangled_name: String::new(),
            kind: self.kind.clone(),
            fields: self
                .members
                .iter()
                .map(|m| InferredFieldInfo {
                    name: m.name.clone(),
                    type_name: m.type_name.clone(),
                    offset: m.offset,
                    size: m.size,
                })
                .collect(),
            size: self.size,
            metadata_address: 0,
        }
    }
}

// ============================================================================
// DWARF Analyzer (gimli-based)
// ============================================================================

/// DWARF debug information analyzer using gimli
pub struct DwarfAnalyzer<'a> {
    binary: &'a LoadedBinary,
    has_debug: bool,
}

impl<'a> DwarfAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        let has_debug = binary.sections.iter().any(|s| {
            let name = s.name.as_str();
            name == ".debug_info" || name == "__debug_info"
        });
        Self { binary, has_debug }
    }

    /// Check if DWARF info is available
    pub fn has_debug_info(&self) -> bool {
        self.has_debug
    }

    /// Analyze DWARF info and extract type information (struct/class/union)
    pub fn analyze_types(&self) -> Vec<DwarfTypeInfo> {
        if !self.has_debug {
            return Vec::new();
        }

        match self.analyze_types_inner() {
            Ok(types) => {
                tracing::info!(
                    "[DwarfAnalyzer] Extracted {} types from DWARF debug info",
                    types.len()
                );
                types
            }
            Err(e) => {
                tracing::warn!("[DwarfAnalyzer] Error parsing DWARF types: {}", e);
                Vec::new()
            }
        }
    }

    /// Analyze DWARF info and extract function information (name, params, locals)
    pub fn analyze_functions(&self) -> Vec<DwarfFunctionInfo> {
        if !self.has_debug {
            return Vec::new();
        }

        match self.analyze_functions_inner() {
            Ok(funcs) => {
                tracing::info!(
                    "[DwarfAnalyzer] Extracted {} functions from DWARF debug info",
                    funcs.len()
                );
                funcs
            }
            Err(e) => {
                tracing::warn!("[DwarfAnalyzer] Error parsing DWARF functions: {}", e);
                Vec::new()
            }
        }
    }

    // ========================================================================
    // gimli::Dwarf construction
    // ========================================================================

    fn build_dwarf(
        &self,
    ) -> Result<gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>, gimli::Error> {
        let sections = SectionData::new(self.binary);
        let data = self.binary.data.as_slice();
        let endian = if self.binary.arch_spec.contains("BE") {
            RunTimeEndian::Big
        } else {
            RunTimeEndian::Little
        };

        let debug_ranges = gimli::DebugRanges::new(sections.get(".debug_ranges", data), endian);
        let debug_rnglists = gimli::DebugRngLists::new(sections.get(".debug_rnglists", data), endian);
        let ranges = gimli::RangeLists::new(debug_ranges, debug_rnglists);

        let debug_loc = gimli::DebugLoc::new(&[], endian);
        let debug_loclists = gimli::DebugLocLists::new(&[], endian);
        let locations = gimli::LocationLists::new(debug_loc, debug_loclists);

        Ok(gimli::Dwarf {
            debug_abbrev: gimli::DebugAbbrev::new(sections.get(".debug_abbrev", data), endian),
            debug_info: gimli::DebugInfo::new(sections.get(".debug_info", data), endian),
            debug_str: gimli::DebugStr::new(sections.get(".debug_str", data), endian),
            debug_line: gimli::DebugLine::new(sections.get(".debug_line", data), endian),
            debug_line_str: gimli::DebugLineStr::new(
                sections.get(".debug_line_str", data),
                endian,
            ),
            debug_types: gimli::DebugTypes::new(sections.get(".debug_types", data), endian),
            ranges,
            locations,
            ..Default::default()
        })
    }

    // ========================================================================
    // Type Extraction
    // ========================================================================

    fn analyze_types_inner(&self) -> Result<Vec<DwarfTypeInfo>, gimli::Error> {
        let dwarf = self.build_dwarf()?;
        let mut types = Vec::new();

        let mut units = dwarf.units();
        while let Some(unit_header) = units.next()? {
            let unit = dwarf.unit(unit_header)?;

            // Build a type name cache for cross-referencing within this CU
            let mut type_cache: HashMap<UnitOffset<usize>, String> = HashMap::new();
            self.collect_type_names(&unit, &dwarf, &mut type_cache)?;

            // Re-iterate for type extraction
            let mut entries = unit.entries();
            while let Some((_, entry)) = entries.next_dfs()? {
                match entry.tag() {
                    DwTag(0x13) | DwTag(0x02) | DwTag(0x17) => {
                        // DW_TAG_structure_type | DW_TAG_class_type | DW_TAG_union_type
                        if let Some(ti) =
                            self.extract_type_info(entry, &unit, &dwarf, &type_cache)?
                        {
                            types.push(ti);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(types)
    }

    fn extract_type_info(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<DwarfTypeInfo>, gimli::Error> {
        let name = match self.get_attr_string(entry, DwAt(0x03), unit, dwarf)? {
            Some(n) if !n.is_empty() => n,
            _ => return Ok(None), // Skip anonymous types
        };

        let kind = match entry.tag() {
            DwTag(0x13) => "struct",
            DwTag(0x02) => "class",
            DwTag(0x17) => "union",
            _ => "struct",
        }
        .to_string();

        let size = self
            .get_attr_u64(entry, DwAt(0x0b))? // DW_AT_byte_size
            .unwrap_or(0) as u32;

        // Extract members from children
        let mut members = Vec::new();
        let mut tree = unit.entries_tree(Some(entry.offset()))?;
        let root = tree.root()?;
        let mut children = root.children();
        while let Some(child) = children.next()? {
            let child_entry = child.entry();
            if child_entry.tag() == DwTag(0x0d) {
                // DW_TAG_member
                if let Some(member) =
                    self.extract_member_info(child_entry, unit, dwarf, type_cache)?
                {
                    members.push(member);
                }
            }
        }

        Ok(Some(DwarfTypeInfo {
            name,
            kind,
            size,
            members,
        }))
    }

    fn extract_member_info(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<DwarfMemberInfo>, gimli::Error> {
        let name = self
            .get_attr_string(entry, DwAt(0x03), unit, dwarf)?
            .unwrap_or_default();
        if name.is_empty() {
            return Ok(None);
        }

        let type_name = self
            .resolve_type_ref(entry, unit, type_cache)?
            .unwrap_or_else(|| "unknown".to_string());

        let offset = self.get_member_offset(entry)?.unwrap_or(0);
        let size = self.get_attr_u64(entry, DwAt(0x0b))?.unwrap_or(0) as u32;

        Ok(Some(DwarfMemberInfo {
            name,
            type_name,
            offset,
            size,
        }))
    }

    // ========================================================================
    // Function Extraction
    // ========================================================================

    fn analyze_functions_inner(&self) -> Result<Vec<DwarfFunctionInfo>, gimli::Error> {
        let dwarf = self.build_dwarf()?;
        let mut functions = Vec::new();

        let mut units = dwarf.units();
        while let Some(unit_header) = units.next()? {
            let unit = dwarf.unit(unit_header)?;

            // Build type cache for this compilation unit
            let mut type_cache: HashMap<UnitOffset<usize>, String> = HashMap::new();
            self.collect_type_names(&unit, &dwarf, &mut type_cache)?;

            // Use flat DFS iteration with depth tracking to avoid ownership issues
            // with EntriesTreeNode::children() consuming self
            let mut entries = unit.entries();
            let mut current_func: Option<FuncBuilder> = None;
            let mut func_depth: isize = 0;

            while let Some((delta_depth, entry)) = entries.next_dfs()? {
                if current_func.is_some() {
                    // We're inside a subprogram — track depth relative to the function DIE
                    func_depth += delta_depth;

                    if func_depth <= 0 {
                        // We've exited the subprogram — finalize it
                        if let Some(func) = current_func.take() {
                            if let Some(fi) = func.build() {
                                functions.push(fi);
                            }
                        }
                        // Fall through to check if this entry is another subprogram
                    } else {
                        // Process children of the current subprogram
                        let func = current_func.as_mut().unwrap();
                        match entry.tag() {
                            DwTag(0x05) => {
                                // DW_TAG_formal_parameter
                                if let Some(param) =
                                    self.extract_param_info(entry, &unit, &dwarf, &type_cache)?
                                {
                                    func.params.push(param);
                                }
                            }
                            DwTag(0x34) => {
                                // DW_TAG_variable (top-level or in lexical block)
                                if let Some(var) =
                                    self.extract_local_var_info(entry, &unit, &dwarf, &type_cache)?
                                {
                                    func.local_vars.push(var);
                                }
                            }
                            _ => {} // DW_TAG_lexical_block, etc. — just continue DFS
                        }
                        continue;
                    }
                }

                // Look for DW_TAG_subprogram at any level
                if entry.tag() == DwTag(0x2e) {
                    // DW_TAG_subprogram — start collecting
                    let address = match self.get_attr_u64(entry, DwAt(0x11))? {
                        Some(addr) if addr != 0 => addr,
                        _ => continue, // Declaration-only / inlined
                    };

                    let raw_name = self
                        .get_attr_string(entry, DwAt(0x6e), &unit, &dwarf)?
                        .or(self.get_attr_string(entry, DwAt(0x03), &unit, &dwarf)?)
                        .unwrap_or_default();
                    if raw_name.is_empty() {
                        continue;
                    }

                    let name = crate::loader::demangle::demangle(&raw_name);
                    let return_type = self.resolve_type_ref(entry, &unit, &type_cache)?;

                    current_func = Some(FuncBuilder {
                        address,
                        name,
                        return_type,
                        params: Vec::new(),
                        local_vars: Vec::new(),
                    });
                    func_depth = 1; // We're at depth 1 relative to this subprogram
                }
            }

            // Finalize any remaining function at end of unit
            if let Some(func) = current_func {
                if let Some(fi) = func.build() {
                    functions.push(fi);
                }
            }
        }

        Ok(functions)
    }

    fn extract_param_info(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<DwarfParamInfo>, gimli::Error> {
        let name = self
            .get_attr_string(entry, DwAt(0x03), unit, dwarf)?
            .unwrap_or_default();
        if name.is_empty() {
            return Ok(None);
        }

        let type_name = self
            .resolve_type_ref(entry, unit, type_cache)?
            .unwrap_or_else(|| "int".to_string());

        let location = self.extract_location(entry, unit)?;

        Ok(Some(DwarfParamInfo {
            name,
            type_name,
            location,
        }))
    }

    fn extract_local_var_info(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<DwarfLocalVar>, gimli::Error> {
        let name = self
            .get_attr_string(entry, DwAt(0x03), unit, dwarf)?
            .unwrap_or_default();
        if name.is_empty() {
            return Ok(None);
        }

        let type_name = self
            .resolve_type_ref(entry, unit, type_cache)?
            .unwrap_or_else(|| "int".to_string());

        let location = self.extract_location(entry, unit)?;

        Ok(Some(DwarfLocalVar {
            name,
            type_name,
            location,
        }))
    }

    // ========================================================================
    // Attribute Helpers
    // ========================================================================

    /// Collect all type names in a compilation unit for cross-referencing
    fn collect_type_names(
        &self,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        cache: &mut HashMap<UnitOffset<usize>, String>,
    ) -> Result<(), gimli::Error> {
        let mut entries = unit.entries();
        while let Some((_, entry)) = entries.next_dfs()? {
            match entry.tag() {
                DwTag(0x13) | DwTag(0x02) | DwTag(0x17) | DwTag(0x04) | DwTag(0x16)
                | DwTag(0x24) | DwTag(0x0f) => {
                    // struct, class, union, enum, typedef, base_type, pointer_type
                    if let Some(name) = self.get_type_display_name(entry, unit, dwarf, cache)? {
                        cache.insert(entry.offset(), name);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Get a display name for a type DIE
    fn get_type_display_name(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<String>, gimli::Error> {
        match entry.tag() {
            DwTag(0x0f) => {
                // DW_TAG_pointer_type → resolve base type + "*"
                if let Some(AttributeValue::UnitRef(ref_offset)) =
                    entry.attr_value(DwAt(0x49))?
                {
                    if let Some(base_name) = cache.get(&ref_offset) {
                        Ok(Some(format!("{}*", base_name)))
                    } else {
                        Ok(Some(format!("ptr_0x{:x}", ref_offset.0)))
                    }
                } else {
                    Ok(Some("void*".to_string()))
                }
            }
            DwTag(0x24) | DwTag(0x16) => {
                // DW_TAG_base_type | DW_TAG_typedef
                Ok(self.get_attr_string(entry, DwAt(0x03), unit, dwarf)?)
            }
            _ => Ok(self.get_attr_string(entry, DwAt(0x03), unit, dwarf)?),
        }
    }

    /// Resolve DW_AT_type attribute to a human-readable type name
    fn resolve_type_ref(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<String>, gimli::Error> {
        match entry.attr_value(DwAt(0x49))? {
            Some(AttributeValue::UnitRef(ref_offset)) => {
                if let Some(name) = type_cache.get(&ref_offset) {
                    return Ok(Some(name.clone()));
                }
                // Fallback: read the referenced DIE directly and chase pointer chains
                if let Ok(ref_entry) = unit.entry(ref_offset) {
                    if ref_entry.tag() == DwTag(0x0f) {
                        // Pointer — resolve its base type
                        if let Some(base) = self.resolve_type_ref(&ref_entry, unit, type_cache)? {
                            return Ok(Some(format!("{}*", base)));
                        }
                        return Ok(Some("void*".to_string()));
                    }
                    if ref_entry.tag() == DwTag(0x35) {
                        // DW_TAG_volatile_type
                        if let Some(base) = self.resolve_type_ref(&ref_entry, unit, type_cache)? {
                            return Ok(Some(format!("volatile {}", base)));
                        }
                    }
                    if ref_entry.tag() == DwTag(0x26) {
                        // DW_TAG_const_type
                        if let Some(base) = self.resolve_type_ref(&ref_entry, unit, type_cache)? {
                            return Ok(Some(format!("const {}", base)));
                        }
                    }
                    // Try to get name attribute directly
                    if let Some(attr) = ref_entry.attr(DwAt(0x03))? {
                        if let AttributeValue::String(s) = attr.value() {
                            return Ok(Some(s.to_string_lossy().to_string()));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Get a string attribute value
    fn get_attr_string(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        attr_name: DwAt,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
    ) -> Result<Option<String>, gimli::Error> {
        match entry.attr_value(attr_name)? {
            Some(AttributeValue::String(s)) => Ok(Some(s.to_string_lossy().to_string())),
            Some(AttributeValue::DebugStrRef(offset)) => {
                let s = dwarf.debug_str.get_str(offset)?;
                Ok(Some(s.to_string_lossy().to_string()))
            }
            Some(AttributeValue::DebugStrOffsetsIndex(index)) => {
                match dwarf.attr_string(unit, AttributeValue::DebugStrOffsetsIndex(index)) {
                    Ok(s) => Ok(Some(s.to_string_lossy().to_string())),
                    Err(_) => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    /// Get a u64 attribute value
    fn get_attr_u64(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        attr_name: DwAt,
    ) -> Result<Option<u64>, gimli::Error> {
        match entry.attr_value(attr_name)? {
            Some(AttributeValue::Udata(v)) => Ok(Some(v)),
            Some(AttributeValue::Data1(v)) => Ok(Some(v as u64)),
            Some(AttributeValue::Data2(v)) => Ok(Some(v as u64)),
            Some(AttributeValue::Data4(v)) => Ok(Some(v as u64)),
            Some(AttributeValue::Data8(v)) => Ok(Some(v)),
            Some(AttributeValue::Addr(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    /// Extract member offset from DW_AT_data_member_location
    fn get_member_offset(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<Option<u32>, gimli::Error> {
        match entry.attr_value(DwAt(0x38))? {
            Some(AttributeValue::Udata(v)) => Ok(Some(v as u32)),
            Some(AttributeValue::Data1(v)) => Ok(Some(v as u32)),
            Some(AttributeValue::Data2(v)) => Ok(Some(v as u32)),
            Some(AttributeValue::Data4(v)) => Ok(Some(v as u32)),
            Some(AttributeValue::Sdata(v)) => Ok(Some(v as u32)),
            _ => Ok(None),
        }
    }

    /// Extract DW_AT_location → DwarfLocation
    fn extract_location(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<DwarfLocation, gimli::Error> {
        match entry.attr_value(DwAt(0x02))? {
            Some(AttributeValue::Exprloc(expr)) => self.parse_location_expr(expr, unit),
            _ => Ok(DwarfLocation::Unknown),
        }
    }

    /// Parse a DWARF location expression to extract stack offset or register
    fn parse_location_expr(
        &self,
        expr: gimli::Expression<EndianSlice<'a, RunTimeEndian>>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<DwarfLocation, gimli::Error> {
        let mut ops = expr.operations(unit.encoding());
        if let Ok(Some(op)) = ops.next() {
            match op {
                gimli::Operation::FrameOffset { offset } => {
                    Ok(DwarfLocation::StackOffset(offset))
                }
                gimli::Operation::Register { register } => {
                    Ok(DwarfLocation::Register(format!("reg{}", register.0)))
                }
                gimli::Operation::RegisterOffset {
                    register, offset, ..
                } => {
                    // If base register is frame/stack pointer, treat as stack offset
                    // x86_64: RBP=6, RSP=7; AArch64: FP=29, SP=31
                    if register.0 == 6 || register.0 == 7 || register.0 == 29 || register.0 == 31 {
                        Ok(DwarfLocation::StackOffset(offset))
                    } else {
                        Ok(DwarfLocation::Register(format!("reg{}", register.0)))
                    }
                }
                _ => Ok(DwarfLocation::Unknown),
            }
        } else {
            Ok(DwarfLocation::Unknown)
        }
    }
}

// ============================================================================
// Internal helper for building DwarfFunctionInfo during DFS
// ============================================================================

struct FuncBuilder {
    address: u64,
    name: String,
    return_type: Option<String>,
    params: Vec<DwarfParamInfo>,
    local_vars: Vec<DwarfLocalVar>,
}

impl FuncBuilder {
    fn build(self) -> Option<DwarfFunctionInfo> {
        Some(DwarfFunctionInfo {
            address: self.address,
            name: self.name,
            return_type: self.return_type,
            params: self.params,
            local_vars: self.local_vars,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::types::{DataBuffer, LoadedBinaryBuilder};

    #[test]
    fn test_analyzer_no_debug_info() {
        let binary =
            LoadedBinaryBuilder::new("test".to_string(), DataBuffer::Heap(Vec::new()))
                .format("test")
                .arch_spec("x86:LE:64:default")
                .entry_point(0)
                .image_base(0)
                .is_64bit(true)
                .build()
                .expect("failed to build test LoadedBinary");

        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(!analyzer.has_debug_info());
        assert!(analyzer.analyze_types().is_empty());
        assert!(analyzer.analyze_functions().is_empty());
    }
}
