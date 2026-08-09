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
    size: u64,
}

impl FuncBuilder {
    fn build(self) -> Option<DwarfFunctionInfo> {
        Some(DwarfFunctionInfo {
            address: self.address,
            name: self.name,
            return_type: self.return_type,
            params: self.params,
            local_vars: self.local_vars,
            size: self.size,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocationListEntryKind {
    Register(u64),
    ConstantValue,
    Other,
}

/// Select the one register that carries a variable after an optional
/// constant-valued prefix. Optimized code commonly keeps a local as a DWARF
/// stack-value constant until its first update, then materializes it in one
/// register. That is one variable identity, not register reuse. Constants
/// after materialization, a non-constant computed location, or two distinct
/// registers remain ambiguous and are rejected.
fn register_after_leading_constants(entries: &[LocationListEntryKind]) -> Option<u64> {
    let mut agreed_register = None;
    for entry in entries {
        match *entry {
            LocationListEntryKind::ConstantValue if agreed_register.is_none() => {}
            LocationListEntryKind::Register(register) => match agreed_register {
                None => agreed_register = Some(register),
                Some(previous) if previous == register => {}
                Some(_) => return None,
            },
            LocationListEntryKind::ConstantValue | LocationListEntryKind::Other => return None,
        }
    }
    agreed_register
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
            // Stack of enclosing `DW_TAG_lexical_block` PC ranges, each tagged
            // with the `func_depth` value *at that block's own DIE* (mirrors
            // how `func_depth == 1` marks the subprogram's own depth below).
            // A variable's scope is the top of this stack at the moment it's
            // visited -- the innermost block it's nested in, or `None` when
            // the stack is empty (declared directly under the function).
            let mut scope_stack: Vec<(isize, u64, u64)> = Vec::new();

            while let Some((delta_depth, entry)) = entries.next_dfs()? {
                if current_func.is_some() {
                    // We're inside a subprogram — track depth relative to the function DIE.
                    // `func_depth` was set to 1 *at* the subprogram's own tag (see below), so
                    // a direct child reports delta=+1 and lands at 2; staying at 1 means this
                    // entry is back at the subprogram's *own* depth -- a sibling, not a child
                    // (e.g. a trailing type DIE GCC places after a function's last real child,
                    // immediately followed by the next DW_TAG_subprogram). The threshold must
                    // be `<= 1`, not `<= 0`: with `<= 0`, a sibling at func_depth==1 was
                    // wrongly treated as still "inside" the current subprogram, silently
                    // folding the next function's own tag (unmatched by any case in the
                    // children match below) and all of its params/locals into the current one.
                    func_depth += delta_depth;

                    if func_depth <= 1 {
                        // We've exited the subprogram — finalize it
                        if let Some(func) = current_func.take() {
                            if let Some(fi) = func.build() {
                                functions.push(fi);
                            }
                        }
                        scope_stack.clear();
                        // Fall through to check if this entry is another subprogram
                    } else {
                        // Pop any lexical_block scopes whose own depth is at or
                        // past the depth we've DFS'd back to -- we've exited them.
                        while scope_stack.last().is_some_and(|&(d, _, _)| d >= func_depth) {
                            scope_stack.pop();
                        }

                        // Process children of the current subprogram
                        // Note: func_depth > 1 guarantees current_func is Some
                        let Some(func) = current_func.as_mut() else {
                            // This should never happen if func_depth tracking is correct
                            tracing::warn!("Inconsistent DWARF function depth tracking");
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
                                if let Some(mut var) =
                                    self.extract_local_var_info(entry, &unit, &dwarf, &type_cache)?
                                {
                                    var.scope = scope_stack.last().map(|&(_, lo, hi)| (lo, hi));
                                    func.local_vars.push(var);
                                }
                            }
                            DwTag(0x0b) => {
                                // DW_TAG_lexical_block
                                if let Some((lo, hi)) = self.lexical_block_range(entry)? {
                                    scope_stack.push((func_depth, lo, hi));
                                }
                            }
                            _ => {}
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
                    let size = self.subprogram_size(entry, address)?;

                    current_func = Some(FuncBuilder {
                        address,
                        name,
                        return_type,
                        params: Vec::new(),
                        local_vars: Vec::new(),
                        size,
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

    /// Resolve a `DW_TAG_subprogram`'s size from `DW_AT_high_pc`.
    ///
    /// Per the DWARF spec, `DW_AT_high_pc` is either an absolute address
    /// (`DW_FORM_addr`/`DW_FORM_addrx*`, same as `DW_AT_low_pc`) or an
    /// unsigned constant *offset from* `DW_AT_low_pc` (`DW_FORM_data*`/
    /// `DW_FORM_udata`) -- compilers are free to pick either, and modern GCC
    /// (observed: GCC 16) emits the offset form. Reading the raw integer
    /// through `get_attr_u64` without checking the form silently produces the
    /// wrong end address for offset-form high_pc (a small byte count, far
    /// below `low_pc`), which used to collapse `size` to `0` via the
    /// `high > address` guard -- and a size-0 function can be mismatched
    /// against a *later* function's address by whatever consumes
    /// `DwarfFunctionInfo` by address range.
    fn subprogram_size(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        low_pc: u64,
    ) -> Result<u64, gimli::Error> {
        let end = match entry.attr_value(DwAt(0x12))? {
            Some(gimli::AttributeValue::Addr(v)) => v,
            Some(gimli::AttributeValue::Udata(v)) => low_pc.saturating_add(v),
            Some(gimli::AttributeValue::Data1(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data2(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data4(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data8(v)) => low_pc.saturating_add(v),
            _ => return Ok(0),
        };
        Ok(end.saturating_sub(low_pc))
    }

    /// Resolve a `DW_TAG_lexical_block`'s PC range from `DW_AT_low_pc`/
    /// `DW_AT_high_pc`, applying the same offset-vs-absolute `high_pc` form
    /// handling as `subprogram_size` -- confirmed via `llvm-dwarfdump -v`
    /// that GCC emits the offset form (`DW_FORM_data8`) here too, not just
    /// on `DW_TAG_subprogram`. Returns `None` for a block with no low_pc, or
    /// one using `DW_AT_ranges` for non-contiguous PCs instead of a single
    /// low/high pair (GCC does this under heavier optimization when a
    /// block's code gets split; not attempted here -- such a variable's
    /// scope degrades to `None`, same as being unscoped, rather than a
    /// wrong range).
    fn lexical_block_range(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<Option<(u64, u64)>, gimli::Error> {
        let Some(low_pc) = self.get_attr_u64(entry, DwAt(0x11))? else {
            return Ok(None);
        };
        let high_pc = match entry.attr_value(DwAt(0x12))? {
            Some(gimli::AttributeValue::Addr(v)) => v,
            Some(gimli::AttributeValue::Udata(v)) => low_pc.saturating_add(v),
            Some(gimli::AttributeValue::Data1(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data2(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data4(v)) => low_pc.saturating_add(u64::from(v)),
            Some(gimli::AttributeValue::Data8(v)) => low_pc.saturating_add(v),
            _ => return Ok(None),
        };
        Ok(Some((low_pc, high_pc)))
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

        let location = self.extract_location(entry, unit, dwarf)?;

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

        let location = self.extract_location(entry, unit, dwarf)?;

        Ok(Some(DwarfLocalVar {
            name,
            type_name,
            location,
            // Filled in by the caller, which knows the current scope_stack;
            // this function only has the DIE, not the DFS traversal state.
            scope: None,
        }))
    }

    /// Extract DW_AT_location → DwarfLocation
    ///
    /// Handles both a bare `Exprloc` (location is invariant for the whole
    /// declared scope) and a location list (`LocListsRef` / `DebugLocListsIndex`
    /// — location varies across PC ranges, the common shape real compilers
    /// emit for locals even when the underlying storage never actually
    /// changes). For a location list we report `DwarfLocation::Register` when
    /// every range resolves to the exact same register, or when a prefix of
    /// pure constant stack-values is followed by ranges in one register. The
    /// latter is the canonical optimized induction-local shape: the variable
    /// is constant until its first update, then gains physical storage. A
    /// constant after materialization, an `entry_value`/computed location, or
    /// disagreement between registers remains `Unknown` -- a missed rename,
    /// never a wrong one.
    pub(super) fn extract_location(
        &self,
        entry: &DebuggingInformationEntry<EndianSlice<'a, RunTimeEndian>, usize>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
    ) -> Result<DwarfLocation, gimli::Error> {
        match entry.attr_value(DwAt(0x02))? {
            Some(gimli::AttributeValue::Exprloc(expr)) => self.parse_location_expr(expr, unit),
            Some(
                attr @ (gimli::AttributeValue::LocationListsRef(_)
                | gimli::AttributeValue::DebugLocListsIndex(_)),
            ) => self.parse_location_list(attr, unit, dwarf),
            _ => Ok(DwarfLocation::Unknown),
        }
    }

    /// Resolve a location-list attribute to a single register under the
    /// agreement/leading-constant contract documented on `extract_location`.
    fn parse_location_list(
        &self,
        attr: gimli::AttributeValue<EndianSlice<'a, RunTimeEndian>>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
        dwarf: &gimli::Dwarf<EndianSlice<'a, RunTimeEndian>>,
    ) -> Result<DwarfLocation, gimli::Error> {
        let Some(mut iter) = dwarf.attr_locations(unit, attr)? else {
            return Ok(DwarfLocation::Unknown);
        };

        let mut entries = Vec::new();
        while let Some(list_entry) = iter.next()? {
            entries.push(self.classify_location_list_expr(list_entry.data, unit)?);
        }

        Ok(register_after_leading_constants(&entries)
            .map(|register| DwarfLocation::Register(format!("reg{register}")))
            .unwrap_or(DwarfLocation::Unknown))
    }

    fn classify_location_list_expr(
        &self,
        expr: gimli::Expression<EndianSlice<'a, RunTimeEndian>>,
        unit: &gimli::Unit<EndianSlice<'a, RunTimeEndian>, usize>,
    ) -> Result<LocationListEntryKind, gimli::Error> {
        let mut ops = expr.operations(unit.encoding());
        let first = ops.next()?;
        let second = ops.next()?;
        let third = ops.next()?;

        match (first, second, third) {
            (Some(gimli::Operation::Register { register }), None, None) => {
                Ok(LocationListEntryKind::Register(u64::from(register.0)))
            }
            (
                Some(gimli::Operation::RegisterOffset { register, .. }),
                Some(gimli::Operation::StackValue),
                None,
            ) if !matches!(register.0, 6 | 7 | 29 | 31) => {
                Ok(LocationListEntryKind::Register(u64::from(register.0)))
            }
            (
                Some(
                    gimli::Operation::UnsignedConstant { .. }
                    | gimli::Operation::SignedConstant { .. },
                ),
                Some(gimli::Operation::StackValue),
                None,
            ) => Ok(LocationListEntryKind::ConstantValue),
            _ => Ok(LocationListEntryKind::Other),
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

#[cfg(test)]
mod location_list_tests {
    use super::{LocationListEntryKind as Entry, register_after_leading_constants};

    #[test]
    fn accepts_constant_prefix_followed_by_one_register() {
        assert_eq!(
            register_after_leading_constants(&[
                Entry::ConstantValue,
                Entry::Register(2),
                Entry::Register(2),
            ]),
            Some(2)
        );
    }

    #[test]
    fn rejects_register_reuse_and_nonleading_constants() {
        assert_eq!(
            register_after_leading_constants(&[Entry::Register(2), Entry::Register(3)]),
            None
        );
        assert_eq!(
            register_after_leading_constants(&[Entry::Register(2), Entry::ConstantValue]),
            None
        );
        assert_eq!(
            register_after_leading_constants(&[Entry::ConstantValue, Entry::Other]),
            None
        );
    }
}
