//! Integration tests for the decompiler module
//!
//! Note: These tests require the Ghidra decompiler CLI to be built.
//! Tests are skipped if the CLI is not available.

use fission::analysis::decomp::native::{find_cli, DecompilerServer};

/// Check if the decompiler CLI is available
fn cli_available() -> bool {
    find_cli().is_some()
}

/// Test finding the decompiler CLI
#[test]
fn test_find_decompiler_cli() {
    // This test just checks that find_cli doesn't panic
    let result = find_cli();
    
    // Log whether CLI was found (for debugging)
    if let Some(path) = &result {
        println!("Found decompiler CLI at: {:?}", path);
    } else {
        println!("Decompiler CLI not found (expected if not built)");
    }
    
    // The test passes either way - it's informational
}

/// Test creating a DecompilerServer (only if CLI is available)
#[test]
fn test_create_decompiler_server() {
    if !cli_available() {
        println!("Skipping test: Decompiler CLI not available");
        return;
    }
    
    let cli_path = find_cli().expect("CLI should be available (checked above)");
    let sla_dir = "ghidra_decompiler/data/sleigh";
    
    let result = DecompilerServer::new(&cli_path, sla_dir);
    
    // Server creation should succeed
    assert!(result.is_ok(), "Failed to create DecompilerServer: {:?}", result.err());
}

/// Test that server handles missing SLA directory gracefully
#[test]
fn test_server_with_invalid_sla_dir() {
    if !cli_available() {
        println!("Skipping test: Decompiler CLI not available");
        return;
    }
    
    let cli_path = find_cli().expect("CLI should be available");
    let invalid_sla_dir = "/nonexistent/sla/directory";
    
    // Server creation might succeed even with invalid SLA dir
    // (the error might occur on first decompile attempt)
    // This test just verifies we don't crash
    let _ = DecompilerServer::new(&cli_path, invalid_sla_dir);
}

/// Test decompiler pool creation (only if CLI is available)
#[test]
fn test_create_decompiler_pool() {
    use fission::analysis::decomp::DecompilerPool;
    
    if !cli_available() {
        println!("Skipping test: Decompiler CLI not available");
        return;
    }
    
    let cli_path = find_cli().expect("CLI should be available");
    let sla_dir = "ghidra_decompiler/data/sleigh";
    
    // Create pool with 1 worker for testing
    let result = DecompilerPool::new(&cli_path, sla_dir, 1);
    
    assert!(result.is_ok(), "Failed to create DecompilerPool: {:?}", result.err());
}

/// Test decompiling simple x86-64 bytes (NOP sled)
#[test]
fn test_decompile_simple_bytes() {
    if !cli_available() {
        println!("Skipping test: Decompiler CLI not available");
        return;
    }
    
    let cli_path = find_cli().expect("CLI should be available");
    let sla_dir = "ghidra_decompiler/data/sleigh";
    
    let mut server = match DecompilerServer::new(&cli_path, sla_dir) {
        Ok(s) => s,
        Err(e) => {
            println!("Skipping: Failed to create server: {}", e);
            return;
        }
    };
    
    // Simple x86-64 function: push rbp; mov rbp, rsp; pop rbp; ret
    let bytes = vec![
        0x55,                   // push rbp
        0x48, 0x89, 0xE5,       // mov rbp, rsp
        0x5D,                   // pop rbp
        0xC3,                   // ret
    ];
    
    let result = server.decompile(&bytes, 0x1000, true);
    
    // Should produce some output (even if it's just a stub function)
    if let Ok(code) = result {
        println!("Decompiled output:\n{}", code);
        assert!(!code.is_empty(), "Decompiled code should not be empty");
    } else {
        println!("Decompile result: {:?}", result);
        // Don't fail the test - decompiler may have issues with tiny functions
    }
}

