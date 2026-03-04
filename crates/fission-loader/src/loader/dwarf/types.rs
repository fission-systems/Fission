//! DWARF Type Information Extraction
//!
//! Extracts struct, class, union, and enum types from DWARF debug information.

use crate::loader::types::{InferredFieldInfo, InferredTypeInfo};
use gimli::{
    AttributeValue, DebuggingInformationEntry, DwAt, DwTag, EndianSlice, RunTimeEndian, UnitOffset,
};
use std::collections::HashMap;

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
    /// Convert to InferredTypeInfo for decompiler integration
    pub fn to_inferred_type(&self) -> InferredTypeInfo {
        InferredTypeInfo {
            name: self.name.clone(),
            mangled_name: self.name.clone(), // DWARF names are already demangled
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

/// Type extraction methods for DwarfAnalyzer
impl<'a> super::analyzer::DwarfAnalyzer<'a> {
    /// Extract all type information from DWARF
    pub(super) fn analyze_types_inner(&self) -> Result<Vec<DwarfTypeInfo>, gimli::Error> {
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

    /// Extract type information from a single DIE
    pub(super) fn extract_type_info(
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

    /// Extract member information from a DW_TAG_member DIE
    pub(super) fn extract_member_info(
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

    /// Collect all type names in a compilation unit for cross-referencing
    pub(super) fn collect_type_names(
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
    pub(super) fn get_type_display_name(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<String>, gimli::Error> {
        match entry.tag() {
            DwTag(0x0f) => {
                // DW_TAG_pointer_type → resolve base type + "*"
                if let Some(AttributeValue::UnitRef(ref_offset)) = entry.attr_value(DwAt(0x49))? {
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
    pub(super) fn resolve_type_ref(
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
}
