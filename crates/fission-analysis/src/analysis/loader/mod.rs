//! Binary Loader Module
//!
//! Parses PE/ELF/Mach-O executables using parsers.

use crate::prelude::*;
use crate::parser::BinaryParser;
use std::fs;
use std::path::Path;

pub mod elf;
pub mod macho;
pub mod pe;
pub mod types;
pub use types::*;

impl LoadedBinary {
    /// Load and parse a binary file using the static parser
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let data = fs::read(&path)?;
        Self::from_bytes(data, path_str)
    }

    /// Parse binary from bytes using the static parser
    pub fn from_bytes(data: Vec<u8>, path: String) -> Result<Self> {
        crate::parser::static_parser::StaticParser::new().parse(data, path)
    }

    /// Parse binary from bytes using the dynamic parser
    pub fn from_bytes_dynamic(data: Vec<u8>, path: String) -> Result<Self> {
        crate::parser::dynamic_parser::DynamicParser::new().parse(data, path)
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
