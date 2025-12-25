//! Integration tests for the binary loader module
//!
//! Tests parsing of various binary formats (PE, ELF, Mach-O).

use std::path::PathBuf;

/// Test loading the test binary itself (fission executable)
#[test]
fn test_load_self_executable() {
    // Get the path to the test binary (the compiled test executable)
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");
    
    // Try to load it using LoadedBinary
    let result = fission::analysis::loader::LoadedBinary::from_file(&exe_path);
    
    assert!(result.is_ok(), "Failed to load self: {:?}", result.err());
    
    let binary = result.unwrap();
    
    // Verify basic properties
    assert!(!binary.data.is_empty(), "Binary data should not be empty");
    
    // On macOS, should be Mach-O
    #[cfg(target_os = "macos")]
    assert!(binary.format.starts_with("Mach-O"), "Expected Mach-O format on macOS, got: {}", binary.format);
    
    // On Linux, should be ELF
    #[cfg(target_os = "linux")]
    assert!(binary.format.starts_with("ELF"), "Expected ELF format on Linux, got: {}", binary.format);
    
    // On Windows, should be PE
    #[cfg(target_os = "windows")]
    assert!(binary.format.starts_with("PE"), "Expected PE format on Windows, got: {}", binary.format);
}

/// Test that non-existent file returns an error
#[test]
fn test_load_nonexistent_file() {
    let fake_path = PathBuf::from("/this/path/definitely/does/not/exist.exe");
    let result = fission::analysis::loader::LoadedBinary::from_file(&fake_path);
    
    assert!(result.is_err(), "Should fail to load non-existent file");
}

/// Test LoadedBinaryBuilder functionality
#[test]
fn test_loaded_binary_builder() {
    use fission::analysis::loader::{LoadedBinaryBuilder, FunctionInfo};
    
    let binary = LoadedBinaryBuilder::new("test.exe".to_string(), vec![0x4D, 0x5A, 0x90, 0x00])
        .format("PE32+")
        .arch_spec("x86_64")
        .entry_point(0x140001000)
        .image_base(0x140000000)
        .is_64bit(true)
        .add_function(FunctionInfo {
            name: "main".to_string(),
            address: 0x140001000,
            size: 100,
            is_export: true,
            is_import: false,
        })
        .build()
        .expect("Build should succeed");
    
    assert_eq!(binary.format, "PE32+");
    assert_eq!(binary.arch_spec, "x86_64");
    assert_eq!(binary.entry_point, 0x140001000);
    assert_eq!(binary.image_base, 0x140000000);
    assert_eq!(binary.data.len(), 4);
    assert_eq!(binary.functions.len(), 1);
    assert_eq!(binary.functions[0].name, "main");
    assert!(binary.is_64bit);
}

/// Test section parsing from loaded binary
#[test]
fn test_section_parsing() {
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let result = fission::analysis::loader::LoadedBinary::from_file(&exe_path);
    
    if let Ok(binary) = result {
        // Should have at least some sections
        assert!(!binary.sections.is_empty(), "Binary should have sections");
        
        // Check that code sections exist (like .text or __TEXT)
        let has_code_section = binary.sections.iter().any(|s| {
            s.name.contains("text") || s.name.contains("TEXT") || s.is_executable
        });
        assert!(has_code_section, "Binary should have a code section");
    }
}

/// Test architecture detection
#[test]
fn test_architecture_detection() {
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let result = fission::analysis::loader::LoadedBinary::from_file(&exe_path);
    
    if let Ok(binary) = result {
        // Current system is 64-bit
        #[cfg(target_pointer_width = "64")]
        assert!(binary.is_64bit, "Expected 64-bit binary on 64-bit system");
        
        // Architecture string should not be empty
        assert!(!binary.arch_spec.is_empty(), "Architecture spec should not be empty");
    }
}
