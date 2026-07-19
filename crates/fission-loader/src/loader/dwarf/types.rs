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
}

struct TypeDieInfo {
    tag: DwTag,
    name: Option<String>,
    type_ref: Option<UnitOffset<usize>>,
}

fn resolve_type_name(
    offset: UnitOffset<usize>,
    all_types: &HashMap<UnitOffset<usize>, TypeDieInfo>,
    resolved: &mut HashMap<UnitOffset<usize>, Option<String>>,
    visiting: &mut std::collections::HashSet<UnitOffset<usize>>,
) -> Option<String> {
    if let Some(cached) = resolved.get(&offset) {
        return cached.clone();
    }
    if visiting.contains(&offset) {
        return None;
    }
    visiting.insert(offset);

    let die_info = all_types.get(&offset)?;
    let result = match die_info.tag {
        DwTag(0x0f) => {
            // DW_TAG_pointer_type
            let base = if let Some(ref_offset) = die_info.type_ref {
                resolve_type_name(ref_offset, all_types, resolved, visiting)
                    .unwrap_or_else(|| format!("ptr_0x{:x}", ref_offset.0))
            } else {
                "void".to_string()
            };
            Some(format!("{}*", base))
        }
        DwTag(0x26) => {
            // DW_TAG_const_type
            let base = if let Some(ref_offset) = die_info.type_ref {
                resolve_type_name(ref_offset, all_types, resolved, visiting)
            } else {
                None
            };
            base.map(|b| format!("const {}", b))
        }
        DwTag(0x35) => {
            // DW_TAG_volatile_type
            let base = if let Some(ref_offset) = die_info.type_ref {
                resolve_type_name(ref_offset, all_types, resolved, visiting)
            } else {
                None
            };
            base.map(|b| format!("volatile {}", b))
        }
        _ => {
            // typedef, base_type, struct, class, union, enum
            die_info.name.clone()
        }
    };

    visiting.remove(&offset);
    resolved.insert(offset, result.clone());
    result
}

impl<'a> super::analyzer::DwarfAnalyzer<'a> {
    /// Collect all type names in a compilation unit for cross-referencing
    pub(super) fn collect_type_names(
        &self,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
        cache: &mut HashMap<UnitOffset<usize>, String>,
    ) -> Result<(), gimli::Error> {
        let mut all_types = HashMap::new();
        let mut entries = unit.entries();
        while let Some((_, entry)) = entries.next_dfs()? {
            match entry.tag() {
                DwTag(0x13) | DwTag(0x02) | DwTag(0x17) | DwTag(0x04) | DwTag(0x16)
                | DwTag(0x24) | DwTag(0x0f) | DwTag(0x26) | DwTag(0x35) => {
                    let name = self.get_attr_string(entry, DwAt(0x03), unit, dwarf)?;
                    let type_ref = match entry.attr_value(DwAt(0x49))? {
                        Some(AttributeValue::UnitRef(ref_offset)) => Some(ref_offset),
                        _ => None,
                    };
                    all_types.insert(
                        entry.offset(),
                        TypeDieInfo {
                            tag: entry.tag(),
                            name,
                            type_ref,
                        },
                    );
                }
                _ => {}
            }
        }

        let mut resolved = HashMap::new();
        let mut visiting = std::collections::HashSet::new();
        // Sorted, not raw HashMap iteration order: `resolve_type_name` is a
        // memoized recursive walk with cycle detection (`visiting`), so for
        // mutually-referential types (e.g. linked-list-style self/mutual
        // struct refs) which offset gets visited *first* can change which
        // side of the cycle resolves a name vs bails out -- unstable
        // std HashMap iteration order made this run-to-run nondeterministic.
        let mut offsets: Vec<_> = all_types.keys().copied().collect();
        offsets.sort();
        for offset in offsets {
            resolve_type_name(offset, &all_types, &mut resolved, &mut visiting);
        }

        for (offset, name_opt) in resolved {
            if let Some(name) = name_opt {
                cache.insert(offset, name);
            }
        }

        Ok(())
    }

    /// Resolve DW_AT_type attribute to a human-readable type name
    pub(super) fn resolve_type_ref(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        _unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        type_cache: &HashMap<UnitOffset<usize>, String>,
    ) -> Result<Option<String>, gimli::Error> {
        match entry.attr_value(DwAt(0x49))? {
            Some(AttributeValue::UnitRef(ref_offset)) => Ok(type_cache.get(&ref_offset).cloned()),
            _ => Ok(None),
        }
    }
}
