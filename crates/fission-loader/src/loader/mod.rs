//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O executables using parsers.

use crate::prelude::*;
use fission_core::constants::binary_format::*;
use std::path::Path;

pub mod cpp;
pub mod demangle;
pub mod dwarf;
pub mod elf;
pub mod golang;
pub mod macho;
pub mod pe;
pub mod rust;
pub mod types;
pub use types::*;

impl LoadedBinary {
    /// Load and parse a binary file using memory mapping
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();

        let file = std::fs::File::open(path_ref)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let data = DataBuffer::Mapped(mmap);

        Self::auto_detect_and_parse(data, path_str)
    }

    /// Parse binary from bytes
    pub fn from_bytes(data: Vec<u8>, path: String) -> Result<Self> {
        // Auto-detect format and parse
        Self::auto_detect_and_parse(DataBuffer::Heap(data), path)
    }

    /// Parse binary from bytes (alias for compatibility)
    pub fn from_bytes_dynamic(data: Vec<u8>, path: String) -> Result<Self> {
        Self::auto_detect_and_parse(DataBuffer::Heap(data), path)
    }

    /// Auto-detect binary format and parse
    fn auto_detect_and_parse(data: DataBuffer, path: String) -> Result<Self> {
        let bytes = data.as_slice();
        // Try to detect format by magic bytes
        if bytes.len() < 4 {
            return Err(FissionError::loader("Binary too small"));
        }

        let format = if bytes.len() > 0x3C + 4 {
            let pe_offset =
                u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
            if pe_offset < bytes.len() - 4 && &bytes[pe_offset..pe_offset + 2] == b"PE" {
                "PE"
            } else if bytes.starts_with(b"\x7fELF") {
                "ELF"
            } else {
                let magic = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if matches!(
                    magic,
                    MACHO_MAGIC_32_BE | MACHO_MAGIC_64_BE | MACHO_MAGIC_32_LE | MACHO_MAGIC_64_LE
                ) {
                    "Mach-O"
                } else {
                    "Unknown"
                }
            }
        } else if bytes.starts_with(b"\x7fELF") {
            "ELF"
        } else {
            let magic = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if matches!(
                magic,
                MACHO_MAGIC_32_BE | MACHO_MAGIC_64_BE | MACHO_MAGIC_32_LE | MACHO_MAGIC_64_LE
            ) {
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
        if let Some(lang) = detection.language() {
            tracing::info!(
                "[Loader] Detected language: {} (confidence: {:?})",
                lang.name,
                lang.confidence
            );
        }

        if detection.language().map_or(false, |d| d.name == "Go") {
            let analyzer = golang::GoAnalyzer::new(&binary);
            if let Ok(go_functions) = analyzer.analyze() {
                // Merge Go functions into binary using a map for O(N) performance
                use std::collections::HashMap;
                let mut addr_to_existing = HashMap::new();
                for (idx, func) in binary.inner().functions.iter().enumerate() {
                    addr_to_existing.insert(func.address, idx);
                }

                for go_func in go_functions {
                    if let Some(&idx) = addr_to_existing.get(&go_func.address) {
                        let existing = &mut binary.inner_mut().functions[idx];
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

            // GAP-6: Go non-null-terminated string struct detection.
            // Equivalent to Ghidra's GolangStringAnalyzer which scans .rodata
            // for {ptr, len} GoString structs and creates labeled data entries.
            {
                let analyzer = golang::GoAnalyzer::new(&binary);
                let go_strings = analyzer.scan_go_strings();
                if !go_strings.is_empty() {
                    tracing::info!(
                        "[Loader] GoString scanner found {} string structs",
                        go_strings.len()
                    );
                    for (addr, content) in go_strings {
                        binary
                            .inner_mut()
                            .global_symbols
                            .entry(addr)
                            .or_insert(content);
                    }
                }
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

            // GAP-7: ObjC objc_msgSend selector resolution
            // Registers each __objc_selrefs slot as a named "sel_<method>" global
            // symbol so the decompiler shows resolved selector names instead of raw
            // pointer values (mirrors Ghidra's ObjectiveC2_MessageAnalyzer).
            {
                let analyzer = macho::apple::AppleAnalyzer::new(&binary);
                let selectors = analyzer.resolve_msg_send_selectors();
                if !selectors.is_empty() {
                    tracing::info!(
                        "[Loader] ObjC selector resolution: {} selrefs resolved",
                        selectors.len()
                    );
                    for (addr, name) in selectors {
                        binary
                            .inner_mut()
                            .global_symbols
                            .entry(addr)
                            .or_insert(name);
                    }
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

        // Rust VTable Analysis
        if detection.language().map_or(false, |d| d.name == "Rust") {
            let analyzer = rust::RustAnalyzer::new(&binary);
            let rust_vtables = analyzer.analyze_vtables();
            for vtable in rust_vtables {
                binary
                    .inner_mut()
                    .inferred_types
                    .push(vtable.to_inferred_type());
            }
        }

        // DWARF Debug Information Analysis (works for ELF and Mach-O with debug info)
        {
            // Phase 1: Extract all DWARF data (immutable borrow)
            let (dwarf_types, dwarf_funcs) = {
                let dwarf_analyzer = dwarf::DwarfAnalyzer::new(&binary);
                if dwarf_analyzer.has_debug_info() {
                    tracing::info!(
                        "[Loader] Found DWARF debug info, extracting types and functions..."
                    );
                    let types = dwarf_analyzer.analyze_types();
                    let funcs = dwarf_analyzer.analyze_functions();
                    (types, funcs)
                } else {
                    (Vec::new(), Vec::new())
                }
            };

            // Phase 2: Apply extracted data (mutable borrow)
            for ty in dwarf_types {
                binary
                    .inner_mut()
                    .inferred_types
                    .push(ty.to_inferred_type());
            }

            if !dwarf_funcs.is_empty() {
                tracing::info!(
                    "[Loader] DWARF: {} functions with debug info extracted",
                    dwarf_funcs.len()
                );

                // Update function names from DWARF if better than symbol table names
                for func_info in &dwarf_funcs {
                    if let Some(idx) = binary.function_addr_index.get(&func_info.address).copied() {
                        let current_name = &binary.functions[idx].name;
                        if current_name.is_empty()
                            || current_name.starts_with("FUN_")
                            || current_name.starts_with("sub_")
                        {
                            binary.inner_mut().functions[idx].name = func_info.name.clone();
                        }
                    }
                }

                // Store DWARF function info for post-processing (param/local name substitution)
                for func_info in dwarf_funcs {
                    binary.dwarf_functions.insert(func_info.address, func_info);
                }
            }
        }

        // C++ RTTI Analysis
        {
            let analyzer = cpp::CppAnalyzer::new(&binary);
            let cpp_types = analyzer.to_inferred_types();
            for ty in cpp_types {
                binary.inner_mut().inferred_types.push(ty);
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
        let Ok(exe_path) = std::env::current_exe() else {
            panic!("current_exe should be available in tests")
        };
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
        let builder =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(vec![0x90; 100]))
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

        let Ok(binary) = builder.build() else {
            panic!("Failed to build LoadedBinary")
        };

        assert_eq!(binary.path, "test.bin");
        assert_eq!(binary.data.as_slice().len(), 100);
        assert_eq!(binary.entry_point, 0x1000);
        assert_eq!(binary.format, "RAW");
        assert!(binary.is_64bit);
        assert_eq!(binary.functions.len(), 1);
        assert_eq!(binary.sections.len(), 1);
        assert!(binary.global_symbols.is_empty());

        let Some(func) = binary.find_function("main") else {
            panic!("main function should exist")
        };
        assert_eq!(func.address, 0x1000);
    }

    #[test]
    fn test_function_lookup_o1() {
        // Test that O(1) function lookups work correctly
        let builder =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(vec![0x90; 1000]))
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

        let Ok(binary) = builder.build() else {
            panic!("Failed to build LoadedBinary")
        };

        // Test find_function by name (O(1) lookup)
        assert!(binary.find_function("func_a").is_some());
        assert!(binary.find_function("func_b").is_some());
        assert!(binary.find_function("func_c").is_some());
        assert!(binary.find_function("nonexistent").is_none());

        // Test function_at_exact (O(1) lookup)
        assert!(binary.function_at_exact(0x1000).is_some());
        if let Some(func) = binary.function_at_exact(0x1000) {
            assert_eq!(func.name, "func_a");
        } else {
            panic!("function_at_exact(0x1000) should return func_a");
        }
        assert!(binary.function_at_exact(0x1100).is_some());
        if let Some(func) = binary.function_at_exact(0x1100) {
            assert_eq!(func.name, "func_b");
        } else {
            panic!("function_at_exact(0x1100) should return func_b");
        }
        assert!(binary.function_at_exact(0x1050).is_none()); // Not at start of function

        // Test function_at with range check (exact match is O(1), range check is O(N))
        assert!(binary.function_at(0x1000).is_some());
        if let Some(func) = binary.function_at(0x1000) {
            assert_eq!(func.name, "func_a");
        } else {
            panic!("function_at(0x1000) should return func_a");
        }
        assert!(binary.function_at(0x1020).is_some()); // Inside func_a (size=50)
        if let Some(func) = binary.function_at(0x1020) {
            assert_eq!(func.name, "func_a");
        } else {
            panic!("function_at(0x1020) should return func_a");
        }
        assert!(binary.function_at(0x1150).is_some()); // Inside func_b (size=100)
        if let Some(func) = binary.function_at(0x1150) {
            assert_eq!(func.name, "func_b");
        } else {
            panic!("function_at(0x1150) should return func_b");
        }
    }
}
