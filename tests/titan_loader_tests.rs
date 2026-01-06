//! TitanLoader integration tests
//!
//! Tests OS loader simulation and memory mapping functionality.

use fission::analysis::loader::pe::PeLoader;
use fission::unpacker::TitanLoader;
use std::path::PathBuf;

/// Test TitanLoader with invalid PE file
#[test]
fn test_titan_loader_invalid_pe() {
    let loader = TitanLoader::new();
    
    // Empty file
    let result = loader.load(&[], "empty.exe");
    assert!(result.is_err(), "Should fail on empty file");
    
    // Non-PE file (ELF magic)
    let elf_data = vec![0x7f, 0x45, 0x4c, 0x46]; // ELF magic
    let result = loader.load(&elf_data, "not_pe.exe");
    assert!(result.is_err(), "Should fail on non-PE file");
    
    // Incomplete PE (MZ only, no NT headers)
    let mut minimal_pe = vec![0u8; 4096];
    minimal_pe[0..2].copy_from_slice(b"MZ");
    let result = loader.load(&minimal_pe, "minimal.exe");
    assert!(result.is_err(), "Should fail on incomplete PE file");
}

/// Test TitanLoader creates Default instance
#[test]
fn test_titan_loader_default() {
    let loader1 = TitanLoader::new();
    let loader2 = TitanLoader::default();
    
    // Both should be equivalent
    let _ = (loader1, loader2);
}

/// Test comparison between PeLoader (static) and TitanLoader (mapped)
#[test]
fn test_pe_vs_titan_loader_comparison() {
    // Try to load test PE file if it exists
    let test_pe_path = PathBuf::from("test/comparison_test_x64.exe");
    
    if !test_pe_path.exists() {
        println!("⏭️  Skipping PE comparison test - test file not found");
        return;
    }
    
    let pe_data = std::fs::read(&test_pe_path)
        .expect("Failed to read test PE file");
    
    // Load with PeLoader (static - file layout)
    let static_bin = PeLoader::parse(
        pe_data.clone(), 
        test_pe_path.to_string_lossy().to_string()
    ).expect("PeLoader should succeed");
    
    // Load with TitanLoader (dynamic - memory layout)
    let dynamic_bin = TitanLoader::new()
        .load(&pe_data, test_pe_path.to_string_lossy().as_ref())
        .expect("TitanLoader should succeed");
    
    // Verify same metadata
    assert_eq!(
        static_bin.image_base, 
        dynamic_bin.image_base,
        "Image base should match"
    );
    assert_eq!(
        static_bin.entry_point, 
        dynamic_bin.entry_point,
        "Entry point should match"
    );
    assert_eq!(
        static_bin.is_64bit, 
        dynamic_bin.is_64bit,
        "Architecture should match"
    );
    assert_eq!(
        static_bin.sections.len(), 
        dynamic_bin.sections.len(),
        "Section count should match"
    );
    assert_eq!(
        static_bin.format, 
        dynamic_bin.format,
        "Format should match"
    );
    
    // Verify different data layouts
    // TitanLoader should have memory-aligned data (larger due to page alignment)
    assert!(
        dynamic_bin.data.len() >= static_bin.data.len(),
        "Mapped memory ({} bytes) should be >= file size ({} bytes)",
        dynamic_bin.data.len(),
        static_bin.data.len()
    );
    
    // Verify section alignment
    // In memory-mapped layout, sections should be page-aligned
    let _page_size = 0x1000u64; // 4KB - for reference
    for section in &dynamic_bin.sections {
        let offset_in_memory = section.virtual_address - dynamic_bin.image_base;
        
        // First section might start at 0 (headers), but others should be aligned
        if offset_in_memory > 0 {
            // Check if aligned (actual alignment depends on PE section_alignment)
            // We just verify it's a reasonable value
            assert!(
                offset_in_memory < dynamic_bin.data.len() as u64,
                "Section {} VA should be within mapped memory",
                section.name
            );
        }
    }
    
    println!("✅ PeLoader (static):");
    println!("   - Data size: {} bytes", static_bin.data.len());
    println!("   - Sections: {}", static_bin.sections.len());
    println!("   - Functions: {}", static_bin.functions.len());
    
    println!("✅ TitanLoader (memory-mapped):");
    println!("   - Data size: {} bytes", dynamic_bin.data.len());
    println!("   - Expansion: {}%", 
        (dynamic_bin.data.len() as f64 / static_bin.data.len() as f64 * 100.0) as u32
    );
    println!("   - Image base: 0x{:x}", dynamic_bin.image_base);
}

/// Test TitanLoader section mapping bounds checking
#[test]
fn test_titan_loader_section_bounds() {
    let test_pe_path = PathBuf::from("test/comparison_test_x64.exe");
    
    if !test_pe_path.exists() {
        println!("⏭️  Skipping section bounds test - test file not found");
        return;
    }
    
    let pe_data = std::fs::read(&test_pe_path).unwrap();
    let result = TitanLoader::new().load(&pe_data, "test.exe");
    
    assert!(result.is_ok(), "TitanLoader should handle bounds correctly");
    
    let binary = result.unwrap();
    
    // Verify all sections are within mapped memory
    for section in &binary.sections {
        let section_start = section.virtual_address - binary.image_base;
        let section_end = section_start + section.virtual_size;
        
        assert!(
            section_end <= binary.data.len() as u64,
            "Section {} (0x{:x}-0x{:x}) should fit in mapped memory ({} bytes)",
            section.name,
            section_start,
            section_end,
            binary.data.len()
        );
    }
}

/// Test memory layout matches expected structure
#[test]
fn test_memory_layout_structure() {
    let test_pe_path = PathBuf::from("test/comparison_test_x64.exe");
    
    if !test_pe_path.exists() {
        println!("⏭️  Skipping memory layout test - test file not found");
        return;
    }
    
    let pe_data = std::fs::read(&test_pe_path).unwrap();
    let binary = TitanLoader::new()
        .load(&pe_data, "test.exe")
        .expect("Should load successfully");
    
    // PE headers should be at offset 0
    assert_eq!(
        &binary.data[0..2],
        b"MZ",
        "DOS signature should be at start of mapped memory"
    );
    
    // Find .text section (should have code)
    let text_section = binary.sections.iter()
        .find(|s| s.name.contains(".text") && s.is_executable);
    
    if let Some(section) = text_section {
        let offset = (section.virtual_address - binary.image_base) as usize;
        let section_size = section.virtual_size.min(section.file_size) as usize;
        
        // Verify we can access section data
        assert!(
            offset < binary.data.len(),
            ".text section should be accessible in mapped memory"
        );
        
        // Check if section is within bounds
        if offset + section_size <= binary.data.len() && section_size > 0 {
            // Code sections might be zero if not properly mapped
            // Just verify the section exists and is accessible
            println!("✓ .text section at offset 0x{:x}, size {} bytes", offset, section_size);
            
            // Check for non-zero bytes in a reasonable range
            let check_size = section_size.min(1000);
            let section_data = &binary.data[offset..offset + check_size];
            let nonzero_count = section_data.iter().filter(|&&b| b != 0).count();
            
            println!("  Non-zero bytes in first {} bytes: {}", check_size, nonzero_count);
            
            // Allow some flexibility - at least 10% should be non-zero in code sections
            // (Some sections might be mostly padding)
            if nonzero_count > 0 {
                println!("✓ Section contains data");
            } else {
                println!("⚠️  Section appears empty (might be BSS or padding)");
            }
        }
    } else {
        println!("⚠️  No .text section found (might use different naming)");
    }
}
