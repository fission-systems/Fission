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

        let debug_loc = gimli::DebugLoc::new(sections.get(".debug_loc", data), endian);
        let debug_loclists =
            gimli::DebugLocLists::new(sections.get(".debug_loclists", data), endian);
        let locations = gimli::LocationLists::new(debug_loc, debug_loclists);
        let debug_addr =
            gimli::DebugAddr::from(EndianSlice::new(sections.get(".debug_addr", data), endian));

        Ok(gimli::Dwarf {
            debug_abbrev: gimli::DebugAbbrev::new(sections.get(".debug_abbrev", data), endian),
            debug_info: gimli::DebugInfo::new(sections.get(".debug_info", data), endian),
            debug_str: gimli::DebugStr::new(sections.get(".debug_str", data), endian),
            debug_line: gimli::DebugLine::new(sections.get(".debug_line", data), endian),
            debug_line_str: gimli::DebugLineStr::new(sections.get(".debug_line_str", data), endian),
            debug_types: gimli::DebugTypes::new(sections.get(".debug_types", data), endian),
            debug_addr,
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

    /// Get a signed i64 attribute value -- needed for `DW_AT_const_value` on
    /// `DW_TAG_enumerator`, which producers encode as `Sdata` for negative
    /// enumerator values (`get_attr_u64` doesn't handle `Sdata` at all, so
    /// it can't be reused here without silently dropping negative values).
    pub(super) fn get_attr_i64(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        attr_name: DwAt,
    ) -> Result<Option<i64>, gimli::Error> {
        match entry.attr_value(attr_name)? {
            Some(AttributeValue::Sdata(v)) => Ok(Some(v)),
            Some(AttributeValue::Udata(v)) => Ok(Some(v as i64)),
            Some(AttributeValue::Data1(v)) => Ok(Some(v as i64)),
            Some(AttributeValue::Data2(v)) => Ok(Some(v as i64)),
            Some(AttributeValue::Data4(v)) => Ok(Some(v as i64)),
            Some(AttributeValue::Data8(v)) => Ok(Some(v as i64)),
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
    use crate::loader::LoadedBinary;
    use crate::loader::types::{DataBuffer, LoadedBinaryBuilder};
    use std::path::PathBuf;

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

    /// `testdata/x64_dyn_enum_test.elf`: GCC-compiled from a source with
    /// `enum Color { RED = 0, GREEN = 1, BLUE = 5, NEGATIVE_ONE = -1 };`,
    /// cross-checked against `objdump --dwarf=info` (confirms
    /// `DW_TAG_enumerator`'s `DW_AT_const_value` really is emitted as
    /// negative for `NEGATIVE_ONE`, exercising `get_attr_i64`'s `Sdata`
    /// handling, which `get_attr_u64` can't do at all).
    #[test]
    fn analyze_types_extracts_real_enum_values() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_enum_test.elf");
        assert!(
            path.is_file(),
            "missing fixture {} (rebuild: gcc -g -O0 -o x64_dyn_enum_test.elf enum_test.c)",
            path.display()
        );
        let binary = LoadedBinary::from_file(&path).expect("load enum test ELF");
        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(analyzer.has_debug_info());

        let types = analyzer.analyze_types();
        let color = types
            .iter()
            .find(|t| t.name == "Color")
            .unwrap_or_else(|| panic!("expected a \"Color\" enum in {types:?}"));
        assert_eq!(color.kind, "enum");
        assert_eq!(color.size, 4);

        let value_of = |name: &str| -> i32 {
            color
                .members
                .iter()
                .find(|m| m.name == name)
                .unwrap_or_else(|| panic!("expected enumerator {name} in {:?}", color.members))
                .offset as i32
        };
        assert_eq!(value_of("RED"), 0);
        assert_eq!(value_of("GREEN"), 1);
        assert_eq!(value_of("BLUE"), 5);
        assert_eq!(value_of("NEGATIVE_ONE"), -1);
    }
}
