//! DWARF Analyzer - Main Coordinator
//!
//! Provides the main DwarfAnalyzer interface for extracting type and function
//! information from DWARF debug sections.

use super::sections::SectionData;
use super::types::DwarfTypeInfo;
use crate::loader::LoadedBinary;
use crate::loader::types::DwarfFunctionInfo;
use gimli::{AttributeValue, DebuggingInformationEntry, DwAt, EndianSlice, RunTimeEndian};

/// DWARF debug information analyzer using gimli
pub struct DwarfAnalyzer<'a> {
    binary: &'a LoadedBinary,
    has_debug: bool,
}

impl<'a> DwarfAnalyzer<'a> {
    /// Create a new DWARF analyzer for the given binary
    pub fn new(binary: &'a LoadedBinary) -> Self {
        let has_debug = binary.sections.iter().any(|s| {
            let name = s.name.as_str();
            name == ".debug_info" || name == "__debug_info"
        });
        Self { binary, has_debug }
    }

    /// Check if DWARF debug information is available
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

    /// Build a gimli::Dwarf instance from the binary's debug sections
    pub(super) fn build_dwarf(
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
        let debug_rnglists =
            gimli::DebugRngLists::new(sections.get(".debug_rnglists", data), endian);
        let ranges = gimli::RangeLists::new(debug_ranges, debug_rnglists);

        let debug_loc = gimli::DebugLoc::new(&[], endian);
        let debug_loclists = gimli::DebugLocLists::new(&[], endian);
        let locations = gimli::LocationLists::new(debug_loc, debug_loclists);

        Ok(gimli::Dwarf {
            debug_abbrev: gimli::DebugAbbrev::new(sections.get(".debug_abbrev", data), endian),
            debug_info: gimli::DebugInfo::new(sections.get(".debug_info", data), endian),
            debug_str: gimli::DebugStr::new(sections.get(".debug_str", data), endian),
            debug_line: gimli::DebugLine::new(sections.get(".debug_line", data), endian),
            debug_line_str: gimli::DebugLineStr::new(sections.get(".debug_line_str", data), endian),
            debug_types: gimli::DebugTypes::new(sections.get(".debug_types", data), endian),
            ranges,
            locations,
            ..Default::default()
        })
    }

    // ========================================================================
    // Attribute helper methods (used by types.rs and functions.rs)
    // ========================================================================

    /// Get a string attribute value
    pub(super) fn get_attr_string(
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
    pub(super) fn get_attr_u64(
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
    pub(super) fn get_member_offset(
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
        let binary = LoadedBinaryBuilder::new("test".to_string(), DataBuffer::Heap(Vec::new()))
            .format("test")
            .arch_spec("x86:LE:64:default")
            .entry_point(0)
            .image_base(0)
            .is_64bit(true)
            .build()
            .unwrap_or_else(|_| panic!("failed to build test LoadedBinary"));

        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(!analyzer.has_debug_info());
        assert!(analyzer.analyze_types().is_empty());
        assert!(analyzer.analyze_functions().is_empty());
    }
}
