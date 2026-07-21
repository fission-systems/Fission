//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O/COFF binaries using Fission-owned loader contracts.

use crate::prelude::*;
use std::path::Path;

pub mod analyzers;
pub mod coff;
pub mod containers;
pub mod demangle;
pub mod dwarf;
pub mod elf;
pub mod formats;
pub mod function_view;
pub mod gcc_lsda;
pub mod identity;
pub mod macho;
pub mod pdb_registers;
pub mod pdb_sidecar;
pub mod pe;
pub mod pipeline;
pub mod reader;
pub mod strings;
pub mod types;
pub use analyzers::{cpp, golang, rust};
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
        let mut binary = pipeline::LoaderPipeline::load(data, path)?;
        binary.identity_report = Some(identity::analyze(
            &binary,
            identity::IdentityScanLimits::default(),
        ));
        let format = binary.format.clone();

        if format.starts_with("PE") && binary.inner().pdb_debug_info.is_some() {
            if let Err(err) = pdb_sidecar::ingest_pdb_function_facts(&mut binary) {
                tracing::debug!("[Loader] focused PDB function ingestion skipped: {err}");
            }
        }
        if format.starts_with("ELF") {
            binary.eh_lsda = elf::lsda::analyze_eh_lsda(&binary);
        }
        if format.starts_with("PE") {
            binary.eh_lsda = pe::seh::analyze_seh_lsda(&binary);
        }
        merge_debug_function_sizes(&mut binary);

        // Go Language Analysis
        let detection = crate::detector::detect(&binary);
        if let Some(lang) = detection.language() {
            tracing::info!(
                "[Loader] Detected language: {} (confidence: {:?})",
                lang.name,
                lang.confidence
            );
        }

        let format_ref = &format;
        let binary_ref = &binary;
        let detection_ref = &detection;

        // Run Go, Apple/ObjC/Swift, DWARF, Rust, and C++ RTTI analyzers concurrently using Rayon!
        let (
            (go_ver, go_functions_result, go_strings, go_types),
            (
                (apple_funcs_res, swift_types_res, objc_classes_res, objc_selectors_res),
                (
                    (dwarf_types_res, dwarf_funcs_res, dwarf_lines_res),
                    (rust_vtables_res, (cpp_types_res, cpp_vtable_funcs_res)),
                ),
            ),
        ) = rayon::join(
            || {
                if detection_ref.language().map_or(false, |d| d.name == "Go") {
                    let analyzer = golang::GoAnalyzer::new(binary_ref);
                    let go_ver = analyzer.detect_go_version();
                    let go_funcs = analyzer.analyze();
                    let go_strings = analyzer.scan_go_strings();
                    let go_types = analyzer.analyze_types();
                    (go_ver, Some(go_funcs), go_strings, go_types)
                } else {
                    (None, None, std::collections::HashMap::new(), Vec::new())
                }
            },
            || {
                rayon::join(
                    || {
                        if format_ref.starts_with("Mach-O") {
                            let analyzer = macho::apple::AppleAnalyzer::new(binary_ref);
                            let apple_funcs = analyzer.analyze();
                            let swift_types = analyzer.analyze_swift_types();
                            let objc_classes = analyzer.analyze_objc_ivars();
                            let objc_selectors = analyzer.resolve_msg_send_selectors();
                            (
                                Some(apple_funcs),
                                Some(swift_types),
                                objc_classes,
                                objc_selectors,
                            )
                        } else {
                            (None, None, Vec::new(), std::collections::HashMap::new())
                        }
                    },
                    || {
                        rayon::join(
                            || {
                                let dwarf_analyzer = dwarf::DwarfAnalyzer::new(binary_ref);
                                if dwarf_analyzer.has_debug_info() {
                                    let types = dwarf_analyzer.analyze_types();
                                    let funcs = dwarf_analyzer.analyze_functions();
                                    let lines = dwarf_analyzer.analyze_lines();
                                    (types, funcs, lines)
                                } else {
                                    (Vec::new(), Vec::new(), Vec::new())
                                }
                            },
                            || {
                                rayon::join(
                                    || {
                                        if detection_ref
                                            .language()
                                            .map_or(false, |d| d.name == "Rust")
                                        {
                                            let analyzer = rust::RustAnalyzer::new(binary_ref);
                                            analyzer.analyze_vtables()
                                        } else {
                                            Vec::new()
                                        }
                                    },
                                    || {
                                        let analyzer = cpp::CppAnalyzer::new(binary_ref);
                                        (
                                            analyzer.to_inferred_types(),
                                            analyzer.discover_vtable_functions(),
                                        )
                                    },
                                )
                            },
                        )
                    },
                )
            },
        );

        // ====================================================================
        // Thread-safe Merge Phase on Main Thread
        // ====================================================================

        // 1. Merge Go Results
        binary.go_version = go_ver;
        if let Some(Ok(go_functions)) = go_functions_result {
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

        if !go_strings.is_empty() {
            tracing::info!(
                "[Loader] GoString scanner found {} string structs",
                go_strings.len()
            );
            for (addr, content) in go_strings {
                let inner = binary.inner_mut();
                inner.global_symbols.entry(addr).or_insert(content.clone());
                inner.string_map.entry(addr).or_insert(content);
            }
        }

        for ty in go_types {
            binary
                .inner_mut()
                .inferred_types
                .push(ty.to_inferred_type());
        }

        // 2. Merge Apple/ObjC/Swift Results
        if let Some(Ok(apple_functions)) = apple_funcs_res {
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

        if let Some(Ok(swift_types)) = swift_types_res {
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

        for class_info in objc_classes_res {
            binary
                .inner_mut()
                .inferred_types
                .push(class_info.to_inferred_type());
        }

        if !objc_selectors_res.is_empty() {
            tracing::info!(
                "[Loader] ObjC selector resolution: {} selrefs resolved",
                objc_selectors_res.len()
            );
            for (addr, name) in objc_selectors_res {
                binary
                    .inner_mut()
                    .global_symbols
                    .entry(addr)
                    .or_insert(name);
            }
        }

        // 3. Merge Rust Results
        for vtable in rust_vtables_res {
            binary
                .inner_mut()
                .inferred_types
                .push(vtable.to_inferred_type());
        }

        // 4. Merge DWARF Results
        for ty in dwarf_types_res {
            binary
                .inner_mut()
                .inferred_types
                .push(ty.to_inferred_type());
        }

        if !dwarf_funcs_res.is_empty() {
            tracing::info!(
                "[Loader] DWARF: {} functions with debug info extracted",
                dwarf_funcs_res.len()
            );

            // Update function names from DWARF if better than symbol table names
            for func_info in &dwarf_funcs_res {
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
            for func_info in dwarf_funcs_res {
                binary.dwarf_functions.insert(func_info.address, func_info);
            }
        }

        // dwarf_lines_res is already sorted ascending by address (see
        // DwarfAnalyzer::analyze_lines) -- required by line_for_address's
        // binary search.
        binary.dwarf_lines = dwarf_lines_res;

        // 5. Merge C++ Results
        for ty in cpp_types_res {
            binary.inner_mut().inferred_types.push(ty);
        }

        // Merge C++ VTable Functions
        if !cpp_vtable_funcs_res.is_empty() {
            let mut addr_to_existing = std::collections::HashMap::new();
            for (idx, func) in binary.inner().functions.iter().enumerate() {
                addr_to_existing.insert(func.address, idx);
            }

            for vfunc in cpp_vtable_funcs_res {
                if let Some(&idx) = addr_to_existing.get(&vfunc.address) {
                    let existing = &mut binary.inner_mut().functions[idx];
                    if existing.name.starts_with("FUN_")
                        || existing.name.starts_with("sub_")
                        || existing.name.is_empty()
                    {
                        existing.name = vfunc.name;
                        existing.origin = vfunc.origin;
                        existing.kind = vfunc.kind;
                    }
                } else {
                    addr_to_existing.insert(vfunc.address, binary.inner().functions.len());
                    binary.inner_mut().functions.push(vfunc);
                }
            }
            binary
                .inner_mut()
                .functions
                .sort_unstable_by_key(|f| f.address);
            binary.inner_mut().functions_sorted = true;
            binary.rebuild_function_indices();
        }

        Ok(binary)
    }
}

fn merge_debug_function_sizes(binary: &mut LoadedBinary) {
    let dwarf = binary.dwarf_functions.clone();
    let pdb = binary.pdb_functions.clone();
    for func in &mut binary.inner_mut().functions {
        if func.is_import || func.size > 0 {
            continue;
        }
        if let Some(debug) = dwarf.get(&func.address) {
            if debug.size > 0 {
                func.size = debug.size;
                continue;
            }
        }
        if let Some(debug) = pdb.get(&func.address) {
            if debug.size > 0 {
                func.size = debug.size;
            }
        }
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
    fn test_profile_loader_on_fixture() {
        use std::time::Instant;
        let fixture_path =
            std::path::Path::new("/Users/sjkim1127/Fission/benchmark").join(format!(
                "binary/x86-64/window/small/binary/c/test_functions.ex{}",
                "e"
            ));
        if !fixture_path.exists() {
            println!("Fixture not found at {}", fixture_path.display());
            return;
        }

        println!("=== Loader profiling on fixture (RUN 1 - COLD) ===");
        let start1 = Instant::now();
        let _binary1 = LoadedBinary::from_file(fixture_path.clone()).unwrap();
        println!("LoadedBinary::from_file RUN 1 took: {:?}", start1.elapsed());

        println!("=== Loader profiling on fixture (RUN 2 - WARM CACHED) ===");
        let start2 = Instant::now();
        let _binary2 = LoadedBinary::from_file(fixture_path).unwrap();
        println!("LoadedBinary::from_file RUN 2 took: {:?}", start2.elapsed());
    }

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
    fn test_string_map_populated() {
        // Build a minimal binary with .rdata containing strings
        let mut data = vec![0x90u8; 256];
        let rdata_start = 128;
        data[rdata_start..rdata_start + 6].copy_from_slice(b"Hello\0");
        data[rdata_start + 6..rdata_start + 17].copy_from_slice(b"World Test\0");

        let binary = LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(data))
            .add_section(SectionInfo {
                name: ".rdata".to_string(),
                virtual_address: 0x2000,
                virtual_size: 128,
                file_offset: 128,
                file_size: 128,
                is_executable: false,
                is_readable: true,
                is_writable: false,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 128,
                file_offset: 0,
                file_size: 128,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

        assert!(!binary.string_map.is_empty());
        assert_eq!(binary.string_map.get(&0x2000), Some(&"Hello".to_string()));
        assert_eq!(
            binary.string_map.get(&0x2006),
            Some(&"World Test".to_string())
        );
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
                    ..Default::default()
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
        assert!(binary.string_map.is_empty());

        let Some(func) = binary.find_function("main") else {
            panic!("main function should exist")
        };
        assert_eq!(func.address, 0x1000);
    }

    #[test]
    fn test_execution_view_prefers_executable_section_for_overlapping_va() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(b"DATA");
        data[16..20].copy_from_slice(b"CODE");

        let binary = LoadedBinaryBuilder::new("reloc.o".to_string(), DataBuffer::Heap(data))
            .format("ELF64")
            .entry_point(0)
            .image_base(0)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".strtab".to_string(),
                virtual_address: 0,
                virtual_size: 4,
                file_offset: 0,
                file_size: 4,
                is_executable: false,
                is_readable: false,
                is_writable: false,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0,
                virtual_size: 4,
                file_offset: 16,
                file_size: 4,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

        assert_eq!(binary.view_executable_bytes(0, 4), Some(&b"CODE"[..]));
        assert_eq!(
            binary
                .section_containing_for_execution(0)
                .map(|section| section.name.as_str()),
            Some(".text")
        );
    }

    #[test]
    fn available_execution_bytes_clamps_to_containing_section_extent() {
        let binary =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(vec![0x90; 100]))
                .format("RAW")
                .entry_point(0x1000)
                .image_base(0x1000)
                .is_64bit(true)
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0x1000,
                    virtual_size: 100,
                    file_offset: 0,
                    file_size: 100,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .build()
                .expect("build");

        assert_eq!(binary.available_execution_bytes(0x1000), Some(100));
        assert_eq!(binary.available_execution_bytes(0x1060), Some(4));
        assert_eq!(binary.available_execution_bytes(0x1064), None);
    }

    #[test]
    fn test_builder_sorts_sections_for_binary_search_lookup() {
        let mut data = vec![0u8; 64];
        data[0..4].copy_from_slice(b"CODE");
        data[32..36].copy_from_slice(b"DATA");

        let binary = LoadedBinaryBuilder::new("unsorted.o".to_string(), DataBuffer::Heap(data))
            .format("ELF64")
            .entry_point(0)
            .image_base(0x100000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".data".to_string(),
                virtual_address: 0x100100,
                virtual_size: 4,
                file_offset: 32,
                file_size: 4,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            })
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x100000,
                virtual_size: 4,
                file_offset: 0,
                file_size: 4,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

        assert_eq!(binary.view_bytes(0x100000, 4), Some(&b"CODE"[..]));
        assert_eq!(binary.view_bytes(0x100100, 4), Some(&b"DATA"[..]));
    }

    #[test]
    fn test_elf_relocatable_executable_views_use_explicit_section_layout() {
        // Synthetic ELF64 relocatable-style layout (no benchmark/binary fixtures).
        let mut data = vec![0u8; 320];
        data[0] = 0x90;
        data[64] = 0x91;
        data[224] = 0x92;

        let binary =
            LoadedBinaryBuilder::new("synthetic_reloc.o".to_string(), DataBuffer::Heap(data))
                .format("ELF64")
                .entry_point(0)
                .image_base(0)
                .is_64bit(true)
                .add_section(SectionInfo {
                    name: ".text.a".to_string(),
                    virtual_address: 0x100000,
                    virtual_size: 1,
                    file_offset: 0,
                    file_size: 1,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_section(SectionInfo {
                    name: ".text.b".to_string(),
                    virtual_address: 0x100140,
                    virtual_size: 1,
                    file_offset: 64,
                    file_size: 1,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_section(SectionInfo {
                    name: ".text.c".to_string(),
                    virtual_address: 0x1001a0,
                    virtual_size: 1,
                    file_offset: 224,
                    file_size: 1,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .build()
                .expect("build synthetic ELF64 relocatable-like binary");

        for (address, expected) in [
            (0x100000_u64, 0x90_u8),
            (0x100140_u64, 0x91_u8),
            (0x1001a0_u64, 0x92_u8),
        ] {
            let exe = binary
                .view_executable_bytes(address, 1)
                .unwrap_or_else(|| panic!("executable bytes at 0x{address:x}"));
            assert_eq!(exe[0], expected);
            let raw = binary
                .view_bytes(address, 1)
                .unwrap_or_else(|| panic!("raw bytes at 0x{address:x}"));
            assert_eq!(raw[0], expected);
        }
    }

    #[test]
    fn test_builder_preserves_relocation_symbols() {
        let mut relocation_symbols = std::collections::HashMap::new();
        relocation_symbols.insert(0x10003a8, "control_sink".to_string());

        let binary = LoadedBinaryBuilder::new("reloc.o".to_string(), DataBuffer::Heap(vec![0; 64]))
            .format("ELF64")
            .entry_point(0x100000)
            .image_base(0x100000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x100000,
                virtual_size: 64,
                file_offset: 0,
                file_size: 64,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_relocation_symbols(relocation_symbols)
            .build()
            .expect("build binary with relocation symbols");

        assert_eq!(
            binary.inner().relocation_symbols.get(&0x10003a8),
            Some(&"control_sink".to_string())
        );
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
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "func_b".to_string(),
                    address: 0x1100,
                    size: 100,
                    is_export: false,
                    is_import: false,
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "func_c".to_string(),
                    address: 0x1200,
                    size: 0, // Unknown size
                    is_export: false,
                    is_import: true,
                    ..Default::default()
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
