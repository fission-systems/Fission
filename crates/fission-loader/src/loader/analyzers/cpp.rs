//! C++ RTTI and Class Hierarchy Analyzer
//!
//! Reconstructs C++ class hierarchies, inheritance, and VTables
//! from Itanium and MSVC RTTI metadata.

use crate::loader::types::{InferredFieldInfo, InferredTypeInfo, LoadedBinary};
use crate::prelude::*;
use msvc_demangler;
use std::collections::HashMap;

/// C++ Class Information
#[derive(Debug, Clone)]
pub struct CppClassInfo {
    pub name: String,
    pub mangled_name: String,
    pub vtable_address: Option<u64>,
    pub base_classes: Vec<u64>, // Addresses of base class type_info
    pub type_info_address: u64,
}

pub struct CppAnalyzer<'a> {
    binary: &'a LoadedBinary,
}

impl<'a> CppAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        Self { binary }
    }

    /// Analyze C++ RTTI metadata to reconstruct class hierarchy
    pub fn analyze_classes(&self) -> Vec<CppClassInfo> {
        let mut classes = self.analyze_itanium_classes();
        classes.extend(self.analyze_msvc_classes());
        classes
    }

    fn analyze_itanium_classes(&self) -> Vec<CppClassInfo> {
        let mut classes = Vec::new();

        // 1. Find all Type Info symbols (_ZTI<name> -> type_info object,
        // _ZTV<name> -> vtable). `LoadedBinaryBuilder` demangles every
        // symbol name before it ever reaches this analyzer (see
        // `loader::demangle::demangle`, applied uniformly to
        // `iat_symbols`/`global_symbols`/`functions` regardless of format)
        // -- so `_ZTI1D`/`_ZTV1D` never appear as raw mangled strings here,
        // only as `cpp_demangle`'s own output: `"typeinfo for D"` and
        // `"{vtable(D)}"` respectively (confirmed against a real
        // `x86_64-linux-musl-g++` build; this loop used to match the raw
        // `"__ZTI"`/`"__ZTV"` mangled prefixes, which is why Itanium RTTI
        // discovery silently found zero classes on every real ELF binary).
        const TYPEINFO_PREFIX: &str = "typeinfo for ";
        const VTABLE_PREFIX: &str = "{vtable(";
        const VTABLE_SUFFIX: &str = ")}";

        let mut type_infos = HashMap::new();
        let mut vtables = HashMap::new();

        let mut collect = |name: &str, addr: u64| {
            if let Some(class_name) = name.strip_prefix(TYPEINFO_PREFIX) {
                type_infos.insert(class_name.to_string(), addr);
            } else if let Some(class_name) = name
                .strip_prefix(VTABLE_PREFIX)
                .and_then(|rest| rest.strip_suffix(VTABLE_SUFFIX))
            {
                vtables.insert(class_name.to_string(), addr);
            }
        };
        for (addr, name) in &self.binary.iat_symbols {
            collect(name, *addr);
        }
        for func in &self.binary.functions {
            collect(&func.name, func.address);
        }
        for (addr, name) in &self.binary.global_symbols {
            collect(name, *addr);
        }

        for (class_name, &ti_addr) in &type_infos {
            let vtable_address = vtables.get(class_name).cloned();

            if let Ok(info) = self.parse_itanium_type_info(ti_addr) {
                classes.push(CppClassInfo {
                    name: class_name.clone(),
                    mangled_name: format!("{TYPEINFO_PREFIX}{class_name}"),
                    vtable_address,
                    base_classes: info.base_type_infos,
                    type_info_address: ti_addr,
                });
            }
        }

        classes
    }

    fn analyze_msvc_classes(&self) -> Vec<CppClassInfo> {
        let mut classes = Vec::new();
        if !self.binary.format.starts_with("PE") {
            return classes;
        }

        let mut cols = HashMap::new();

        // 1. Find Complete Object Locators (COL) via symbols
        // ??_R4... -> Complete Object Locator
        for (addr, name) in &self.binary.iat_symbols {
            if name.starts_with("??_R4") {
                cols.insert(*addr, name.clone());
            }
        }
        for (addr, name) in &self.binary.global_symbols {
            if name.starts_with("??_R4") {
                cols.insert(*addr, name.clone());
            }
        }

        // 2. If no symbols, scan .rdata/.data for potential COLs
        if cols.is_empty() {
            let image_base = self.binary.image_base;
            let max_va = self
                .binary
                .sections
                .iter()
                .map(|s| s.virtual_address + s.virtual_size)
                .max()
                .unwrap_or(0);
            let max_rva = max_va.saturating_sub(image_base) as u32;

            for section in &self.binary.sections {
                if section.name == ".rdata" || section.name == ".data" {
                    let Some(data) = self
                        .binary
                        .view_bytes(section.virtual_address, section.virtual_size as usize)
                    else {
                        continue;
                    };

                    let step = 4;
                    let len = data.len();
                    if len < 24 {
                        continue;
                    }
                    for i in (0..=len - 24).step_by(step) {
                        let signature =
                            u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                        if signature != 0 && signature != 1 {
                            continue;
                        }

                        // Validate COL fields directly from section slice (zero-copy)
                        let offset = u32::from_le_bytes([
                            data[i + 4],
                            data[i + 5],
                            data[i + 6],
                            data[i + 7],
                        ]);
                        let cd_offset = u32::from_le_bytes([
                            data[i + 8],
                            data[i + 9],
                            data[i + 10],
                            data[i + 11],
                        ]);
                        let td_val = u32::from_le_bytes([
                            data[i + 12],
                            data[i + 13],
                            data[i + 14],
                            data[i + 15],
                        ]);
                        let chd_val = u32::from_le_bytes([
                            data[i + 16],
                            data[i + 17],
                            data[i + 18],
                            data[i + 19],
                        ]);

                        // Simple heuristics: offset and cd_offset are usually small
                        if offset > 0x100000 || cd_offset > 0x100000 {
                            continue;
                        }

                        let valid = if signature == 1 {
                            // x64: td_val and chd_val are RVAs
                            td_val > 0 && td_val < max_rva && chd_val > 0 && chd_val < max_rva
                        } else {
                            // x86: td_val and chd_val are absolute VAs
                            let td_val64 = td_val as u64;
                            let chd_val64 = chd_val as u64;
                            td_val64 > image_base
                                && td_val64 < max_va
                                && chd_val64 > image_base
                                && chd_val64 < max_va
                        };

                        if !valid {
                            continue;
                        }

                        // Potential COL found! Verify and parse it.
                        let addr = section.virtual_address + i as u64;
                        if self.parse_msvc_col(addr).is_ok() {
                            cols.insert(addr, format!("??_R4_auto_{:x}", addr));
                        }
                    }
                }
            }
        }

        for (&addr, mangled) in &cols {
            if let Ok(col) = self.parse_msvc_col(addr) {
                let base_classes = col.base_classes;

                classes.push(CppClassInfo {
                    name: col.class_name,
                    mangled_name: mangled.clone(),
                    vtable_address: None, // Will be linked later
                    base_classes,
                    type_info_address: addr,
                });
            }
        }

        classes
    }

    fn parse_msvc_col(&self, addr: u64) -> Result<MsvcColInfo> {
        let signature = self
            .binary
            .get_bytes(addr, 4)
            .and_then(|b| b.try_into().ok().map(u32::from_le_bytes))
            .ok_or_else(|| FissionError::loader("Failed to read COL signature"))?;

        let is_x64 = signature == 1;
        let image_base = self.binary.image_base;

        let (td_addr, chd_addr) = if is_x64 {
            // pTypeDescriptor (RVA), pClassDescriptor (RVA)
            let td_rva = u32::from_le_bytes(
                self.binary
                    .get_bytes(addr + 12, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read TD RVA"))?,
            );
            let chd_rva = u32::from_le_bytes(
                self.binary
                    .get_bytes(addr + 16, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read CHD RVA"))?,
            );
            (image_base + td_rva as u64, image_base + chd_rva as u64)
        } else {
            // pTypeDescriptor (VA), pClassDescriptor (VA)
            let td_va = u32::from_le_bytes(
                self.binary
                    .get_bytes(addr + 12, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read TD VA"))?,
            );
            let chd_va = u32::from_le_bytes(
                self.binary
                    .get_bytes(addr + 16, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read CHD VA"))?,
            );
            (td_va as u64, chd_va as u64)
        };

        // Parse Type Descriptor
        // vtable, spare, name
        let name_offset = if self.binary.is_64bit { 16 } else { 8 };
        let mut name_bytes = Vec::new();
        let mut curr = td_addr + name_offset;
        loop {
            let b = self
                .binary
                .get_bytes(curr, 1)
                .ok_or_else(|| FissionError::loader(".."))?[0];
            if b == 0 {
                break;
            }
            name_bytes.push(b);
            curr += 1;
            if name_bytes.len() > 256 {
                break;
            } // Safety
        }
        let mangled_name = String::from_utf8_lossy(&name_bytes).to_string();
        let class_name =
            msvc_demangler::demangle(&mangled_name, msvc_demangler::DemangleFlags::NAME_ONLY)
                .unwrap_or_else(|_| mangled_name.clone());

        // Parse Class Hierarchy Descriptor
        let num_bases = u32::from_le_bytes(
            self.binary
                .get_bytes(chd_addr + 8, 4)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| FissionError::loader("Failed to read number of base classes"))?,
        );
        let base_array_addr = if is_x64 {
            let rva = u32::from_le_bytes(
                self.binary
                    .get_bytes(chd_addr + 12, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read base array RVA"))?,
            );
            image_base + rva as u64
        } else {
            let va = u32::from_le_bytes(
                self.binary
                    .get_bytes(chd_addr + 12, 4)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| FissionError::loader("Failed to read base array VA"))?,
            );
            va as u64
        };

        let mut base_classes = Vec::new();
        // Index 0 is the class itself, so start from 1
        for i in 1..num_bases {
            let bcd_addr = if is_x64 {
                let rva = u32::from_le_bytes(
                    self.binary
                        .get_bytes(base_array_addr + (i * 4) as u64, 4)
                        .and_then(|b| b.try_into().ok())
                        .ok_or_else(|| FissionError::loader("Failed to read BCD RVA"))?,
                );
                image_base + rva as u64
            } else {
                let va = u32::from_le_bytes(
                    self.binary
                        .get_bytes(base_array_addr + (i * 4) as u64, 4)
                        .and_then(|b| b.try_into().ok())
                        .ok_or_else(|| FissionError::loader("Failed to read BCD VA"))?,
                );
                va as u64
            };

            // BCD points to Type Descriptor
            let bcd_td_addr = if is_x64 {
                let rva = u32::from_le_bytes(
                    self.binary
                        .get_bytes(bcd_addr, 4)
                        .and_then(|b| b.try_into().ok())
                        .ok_or_else(|| FissionError::loader("Failed to read BCD TD RVA"))?,
                );
                image_base + rva as u64
            } else {
                let va = u32::from_le_bytes(
                    self.binary
                        .get_bytes(bcd_addr, 4)
                        .and_then(|b| b.try_into().ok())
                        .ok_or_else(|| FissionError::loader("Failed to read BCD TD VA"))?,
                );
                va as u64
            };
            base_classes.push(bcd_td_addr);
        }

        Ok(MsvcColInfo {
            class_name,
            base_classes,
        })
    }

    /// Parse Itanium ABI type_info object
    fn parse_itanium_type_info(&self, addr: u64) -> Result<ItaniumTypeInfo> {
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        let vtable_ptr = self.binary.read_ptr(addr)?;
        let name_ptr = self.binary.read_ptr(addr + ptr_size)?;

        let mut base_type_infos = Vec::new();

        // For a dynamically-linked binary, `__si_class_type_info`'s/
        // `__vmi_class_type_info`'s own vtable lives in libstdc++ (an
        // external DSO), not this binary -- so the `vtable_ptr` field just
        // read is an *unrelocated on-disk placeholder*, almost always
        // literally `0` (ELF RELA relocations carry their addend in the
        // relocation entry itself, not in-place, so the linker leaves the
        // slot's own bytes zeroed). Checked first, before any value-based
        // lookup below: `0` (or any other placeholder) can coincidentally
        // collide with an unrelated real symbol at that address -- e.g.
        // this loader's own synthetic `"ELF_HEADER"` marker at address
        // `0` -- silently masking the correct answer. The field's *slot*
        // (`addr`, this type_info's own address) is where the loader's
        // relocation table actually names the target (an `R_X86_64_64`
        // entry here, same "resolve by slot address" pattern
        // `elf/lsda.rs`'s `symbol_at` closure already uses for LSDA
        // type-table entries). Confirmed against a real dynamically-linked
        // `x86_64-linux-musl-g++` build: `relocation_symbols[addr]`
        // resolves to `"{vtable(__cxxabiv1::__vmi_class_type_info)}"`.
        let mut vt_name_found = self.binary.relocation_symbols.get(&addr).cloned();

        if vt_name_found.is_none() {
            vt_name_found = self.binary.iat_symbols.get(&vtable_ptr).cloned();
        }
        if vt_name_found.is_none() {
            vt_name_found = self.binary.global_symbols.get(&vtable_ptr).cloned();
        }

        // On Mach-O AARCH64, these might be in GOT
        if vt_name_found.is_none() {
            for (got_addr, sym_name) in &self.binary.iat_symbols {
                if *got_addr == vtable_ptr {
                    vt_name_found = Some(sym_name.clone());
                    break;
                }
            }
        }

        if let Some(vt_name) = vt_name_found {
            if vt_name.contains("__si_class_type_info") {
                let base_ti = self.binary.read_ptr(addr + 2 * ptr_size)?;
                base_type_infos.push(base_ti);
            } else if vt_name.contains("__vmi_class_type_info") {
                // __vmi_class_type_info (multiple and/or virtual inheritance):
                // base __class_type_info (vtable_ptr, name_ptr), then
                // `unsigned int flags`, `unsigned int base_count`, then
                // `base_count` `__base_class_type_info` entries, each
                // `{ __class_type_info *base_type; long offset_flags; }` --
                // a base's own type_info pointer followed by a pointer-sized
                // word packing is-virtual (bit 0) / is-public (bit 1) / byte
                // offset (bits 8+, signed -- for a virtual base this is a
                // vcall-offset into the vtable, not a direct object offset,
                // so it can be negative) into one field. Verified byte-for-
                // byte against a real `x86_64-linux-musl-g++` build (both a
                // static and a `-fPIE` binary) with a two-base non-virtual
                // diamond leg (`struct D : B, C`) and a single virtual base
                // (`struct E : virtual A`): flags/base_count/each base's
                // type_info address, and (for the virtual case) the
                // is-virtual bit and the negative vcall-offset all decoded
                // exactly as this layout predicts.
                //
                // Only the base type_info addresses are surfaced here (not
                // the offset/virtual bits) -- `CppClassInfo::base_classes`
                // is already just a flat `Vec<u64>` of base addresses, the
                // same shape `parse_msvc_col`'s MSVC-side base array already
                // produces, so this matches existing consumption without
                // inventing a richer type nothing downstream reads yet.
                let base_count = self
                    .binary
                    .get_bytes(addr + 2 * ptr_size + 4, 4)
                    .and_then(|b| b.try_into().ok())
                    .map(u32::from_le_bytes)
                    .unwrap_or(0);
                let base_array_addr = addr + 2 * ptr_size + 8;
                let entry_size = 2 * ptr_size;
                for i in 0..u64::from(base_count) {
                    let Ok(base_ti) = self.binary.read_ptr(base_array_addr + i * entry_size) else {
                        break;
                    };
                    base_type_infos.push(base_ti);
                }
            }
        }

        Ok(ItaniumTypeInfo {
            _vtable_ptr: vtable_ptr,
            _name_ptr: name_ptr,
            base_type_infos,
        })
    }

    /// Resolves a base class's display name from its type_info address,
    /// via a direct symbol lookup, falling back to a `function_addr_index`
    /// lookup (matching the two-step fallback `parse_itanium_type_info`
    /// itself uses for the vtable-pointer-to-symbol-name lookup), and
    /// finally an unresolved `base@0x...` placeholder.
    fn resolve_base_class_name(&self, base_ti: u64) -> String {
        // Itanium: `iat_symbols`/`global_symbols` hold `demangle()`'s own
        // output (`"typeinfo for X"`), not a raw `_ZTI`/`__ZTI` mangled
        // string -- see `analyze_itanium_classes`'s doc comment.
        for table in [&self.binary.iat_symbols, &self.binary.global_symbols] {
            if let Some(name) = table.get(&base_ti) {
                if let Some(class_name) = name.strip_prefix("typeinfo for ") {
                    return class_name.to_string();
                } else if name.starts_with("??_R0") || name.starts_with(".?AV") {
                    return msvc_demangler::demangle(
                        name,
                        msvc_demangler::DemangleFlags::NAME_ONLY,
                    )
                    .unwrap_or_else(|_| name.clone());
                }
            }
        }
        if let Some(&idx) = self.binary.function_addr_index.get(&base_ti) {
            let sym = &self.binary.functions[idx];
            if let Some(class_name) = sym.name.strip_prefix("typeinfo for ") {
                return class_name.to_string();
            } else if sym.name.starts_with("??_R0") || sym.name.starts_with(".?AV") {
                return msvc_demangler::demangle(
                    &sym.name,
                    msvc_demangler::DemangleFlags::NAME_ONLY,
                )
                .unwrap_or_else(|_| sym.name.clone());
            }
        }
        format!("base@0x{base_ti:X}")
    }

    pub fn to_inferred_types(&self) -> Vec<InferredTypeInfo> {
        let classes = self.analyze_classes();
        classes
            .into_iter()
            .map(|c| {
                let mut fields = Vec::new();

                // If it has a vtable, add a vtable pointer field at offset 0
                if c.vtable_address.is_some() {
                    fields.push(InferredFieldInfo {
                        name: "__vptr".to_string(),
                        type_name: "void**".to_string(),
                        offset: 0,
                        size: if self.binary.is_64bit { 8 } else { 4 },
                    });
                }

                InferredTypeInfo {
                    name: if !c.base_classes.is_empty() {
                        // One or more bases (single-inheritance -> exactly
                        // one; __vmi_class_type_info multiple/virtual
                        // inheritance -> one per base_classes entry).
                        let base_names: Vec<String> = c
                            .base_classes
                            .iter()
                            .map(|&base_ti| self.resolve_base_class_name(base_ti))
                            .collect();
                        format!("{} : public {}", c.name, base_names.join(", public "))
                    } else {
                        c.name.clone()
                    },
                    mangled_name: c.mangled_name,
                    kind: "CppClass".to_string(),
                    fields,
                    size: 0,
                    metadata_address: c.type_info_address,
                }
            })
            .collect()
    }

    /// Discovers functions by sweeping VTable payloads.
    pub fn discover_vtable_functions(&self) -> Vec<crate::loader::types::FunctionInfo> {
        let mut functions = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        // Helper closure to sweep a VTable
        let mut sweep_vtable = |start_addr: u64| {
            let mut current_addr = start_addr;
            loop {
                let Ok(ptr) = self.binary.read_ptr(current_addr) else {
                    break;
                };
                if ptr == 0 {
                    break;
                }

                // Check if the pointer falls within an executable section
                let mut is_executable = false;
                for sec in &self.binary.sections {
                    if ptr >= sec.virtual_address && ptr < sec.virtual_address + sec.virtual_size {
                        if sec.is_executable {
                            is_executable = true;
                        }
                        break;
                    }
                }

                if !is_executable {
                    break;
                }

                functions.push(crate::loader::types::FunctionInfo {
                    address: ptr,
                    name: format!("vfunc_{:x}", ptr),
                    size: 0,
                    is_export: false,
                    is_import: false,
                    origin: Some("cpp_rtti".to_string()),
                    kind: Some("vtable".to_string()),
                    ..Default::default()
                });

                current_addr += ptr_size;
            }
        };

        // 1. Sweep Itanium VTables
        // In Itanium, __ZTV points to the start of the vtable array.
        // Index 0: offset_to_top, Index 1: typeinfo ptr, Index 2: first vfunc.
        let mut itanium_vtables = Vec::new();
        for (addr, name) in &self.binary.iat_symbols {
            if name.starts_with("__ZTV") {
                itanium_vtables.push(*addr);
            }
        }
        for func in &self.binary.functions {
            if func.name.starts_with("__ZTV") {
                itanium_vtables.push(func.address);
            }
        }
        for (addr, name) in &self.binary.global_symbols {
            if name.starts_with("__ZTV") {
                itanium_vtables.push(*addr);
            }
        }

        for vt_addr in itanium_vtables {
            let func_array_start = vt_addr + 2 * ptr_size;
            sweep_vtable(func_array_start);
        }

        // 2. Sweep MSVC VTables
        // In MSVC, the COL (??_R4) is pointed to by the entry immediately preceding the VTable.
        if self.binary.format.starts_with("PE") {
            let msvc_classes = self.analyze_msvc_classes();
            for class in msvc_classes {
                let col_addr = class.type_info_address;

                // Scan .rdata for pointers to this COL
                for section in &self.binary.sections {
                    if section.name == ".rdata" || section.name == ".data" {
                        let Some(data) = self
                            .binary
                            .view_bytes(section.virtual_address, section.virtual_size as usize)
                        else {
                            continue;
                        };

                        let step = ptr_size as usize;
                        if data.len() < step {
                            continue;
                        }
                        for i in (0..=data.len() - step).step_by(step) {
                            let val = if ptr_size == 8 {
                                u64::from_le_bytes(data[i..i + 8].try_into().unwrap())
                            } else {
                                u32::from_le_bytes(data[i..i + 4].try_into().unwrap()) as u64
                            };

                            if val == col_addr {
                                // Found a pointer to COL. The VTable starts immediately after this pointer.
                                let vtable_start = section.virtual_address + (i as u64) + ptr_size;
                                sweep_vtable(vtable_start);
                            }
                        }
                    }
                }
            }
        }

        functions.sort_by_key(|f| f.address);
        functions.dedup_by_key(|f| f.address);
        functions
    }
}

struct ItaniumTypeInfo {
    _vtable_ptr: u64,
    _name_ptr: u64,
    base_type_infos: Vec<u64>,
}

struct MsvcColInfo {
    class_name: String,
    base_classes: Vec<u64>, // Addresses of base class type descriptors
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `x86_64-linux-musl-g++`-compiled (`-fPIE`) fixture:
    /// `struct D : public B, public C` (two-base, non-virtual multiple
    /// inheritance -- a "non-diamond repeat" of `A` via both `B` and `C`)
    /// and `struct E : public virtual A` (single virtual base). Both use
    /// `__vmi_class_type_info`, not `__si_class_type_info` -- `D` because
    /// it has more than one base, `E` because its one base is virtual.
    /// Addresses cross-checked against `nm`/`objdump -s` byte-for-byte
    /// before trusting this test (see `parse_itanium_type_info`'s doc
    /// comment on the `__vmi_class_type_info` branch for the full layout
    /// derivation).
    #[test]
    fn analyze_itanium_classes_resolves_vmi_multiple_and_virtual_bases() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_vmi_rtti_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load VMI RTTI test ELF");
        let analyzer = CppAnalyzer::new(&binary);
        let classes = analyzer.analyze_itanium_classes();

        let d = classes
            .iter()
            .find(|c| c.name == "D")
            .unwrap_or_else(|| panic!("no class D in {classes:#x?}"));
        // B @ 0x2d88, C @ 0x2d70, in declaration order.
        assert_eq!(d.base_classes, vec![0x2d88, 0x2d70]);

        let e = classes
            .iter()
            .find(|c| c.name == "E")
            .unwrap_or_else(|| panic!("no class E in {classes:#x?}"));
        // A @ 0x2da0, via a single *virtual* base -- still a VMI type_info
        // (base_count == 1 doesn't imply __si_class_type_info once the
        // base is virtual), and its address must still be captured even
        // though the virtual/offset bits in `offset_flags` aren't decoded.
        assert_eq!(e.base_classes, vec![0x2da0]);

        // B/C : public A -- plain (non-virtual, single-base)
        // __si_class_type_info, confirming the pre-existing SI path still
        // resolves correctly through the same rewritten discovery loop
        // and the relocation-symbols-first `vt_name_found` lookup.
        let b = classes
            .iter()
            .find(|c| c.name == "B")
            .unwrap_or_else(|| panic!("no class B in {classes:#x?}"));
        assert_eq!(b.base_classes, vec![0x2da0]);
    }

    #[test]
    fn to_inferred_types_names_all_vmi_bases_not_just_the_first() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_vmi_rtti_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load VMI RTTI test ELF");
        let analyzer = CppAnalyzer::new(&binary);
        let inferred = analyzer.to_inferred_types();

        let d = inferred
            .iter()
            .find(|t| t.name.starts_with("D "))
            .unwrap_or_else(|| panic!("no D entry in {inferred:#x?}"));
        assert_eq!(d.name, "D : public B, public C");
    }
}
