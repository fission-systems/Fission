//! Rust-specific analysis for Fission
//! Extracts VTable and Trait information from Rust binaries.

use crate::loader::{LoadedBinary, types::InferredFieldInfo, types::InferredTypeInfo};

/// Rust type information extracted from vtables
#[derive(Debug, Clone)]
pub struct RustVTableInfo {
    pub address: u64,
    pub name: String,
    pub size: u64,
    pub align: u64,
    pub methods: Vec<u64>,
}

impl RustVTableInfo {
    pub fn to_inferred_type(&self) -> InferredTypeInfo {
        InferredTypeInfo {
            name: self.name.clone(),
            mangled_name: String::new(),
            kind: "rust_vtable".to_string(),
            fields: self
                .methods
                .iter()
                .enumerate()
                .map(|(i, &_addr)| InferredFieldInfo {
                    name: format!("vfunc_{}", i),
                    type_name: "fn*".to_string(),
                    offset: (i * 8) as u32,
                    size: 8,
                })
                .collect(),
            size: self.size as u32,
            metadata_address: self.address,
        }
    }
}

pub struct RustAnalyzer<'a> {
    binary: &'a LoadedBinary,
}

impl<'a> RustAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary) -> Self {
        Self { binary }
    }

    /// Analyze Rust vtables in the binary
    pub fn analyze_vtables(&self) -> Vec<RustVTableInfo> {
        let mut vtables = Vec::new();
        let ptr_size = if self.binary.is_64bit { 8 } else { 4 };

        // Search in sections likely to contain vtables
        for section in &self.binary.sections {
            if section.name == "__const"
                || section.name == ".rodata"
                || section.name == ".data.rel.ro"
            {
                let Some(data) = self
                    .binary
                    .view_bytes(section.virtual_address, section.virtual_size as usize)
                else {
                    continue;
                };

                let mut offset = 0;
                while offset + (ptr_size * 4) <= data.len() {
                    let addr = section.virtual_address + offset as u64;

                    if let Some(vtable) = self.try_parse_vtable(addr, ptr_size) {
                        let vtable_len = vtable.methods.len();
                        vtables.push(vtable);
                        // Skip the parsed vtable
                        offset += (ptr_size * 3) + (vtable_len * ptr_size);
                    } else {
                        offset += ptr_size;
                    }
                }
            }
        }

        for vt in &vtables {
            tracing::info!(
                "  - Found {}: {} methods, size={}, align={}",
                vt.name,
                vt.methods.len(),
                vt.size,
                vt.align
            );
        }

        tracing::info!(
            "[RustAnalyzer] Found {} potential Rust vtables",
            vtables.len()
        );
        vtables
    }

    fn try_parse_vtable(&self, addr: u64, ptr_size: usize) -> Option<RustVTableInfo> {
        // Rust vtable layout:
        // 0: pointer to drop_in_place
        // 1: size (usize)
        // 2: align (usize)
        // 3...: optional method pointers

        let Some(header) = self.binary.view_bytes(addr, ptr_size * 3) else {
            return None;
        };

        let drop_ptr = self.read_ptr(header, 0, ptr_size);
        let size = self.read_ptr(header, ptr_size, ptr_size);
        let align = self.read_ptr(header, ptr_size * 2, ptr_size);

        // Basic heuristic validation
        if drop_ptr == 0 || !self.is_valid_func_ptr(drop_ptr) {
            return None;
        }

        // size and align should be reasonable
        // alignment for Rust vtables is at least ptr_size for most practical types
        if size > 0x1000000
            || align == 0
            || (align & (align - 1)) != 0
            || align > 4096
            || align < (ptr_size as u64)
        {
            return None;
        }

        // It looks like a vtable. Now scan for methods.
        let mut methods = Vec::new();
        let mut method_addr = addr + (ptr_size * 3) as u64;

        while let Some(m_ptr_bytes) = self.binary.view_bytes(method_addr, ptr_size) {
            let m_ptr = self.read_ptr(m_ptr_bytes, 0, ptr_size);
            if self.is_valid_func_ptr(m_ptr) {
                methods.push(m_ptr);
                method_addr += ptr_size as u64;
            } else {
                break;
            }
        }

        // Rust vtables usually have at least one method or just drop/size/align
        let name = self.heuristic_name_vtable(drop_ptr);

        Some(RustVTableInfo {
            address: addr,
            name,
            size,
            align,
            methods,
        })
    }

    fn read_ptr(&self, data: &[u8], offset: usize, size: usize) -> u64 {
        if offset + size > data.len() {
            return 0;
        }
        if size == 8 {
            // Safe: bounds already checked above
            match data[offset..offset + 8].try_into() {
                Ok(bytes) => u64::from_le_bytes(bytes),
                Err(_) => 0, // Should never happen due to bounds check
            }
        } else {
            match data[offset..offset + 4].try_into() {
                Ok(bytes) => u32::from_le_bytes(bytes) as u64,
                Err(_) => 0,
            }
        }
    }

    fn is_valid_func_ptr(&self, ptr: u64) -> bool {
        if ptr == 0 {
            return true;
        } // NULL is allowed for some cases but usually not for drop_in_place

        // Find if ptr is in an executable section
        self.binary.sections.iter().any(|s| {
            s.is_executable && ptr >= s.virtual_address && ptr < s.virtual_address + s.virtual_size
        })
    }

    fn heuristic_name_vtable(&self, drop_ptr: u64) -> String {
        // Try to find a name from the drop symbol
        if let Some(idx) = self.binary.function_addr_index.get(&drop_ptr) {
            let func = &self.binary.functions[*idx];
            if func.name.contains("drop_in_place") {
                // Example: core::ptr::drop_in_place<std::string::String>
                // Extract the type inside <>
                if let (Some(start), Some(end)) = (func.name.find('<'), func.name.rfind('>')) {
                    let type_name = &func.name[start + 1..end];
                    return format!("vtable_for_{}", type_name.replace("::", "_"));
                }
            }
            return format!("vtable_{}", func.name);
        }
        format!("vtable_at_0x{:x}", drop_ptr)
    }
}
