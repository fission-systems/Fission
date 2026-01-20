//! DWARF Debug Information Parser
//!
//! Extracts type information from DWARF debug sections in binaries.
//! Supports ELF and Mach-O formats with debug info.

use crate::loader::{LoadedBinary, types::InferredFieldInfo, types::InferredTypeInfo};
use crate::prelude::*;

/// DWARF section names for different platforms
const DWARF_SECTIONS: &[&str] = &[
    // ELF
    ".debug_info",
    ".debug_abbrev",
    ".debug_str",
    ".debug_types",
    // Mach-O
    "__debug_info",
    "__debug_abbrev",
    "__debug_str",
    "__debug_types",
    // DWARF5
    ".debug_str_offsets",
    "__debug_str_offs",
];

/// DWARF tag constants
mod tag {
    pub const DW_TAG_structure_type: u16 = 0x13;
    pub const DW_TAG_class_type: u16 = 0x02;
    pub const DW_TAG_member: u16 = 0x0d;
    pub const DW_TAG_typedef: u16 = 0x16;
    pub const DW_TAG_base_type: u16 = 0x24;
    pub const DW_TAG_pointer_type: u16 = 0x0f;
    pub const DW_TAG_array_type: u16 = 0x01;
    pub const DW_TAG_union_type: u16 = 0x17;
    pub const DW_TAG_enumeration_type: u16 = 0x04;
}

/// DWARF attribute constants  
mod attr {
    pub const DW_AT_name: u16 = 0x03;
    pub const DW_AT_byte_size: u16 = 0x0b;
    pub const DW_AT_data_member_location: u16 = 0x38;
    pub const DW_AT_type: u16 = 0x49;
    pub const DW_AT_sibling: u16 = 0x01;
}

/// DWARF debug information analyzer
pub struct DwarfAnalyzer<'a> {
    binary: &'a LoadedBinary,
    debug_info: Option<Vec<u8>>,
    debug_abbrev: Option<Vec<u8>>,
    debug_str: Option<Vec<u8>>,
    is_64bit: bool,
}

impl<'a> DwarfAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        let mut analyzer = Self {
            binary,
            debug_info: None,
            debug_abbrev: None,
            debug_str: None,
            is_64bit: binary.is_64bit,
        };
        analyzer.load_sections();
        analyzer
    }

    /// Check if DWARF info is available
    pub fn has_debug_info(&self) -> bool {
        self.debug_info.is_some()
    }

    fn load_sections(&mut self) {
        for section in &self.binary.sections {
            let name = section.name.as_str();
            if name == ".debug_info" || name == "__debug_info" {
                self.debug_info = self
                    .binary
                    .get_bytes(section.virtual_address, section.virtual_size as usize);
            } else if name == ".debug_abbrev" || name == "__debug_abbrev" {
                self.debug_abbrev = self
                    .binary
                    .get_bytes(section.virtual_address, section.virtual_size as usize);
            } else if name == ".debug_str" || name == "__debug_str" {
                self.debug_str = self
                    .binary
                    .get_bytes(section.virtual_address, section.virtual_size as usize);
            }
        }
    }

    /// Analyze DWARF info and extract type information
    pub fn analyze_types(&self) -> Vec<DwarfTypeInfo> {
        let mut types = Vec::new();

        let Some(debug_info) = &self.debug_info else {
            return types;
        };

        let abbrev = self.debug_abbrev.as_ref();
        let str_section = self.debug_str.as_ref();

        // Parse compilation units
        let mut offset = 0;
        while offset < debug_info.len() {
            if let Some((cu_end, cu_types)) =
                self.parse_compilation_unit(debug_info, offset, abbrev, str_section)
            {
                types.extend(cu_types);
                offset = cu_end;
            } else {
                break;
            }
        }

        tracing::info!(
            "[DwarfAnalyzer] Found {} types from DWARF debug info",
            types.len()
        );
        for ty in &types {
            tracing::info!(
                "  - {} ({}) with {} members",
                ty.name,
                ty.kind,
                ty.members.len()
            );
        }

        types
    }

    fn parse_compilation_unit(
        &self,
        data: &[u8],
        offset: usize,
        abbrev: Option<&Vec<u8>>,
        str_section: Option<&Vec<u8>>,
    ) -> Option<(usize, Vec<DwarfTypeInfo>)> {
        if offset + 11 > data.len() {
            return None;
        }

        // Read compilation unit header
        let mut pos = offset;

        // Unit length (4 or 12 bytes for 64-bit DWARF)
        let unit_length =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        if unit_length == 0xffffffff {
            // 64-bit DWARF format
            pos += 8; // Skip 64-bit length
        }

        let unit_end = offset + 4 + unit_length as usize;
        if unit_end > data.len() {
            return None;
        }

        // Version (2 bytes)
        let _version = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        // Skip header (version dependent)
        pos += if self.is_64bit { 9 } else { 5 };

        let mut types = Vec::new();

        // Parse DIEs (Debug Information Entries)
        while pos < unit_end {
            if let Some((die_end, type_info)) =
                self.parse_die(data, pos, unit_end, abbrev, str_section)
            {
                if let Some(ti) = type_info {
                    types.push(ti);
                }
                pos = die_end;
            } else {
                pos += 1;
            }
        }

        Some((unit_end, types))
    }

    fn parse_die(
        &self,
        data: &[u8],
        offset: usize,
        limit: usize,
        abbrev: Option<&Vec<u8>>,
        str_section: Option<&Vec<u8>>,
    ) -> Option<(usize, Option<DwarfTypeInfo>)> {
        if offset >= limit {
            return None;
        }

        // Read abbreviation code (ULEB128)
        let (abbrev_code, code_len) = self.read_uleb128(data, offset)?;
        let mut pos = offset + code_len;

        if abbrev_code == 0 {
            // Null DIE
            return Some((pos, None));
        }

        // For now, we use simplified parsing based on common patterns
        // A full implementation would decode abbreviation tables

        // Try to identify struct/class types by scanning for patterns
        if pos + 16 > limit {
            return Some((pos + 4, None));
        }

        // Simplified: Look for name string reference and size
        let mut name = String::new();
        let mut size: u32 = 0;
        let mut kind = "unknown";
        let mut members = Vec::new();

        // Scan ahead for potential attributes
        let scan_limit = (limit - pos).min(128);
        let scan_data = &data[pos..pos + scan_limit];

        // Look for string references in the .debug_str section
        if let Some(str_sec) = str_section {
            for i in 0..scan_limit.saturating_sub(4) {
                let potential_offset = u32::from_le_bytes([
                    scan_data[i],
                    scan_data[i + 1],
                    scan_data[i + 2],
                    scan_data[i + 3],
                ]) as usize;

                if potential_offset < str_sec.len() {
                    if let Some(s) = self.read_string_at(str_sec, potential_offset) {
                        if !s.is_empty()
                            && s.len() < 64
                            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        {
                            if name.is_empty() {
                                name = s;
                                kind = "struct";
                            }
                        }
                    }
                }
            }
        }

        // Skip to next DIE (simplified)
        let next_pos = (pos + 16).min(limit);

        if !name.is_empty() {
            Some((
                next_pos,
                Some(DwarfTypeInfo {
                    name,
                    kind: kind.to_string(),
                    size,
                    members,
                }),
            ))
        } else {
            Some((next_pos, None))
        }
    }

    fn read_uleb128(&self, data: &[u8], offset: usize) -> Option<(u64, usize)> {
        let mut result: u64 = 0;
        let mut shift = 0;
        let mut pos = offset;

        loop {
            if pos >= data.len() {
                return None;
            }
            let byte = data[pos];
            pos += 1;
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }

        Some((result, pos - offset))
    }

    fn read_string_at(&self, data: &[u8], offset: usize) -> Option<String> {
        if offset >= data.len() {
            return None;
        }
        let mut end = offset;
        while end < data.len() && data[end] != 0 {
            end += 1;
        }
        if end == offset {
            return None;
        }
        Some(String::from_utf8_lossy(&data[offset..end]).to_string())
    }
}

/// DWARF type information
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uleb128() {
        let analyzer = DwarfAnalyzer {
            binary: unsafe { std::mem::zeroed() }, // Note: only for testing read_uleb128
            debug_info: None,
            debug_abbrev: None,
            debug_str: None,
            is_64bit: true,
        };

        // 624485 in ULEB128 = [0xe5, 0x8e, 0x26]
        let data = [0xe5, 0x8e, 0x26];
        let result = analyzer.read_uleb128(&data, 0);
        assert!(result.is_some());
        let (value, len) = result.unwrap();
        assert_eq!(value, 624485);
        assert_eq!(len, 3);
    }
}
