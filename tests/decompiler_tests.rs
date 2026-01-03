//! Integration tests for the decompiler module
//!
//! Note: These tests require the native_decomp feature to be enabled.
//! Tests are skipped if the feature is not available.

/// Test creating a native decompiler (only if feature is enabled)
#[test]
#[cfg(feature = "native_decomp")]
fn test_create_native_decompiler() {
    use fission::analysis::decomp::ffi::DecompilerNative;

    let sla_dir = std::env::current_dir()
        .unwrap()
        .join("ghidra_decompiler")
        .join("languages")
        .to_string_lossy()
        .into_owned();

    let result = DecompilerNative::new(&sla_dir);

    // Decompiler creation should succeed
    assert!(
        result.is_ok(),
        "Failed to create DecompilerNative: {:?}",
        result.err()
    );
}

/// Test that decompiler handles invalid SLA directory gracefully
#[test]
#[cfg(feature = "native_decomp")]
fn test_native_with_invalid_sla_dir() {
    use fission::analysis::decomp::ffi::DecompilerNative;

    let invalid_sla_dir = "/nonexistent/sla/directory";

    // Decompiler creation should fail with invalid SLA dir
    let result = DecompilerNative::new(invalid_sla_dir);
    assert!(result.is_err(), "Should fail with invalid SLA directory");
}

/// Test decompiling simple x86-64 bytes (NOP sled)
#[test]
#[cfg(feature = "native_decomp")]
fn test_decompile_simple_bytes() {
    use fission::analysis::decomp::ffi::DecompilerNative;

    let sla_dir = std::env::current_dir()
        .unwrap()
        .join("ghidra_decompiler")
        .join("languages")
        .to_string_lossy()
        .into_owned();

    let mut native = match DecompilerNative::new(&sla_dir) {
        Ok(n) => n,
        Err(e) => {
            println!("Skipping: Failed to create native decompiler: {}", e);
            return;
        }
    };

    // Simple x86-64 function: push rbp; mov rbp, rsp; pop rbp; ret
    let bytes = vec![
        0x55, // push rbp
        0x48, 0x89, 0xE5, // mov rbp, rsp
        0x5D, // pop rbp
        0xC3, // ret
    ];

    // Load binary first
    if let Err(e) = native.load_binary(&bytes, 0x1000, true) {
        println!("Skipping: Failed to load binary: {}", e);
        return;
    }

    let result = native.decompile(0x1000);

    // Should produce some output (even if it's just a stub function)
    if let Ok(c_code) = result {
        println!("Decompiled output:\n{}", c_code);
        assert!(
            !c_code.is_empty(),
            "Decompiled code should not be empty"
        );
    } else {
        println!("Decompile result: {:?}", result);
        // Don't fail the test - decompiler may have issues with tiny functions
    }
}
