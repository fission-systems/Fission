//! DWARF Function Information Extraction
//!
//! Extracts function names, parameters, return types, and local variables
//! from DWARF debug information.

use crate::loader::types::{DwarfFunctionInfo, DwarfLocalVar, DwarfLocation, DwarfParamInfo};
use gimli::{DebuggingInformationEntry, DwAt, DwTag, EndianSlice, RunTimeEndian, UnitOffset};
use std::collections::HashMap;

/// Internal helper for building DwarfFunctionInfo during DFS
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

/// Function extraction methods for DwarfAnalyzer
impl<'a> super::analyzer::DwarfAnalyzer<'a> {
    /// Extract all function information from DWARF
    pub(super) fn analyze_functions_inner(&self) -> Result<Vec<DwarfFunctionInfo>, gimli::Error> {
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
                        // Note: func_depth > 0 guarantees current_func is Some
                        let Some(func) = current_func.as_mut() else {
                            // This should never happen if func_depth tracking is correct
                            log::warn!("Inconsistent DWARF function depth tracking");
                            continue;
                        };
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

    /// Extract parameter information from a DW_TAG_formal_parameter DIE
    pub(super) fn extract_param_info(
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

    /// Extract local variable information from a DW_TAG_variable DIE
    pub(super) fn extract_local_var_info(
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

    /// Extract DW_AT_location → DwarfLocation
    pub(super) fn extract_location(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<DwarfLocation, gimli::Error> {
        match entry.attr_value(DwAt(0x02))? {
            Some(gimli::AttributeValue::Exprloc(expr)) => self.parse_location_expr(expr, unit),
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
                gimli::Operation::FrameOffset { offset } => Ok(DwarfLocation::StackOffset(offset)),
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
