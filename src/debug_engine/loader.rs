use crate::core::prelude::*;
use crate::analysis::loader::types::{LoadedBinary, SectionInfo, FunctionInfo};
use super::types::*;
use goblin::Object;

/// Rust implementation of TitanEngine's file loading capabilities.
/// Focuses on mapping files as if they were loaded by the OS loader.
pub struct TitanLoader;

impl TitanLoader {
    pub fn new() -> Self {
        Self
    }

    /// Simulates the OS loader to map the binary into memory.
    /// This is crucial for "Dynamic Parsing" where we want to see the file
    /// exactly as it would appear in memory (sections aligned, imports resolved, etc.)
    pub fn load(&self, data: &[u8], path: &str) -> Result<LoadedBinary> {
        crate::core::logging::info(&format!("[TitanLoader] Loading {} via TitanEngine-rs", path));

        match Object::parse(data).map_err(|e| FissionError::loader(format!("Goblin parse error: {}", e)))? {
            Object::PE(pe) => {
                let is_64bit = pe.is_64;
                let image_base = pe.image_base as u64;
                let entry_point = pe.entry as u64 + image_base;
                
                let optional_header = pe.header.optional_header.ok_or(FissionError::loader("No optional header in PE"))?;
                let size_of_image = optional_header.windows_fields.size_of_image as usize;
                
                // 1. Allocate memory for the mapped image
                let mut mapped_data = vec![0u8; size_of_image];
                
                // 2. Copy Headers
                let size_of_headers = optional_header.windows_fields.size_of_headers as usize;
                if size_of_headers <= data.len() {
                    mapped_data[0..size_of_headers].copy_from_slice(&data[0..size_of_headers]);
                }

                // 3. Map Sections
                let mut sections = Vec::new();
                for section in &pe.sections {
                    let name = section.name().unwrap_or("").to_string();
                    let va = section.virtual_address as usize;
                    let vsize = section.virtual_size as usize;
                    let raw_ptr = section.pointer_to_raw_data as usize;
                    let raw_size = section.size_of_raw_data as usize;
                    
                    // Copy raw data to virtual address
                    // Use raw_size for copying from file, but vsize for the section info
                    let copy_size = std::cmp::min(raw_size, vsize);
                    
                    if raw_ptr + copy_size <= data.len() && va + copy_size <= mapped_data.len() {
                        mapped_data[va..va + copy_size].copy_from_slice(&data[raw_ptr..raw_ptr + copy_size]);
                    }
                    
                    sections.push(SectionInfo {
                        name,
                        virtual_address: image_base + va as u64,
                        virtual_size: vsize as u64,
                        // For memory mapped binary, file_offset is the RVA
                        file_offset: va as u64, 
                        file_size: raw_size as u64,
                        is_executable: section.characteristics & goblin::pe::section_table::IMAGE_SCN_MEM_EXECUTE != 0,
                        is_readable: section.characteristics & goblin::pe::section_table::IMAGE_SCN_MEM_READ != 0,
                        is_writable: section.characteristics & goblin::pe::section_table::IMAGE_SCN_MEM_WRITE != 0,
                    });
                }

                // 4. Extract Exports/Imports as Functions
                let mut functions = Vec::new();
                
                for export in &pe.exports {
                    if let Some(name) = &export.name {
                        functions.push(FunctionInfo {
                            name: name.to_string(),
                            address: image_base + export.rva as u64,
                            size: 0, // Unknown size
                            is_export: true,
                            is_import: false,
                        });
                    }
                }
                
                for import in &pe.imports {
                    functions.push(FunctionInfo {
                        name: import.name.to_string(),
                        address: image_base + import.rva as u64, 
                        size: 0,
                        is_export: false,
                        is_import: true,
                    });
                }

                let mut binary = LoadedBinary {
                    path: path.to_string(),
                    data: mapped_data,
                    image_base,
                    entry_point,
                    arch_spec: if is_64bit { "x86:LE:64:default".to_string() } else { "x86:LE:32:default".to_string() },
                    sections,
                    functions,
                    is_64bit,
                    is_dotnet: optional_header.data_directories.get_clr_runtime_header().is_some(),
                    dotnet_runtime_version: None,
                    format: "PE".to_string(),
                    iat_symbols: std::collections::HashMap::new(),
                    function_addr_index: std::collections::HashMap::new(),
                    function_name_index: std::collections::HashMap::new(),
                };
                
                binary.rebuild_function_indices();
                Ok(binary)
            },
            _ => Err(FissionError::loader("Only PE files are supported by TitanLoader currently")),
        }
    }
}
