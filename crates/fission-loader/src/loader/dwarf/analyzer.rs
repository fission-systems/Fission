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
        let has_debug = Self::debug_source_of(binary).sections.iter().any(|s| {
            let name = s.name.as_str();
            name == ".debug_info" || name == "__debug_info"
        });
        Self { binary, has_debug }
    }

    /// The `LoadedBinary` whose sections/bytes actually hold DWARF data --
    /// `binary.external_debug_binary` when this binary is stripped and a
    /// `.gnu_debuglink`/build-id companion was resolved (see
    /// `dwarf::external::resolve_external_debug_binary`), `binary` itself
    /// otherwise. Every section/byte access in this module goes through
    /// this, not `self.binary` directly, so the split-debug-info case is
    /// transparent to the rest of the analyzer.
    fn debug_source_of(binary: &'a LoadedBinary) -> &'a LoadedBinary {
        binary.external_debug_binary.as_deref().unwrap_or(binary)
    }

    fn debug_source(&self) -> &'a LoadedBinary {
        Self::debug_source_of(self.binary)
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
        let debug_source = self.debug_source();
        let sections = SectionData::new(debug_source);
        let data = debug_source.data.as_slice();
        let endian = if debug_source.arch_spec.contains("BE") {
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

    /// `testdata/x64_dyn_array_test.elf`: GCC-compiled from a source with
    /// `struct WithArrays { int arr[10]; int matrix[3][4]; char name[16]; };`,
    /// cross-checked against `objdump --dwarf=info` first: each array
    /// member's `DW_TAG_array_type` has one `DW_TAG_subrange_type` child per
    /// dimension, each carrying `DW_AT_upper_bound` (not `DW_AT_count`) --
    /// `9`/`{2,3}`/`15` for `arr`/`matrix`/`name` respectively, i.e. this
    /// compiler always emits the "inclusive upper bound" form, exercising
    /// `array_subrange_dimensions`' `+ 1` fallback path, not its
    /// `DW_AT_count` branch.
    #[test]
    fn analyze_types_resolves_array_member_type_names() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_array_test.elf");
        assert!(
            path.is_file(),
            "missing fixture {} (rebuild: gcc -g -O0 -o x64_dyn_array_test.elf array_test.c)",
            path.display()
        );
        let binary = LoadedBinary::from_file(&path).expect("load array test ELF");
        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(analyzer.has_debug_info());

        let types = analyzer.analyze_types();
        let with_arrays = types
            .iter()
            .find(|t| t.name == "WithArrays")
            .unwrap_or_else(|| panic!("expected a \"WithArrays\" struct in {types:?}"));
        assert_eq!(with_arrays.kind, "struct");

        let type_name_of = |field: &str| -> &str {
            with_arrays
                .members
                .iter()
                .find(|m| m.name == field)
                .unwrap_or_else(|| panic!("expected field {field} in {:?}", with_arrays.members))
                .type_name
                .as_str()
        };
        assert_eq!(type_name_of("arr"), "int[10]");
        assert_eq!(type_name_of("matrix"), "int[3][4]");
        assert_eq!(type_name_of("name"), "char[16]");
    }

    /// `testdata/x64_dyn_lexblock_test.elf`: GCC-compiled from a source with
    /// three nested `{ int total = ...; ... }` blocks, each shadowing the
    /// outer `total` (compiled with `-Wshadow` to confirm GCC actually
    /// treats them as three distinct variables, not one reused slot).
    /// Cross-checked against `llvm-dwarfdump -v --debug-info`: the two
    /// nested `DW_TAG_lexical_block`s use `DW_AT_high_pc [DW_FORM_data8]`
    /// (the offset-from-low_pc form, same as `DW_TAG_subprogram`'s
    /// `high_pc` -- confirming `lexical_block_range` needs the identical
    /// form handling `subprogram_size` already has), resolving to
    /// `[0x401113, 0x40113e)` for the outer block and `[0x401125, 0x401138)`
    /// for the inner one.
    #[test]
    fn analyze_functions_scopes_shadowed_locals_to_their_lexical_block() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_lexblock_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load lexblock test ELF");
        let analyzer = DwarfAnalyzer::new(&binary);
        let functions = analyzer.analyze_functions();
        let compute = functions
            .iter()
            .find(|f| f.name == "compute")
            .unwrap_or_else(|| panic!("expected a \"compute\" function in {functions:?}"));

        assert_eq!(compute.local_vars.len(), 3, "{:?}", compute.local_vars);
        let by_offset = |offset: i64| {
            compute
                .local_vars
                .iter()
                .find(|v| matches!(v.location, crate::loader::types::DwarfLocation::StackOffset(o) if o == offset))
                .unwrap_or_else(|| panic!("expected a local at fbreg {offset} in {:?}", compute.local_vars))
        };

        // Function-level `total` (line 2): not nested in any block.
        assert_eq!(by_offset(-20).scope, None);
        // Outer block's `total` (line 4).
        assert_eq!(by_offset(-24).scope, Some((0x401113, 0x40113e)));
        // Inner (if-body) block's `total` (line 7) -- the innermost
        // enclosing block wins, not the outer one it's also nested in.
        assert_eq!(by_offset(-28).scope, Some((0x401125, 0x401138)));
    }
}
