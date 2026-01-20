//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O executables using parsers.

use crate::prelude::*;
use std::fs;
use std::path::Path;

pub mod demangle;
pub mod dwarf;
pub mod elf;
pub mod golang;
pub mod macho;
pub mod pe;
pub mod types;
pub use types::*;

impl LoadedBinary {
    /// Load and parse a binary file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let data = fs::read(&path)?;
        Self::from_bytes(data, path_str)
    }

    /// Parse binary from bytes
    pub fn from_bytes(data: Vec<u8>, path: String) -> Result<Self> {
        // Auto-detect format and parse
        Self::auto_detect_and_parse(data, path)
    }

    /// Parse binary from bytes (alias for compatibility)
    /// Parse binary from bytes (alias for compatibility)
    pub fn from_bytes_dynamic(data: Vec<u8>, path: String) -> Result<Self> {
        Self::auto_detect_and_parse(data, path)
    }

    /// Auto-detect binary format and parse
    fn auto_detect_and_parse(data: Vec<u8>, path: String) -> Result<Self> {
        // Try to detect format by magic bytes
        if data.len() < 4 {
            return Err(FissionError::loader("Binary too small"));
        }

        let format = if data.len() > 0x3C + 4 {
            let pe_offset =
                u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
            if pe_offset < data.len() - 4 && &data[pe_offset..pe_offset + 2] == b"PE" {
                "PE"
            } else if data.starts_with(b"\x7fELF") {
                "ELF"
            } else {
                let magic = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
                if matches!(magic, 0xfeedface | 0xfeedfacf | 0xcefaedfe | 0xcffaedfe) {
                    "Mach-O"
                } else {
                    "Unknown"
                }
            }
        } else if data.starts_with(b"\x7fELF") {
            "ELF"
        } else {
            let magic = u32::from_ne_bytes([data[0], data[1], data[2], data[3]]);
            if matches!(magic, 0xfeedface | 0xfeedfacf | 0xcefaedfe | 0xcffaedfe) {
                "Mach-O"
            } else {
                "Unknown"
            }
        };

        let mut binary = match format {
            "PE" => pe::PeLoader::parse(data, path)?,
            "ELF" => elf::ElfLoader::parse(data, path)?,
            "Mach-O" => macho::MachoLoader::parse(data, path)?,
            _ => return Err(FissionError::loader("Unknown binary format")),
        };

        // Go Language Analysis
        let detection = crate::detector::detect(&binary);
        if detection.language().map_or(false, |d| d.name == "Go") {
            let analyzer = golang::GoAnalyzer::new(&binary);
            if let Ok(go_functions) = analyzer.analyze() {
                // Merge Go functions into binary
                for go_func in go_functions {
                    if let Some(existing) = binary
                        .inner_mut()
                        .functions
                        .iter_mut()
                        .find(|f| f.address == go_func.address)
                    {
                        if existing.name.starts_with("FUN_")
                            || existing.name.starts_with("sub_")
                            || existing.name.is_empty()
                        {
                            existing.name = go_func.name;
                        }
                    } else {
                        binary.inner_mut().functions.push(go_func);
                    }
                }
                binary.rebuild_indices();
            }
        }

        // Apple (ObjC/Swift) Analysis
        if format == "Mach-O" {
            // ObjC function analysis
            {
                let analyzer = macho::apple::AppleAnalyzer::new(&binary);
                if let Ok(apple_functions) = analyzer.analyze() {
                    for apple_func in apple_functions {
                        if let Some(existing) = binary
                            .inner_mut()
                            .functions
                            .iter_mut()
                            .find(|f| f.address == apple_func.address)
                        {
                            if existing.name.starts_with("sub_") || existing.name.is_empty() {
                                existing.name = apple_func.name;
                            }
                        } else {
                            binary.inner_mut().functions.push(apple_func);
                        }
                    }
                    binary.rebuild_indices();
                }
            }

            // Swift type metadata analysis (separate scope to avoid borrow conflict)
            {
                let analyzer = macho::apple::AppleAnalyzer::new(&binary);
                if let Ok(swift_types) = analyzer.analyze_swift_types() {
                    for ty in swift_types {
                        let inferred = types::InferredTypeInfo {
                            name: ty.name,
                            mangled_name: ty.mangled_name,
                            kind: format!("{:?}", ty.kind),
                            fields: ty
                                .fields
                                .into_iter()
                                .map(|f| types::InferredFieldInfo {
                                    name: f.name,
                                    type_name: f.type_name,
                                    offset: f.offset,
                                    size: 0,
                                })
                                .collect(),
                            size: ty.size,
                            metadata_address: 0,
                        };
                        binary.inner_mut().inferred_types.push(inferred);
                    }
                }
            }

            // Objective-C ivar analysis (separate scope)
            {
                let analyzer = macho::apple::AppleAnalyzer::new(&binary);
                let objc_classes = analyzer.analyze_objc_ivars();
                for class_info in objc_classes {
                    binary
                        .inner_mut()
                        .inferred_types
                        .push(class_info.to_inferred_type());
                }
            }
        }

        // Go Type Analysis (works for any format with Go reflection data)
        if detection.language().map_or(false, |d| d.name == "Go") {
            let analyzer = golang::GoAnalyzer::new(&binary);
            let go_types = analyzer.analyze_types();
            for ty in go_types {
                binary
                    .inner_mut()
                    .inferred_types
                    .push(ty.to_inferred_type());
            }
        }

        // DWARF Debug Information Analysis (works for ELF and Mach-O with debug info)
        {
            let dwarf_analyzer = dwarf::DwarfAnalyzer::new(&binary);
            if dwarf_analyzer.has_debug_info() {
                tracing::info!("[Loader] Found DWARF debug info, extracting types...");
                let dwarf_types = dwarf_analyzer.analyze_types();
                for ty in dwarf_types {
                    binary
                        .inner_mut()
                        .inferred_types
                        .push(ty.to_inferred_type());
                }
            }
        }

        Ok(binary)
    }
}

impl LoadedBinary {
    /// Rebuild internal indices after modifying functions
    pub fn rebuild_indices(&mut self) {
        let inner = self.inner_mut();
        inner.functions.sort_by_key(|f| f.address);

        let mut addr_index = std::collections::HashMap::new();
        let mut name_index = std::collections::HashMap::new();

        for (idx, func) in inner.functions.iter().enumerate() {
            addr_index.insert(func.address, idx);
            if !func.name.is_empty() {
                name_index.insert(func.name.clone(), idx);
            }
        }

        inner.function_addr_index = addr_index;
        inner.function_name_index = name_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_self() {
        // Parse the test executable itself
        let exe_path = std::env::current_exe().unwrap();
        let result = LoadedBinary::from_file(&exe_path);

        if let Ok(binary) = result {
            println!("{}", binary.summary());
            println!("\nFirst 10 functions:");
            for func in binary.functions_sorted().iter().take(10) {
                println!(
                    "  0x{:08x}: {} (size: {})",
                    func.address, func.name, func.size
                );
            }
            if !binary.format.contains("Mach-O") {
                assert!(binary.entry_point != 0);
            }
            assert!(!binary.sections.is_empty());
        } else {
            println!("Could not parse self: {:?}", result);
        }
    }

    #[test]
    fn test_loaded_binary_builder() {
        let builder = LoadedBinaryBuilder::new("test.bin".to_string(), vec![0x90; 100])
            .format("RAW")
            .entry_point(0x1000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "main".to_string(),
                address: 0x1000,
                size: 20,
                is_export: true,
                is_import: false,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 100,
                file_offset: 0,
                file_size: 100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            });

        let binary = builder.build().expect("Failed to build LoadedBinary");

        assert_eq!(binary.path, "test.bin");
        assert_eq!(binary.data.len(), 100);
        assert_eq!(binary.entry_point, 0x1000);
        assert_eq!(binary.format, "RAW");
        assert!(binary.is_64bit);
        assert_eq!(binary.functions.len(), 1);
        assert_eq!(binary.sections.len(), 1);
        assert!(binary.global_symbols.is_empty());

        let func = binary.find_function("main").unwrap();
        assert_eq!(func.address, 0x1000);
    }

    #[test]
    fn test_function_lookup_o1() {
        // Test that O(1) function lookups work correctly
        let builder = LoadedBinaryBuilder::new("test.bin".to_string(), vec![0x90; 1000])
            .format("RAW")
            .entry_point(0x1000)
            .image_base(0x1000)
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "func_a".to_string(),
                address: 0x1000,
                size: 50,
                is_export: true,
                is_import: false,
            })
            .add_function(FunctionInfo {
                name: "func_b".to_string(),
                address: 0x1100,
                size: 100,
                is_export: false,
                is_import: false,
            })
            .add_function(FunctionInfo {
                name: "func_c".to_string(),
                address: 0x1200,
                size: 0, // Unknown size
                is_export: false,
                is_import: true,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 1000,
                file_offset: 0,
                file_size: 1000,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            });

        let binary = builder.build().expect("Failed to build LoadedBinary");

        // Test find_function by name (O(1) lookup)
        assert!(binary.find_function("func_a").is_some());
        assert!(binary.find_function("func_b").is_some());
        assert!(binary.find_function("func_c").is_some());
        assert!(binary.find_function("nonexistent").is_none());

        // Test function_at_exact (O(1) lookup)
        assert!(binary.function_at_exact(0x1000).is_some());
        assert_eq!(binary.function_at_exact(0x1000).unwrap().name, "func_a");
        assert!(binary.function_at_exact(0x1100).is_some());
        assert_eq!(binary.function_at_exact(0x1100).unwrap().name, "func_b");
        assert!(binary.function_at_exact(0x1050).is_none()); // Not at start of function

        // Test function_at with range check (exact match is O(1), range check is O(N))
        assert!(binary.function_at(0x1000).is_some());
        assert_eq!(binary.function_at(0x1000).unwrap().name, "func_a");
        assert!(binary.function_at(0x1020).is_some()); // Inside func_a (size=50)
        assert_eq!(binary.function_at(0x1020).unwrap().name, "func_a");
        assert!(binary.function_at(0x1150).is_some()); // Inside func_b (size=100)
        assert_eq!(binary.function_at(0x1150).unwrap().name, "func_b");
    }
}
