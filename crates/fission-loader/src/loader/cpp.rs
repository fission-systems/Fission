//! C++ RTTI and Class Hierarchy Analyzer
//!
//! Reconstructs C++ class hierarchies, inheritance, and VTables
//! from Itanium and MSVC RTTI metadata.

use crate::loader::types::{InferredFieldInfo, InferredTypeInfo, LoadedBinary};
use crate::prelude::*;
use msvc_demangler;
use rustc_demangle::demangle;
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

        // 1. Find all Type Info symbols (_ZTI)
        // In Mach-O/ELF using Itanium ABI:
        // __ZTI<name> -> type_info object
        // __ZTS<name> -> name string
        // __ZTV<name> -> vtable

        let mut type_infos = HashMap::new();
        let mut vtables = HashMap::new();

        for (addr, name) in &self.binary.iat_symbols {
            if name.starts_with("__ZTI") {
                type_infos.insert(name.clone(), *addr);
            } else if name.starts_with("__ZTV") {
                vtables.insert(name.clone(), *addr);
            }
        }

        // Also check global symbols and function names (some might be labels)
        for func in &self.binary.functions {
            if func.name.starts_with("__ZTI") {
                type_infos.insert(func.name.clone(), func.address);
            } else if func.name.starts_with("__ZTV") {
                vtables.insert(func.name.clone(), func.address);
            }
        }

        for (addr, name) in &self.binary.global_symbols {
            if name.starts_with("__ZTI") {
                type_infos.insert(name.clone(), *addr);
            } else if name.starts_with("__ZTV") {
                vtables.insert(name.clone(), *addr);
            }
        }

        for (mangled, &ti_addr) in &type_infos {
            let demangled = format!("{:?}", demangle(mangled));
            let class_name_clean = demangled.trim_start_matches("typeinfo for ").to_string();

            let vtable_name = format!("__ZTV{}", mangled.trim_start_matches("__ZTI"));
            let vtable_address = vtables.get(&vtable_name).cloned();

            if let Ok(info) = self.parse_itanium_type_info(ti_addr) {
                let mut base_classes = Vec::new();
                if let Some(base_ti) = info.base_type_info {
                    base_classes.push(base_ti);
                }

                classes.push(CppClassInfo {
                    name: class_name_clean,
                    mangled_name: mangled.clone(),
                    vtable_address,
                    base_classes,
                    type_info_address: ti_addr,
                });
            }
        }

        classes
    }

    fn analyze_msvc_classes(&self) -> Vec<CppClassInfo> {
        let mut classes = Vec::new();
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

        // 2. If no symbols, scan .rdata for potential COLs
        if cols.is_empty() {
            for section in &self.binary.sections {
                if section.name == ".rdata" || section.name == ".data" {
                    let data = match self
                        .binary
                        .view_bytes(section.virtual_address, section.virtual_size as usize)
                    {
                        Some(d) => d,
                        None => continue,
                    };

                    let step = 4;
                    for i in (0..(data.len().saturating_sub(24))).step_by(step) {
                        // Safe: loop bounds ensure i+4 is valid
                        let signature = match data.get(i..i + 4) {
                            Some(bytes) => {
                                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                            }
                            None => continue, // Skip if bounds check fails
                        };
                        if signature == 0 || signature == 1 {
                            // Potential COL
                            let addr = section.virtual_address + i as u64;
                            if self.parse_msvc_col(addr).is_ok() {
                                cols.insert(addr, format!("??_R4_auto_{:x}", addr));
                            }
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

        let mut base_type_info = None;

        let mut vt_name_found = self.binary.iat_symbols.get(&vtable_ptr).cloned();
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
                base_type_info = Some(base_ti);
            }
        }

        Ok(ItaniumTypeInfo {
            _vtable_ptr: vtable_ptr,
            _name_ptr: name_ptr,
            base_type_info,
        })
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
                        let mut base_name = format!("base@0x{:X}", c.base_classes[0]);
                        // Try to find symbols for the base
                        for (&addr, name) in &self.binary.iat_symbols {
                            if addr == c.base_classes[0] {
                                if name.starts_with("__ZTI") {
                                    base_name = format!("{:?}", demangle(name));
                                    break;
                                } else if name.starts_with("??_R0") || name.starts_with(".?AV") {
                                    base_name = msvc_demangler::demangle(
                                        name,
                                        msvc_demangler::DemangleFlags::NAME_ONLY,
                                    )
                                    .unwrap_or_else(|_| name.clone());
                                    break;
                                }
                            }
                        }
                        if base_name.starts_with("base@") {
                            for sym in &self.binary.functions {
                                if sym.address == c.base_classes[0] {
                                    if sym.name.starts_with("__ZTI") {
                                        base_name = format!("{:?}", demangle(&sym.name));
                                        break;
                                    } else if sym.name.starts_with("??_R0")
                                        || sym.name.starts_with(".?AV")
                                    {
                                        base_name = msvc_demangler::demangle(
                                            &sym.name,
                                            msvc_demangler::DemangleFlags::NAME_ONLY,
                                        )
                                        .unwrap_or_else(|_| sym.name.clone());
                                        break;
                                    }
                                }
                            }
                        }
                        format!("{} : public {}", c.name, base_name)
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
}

struct ItaniumTypeInfo {
    _vtable_ptr: u64,
    _name_ptr: u64,
    base_type_info: Option<u64>,
}

struct MsvcColInfo {
    class_name: String,
    base_classes: Vec<u64>, // Addresses of base class type descriptors
}
