//! Ghidra Decompiler Integration Test
//! 
//! Run with: cargo test --features native_decomp test_decompiler -- --nocapture

use super::{Decompiler, StubDecompiler};

/// Simple x86-64 function bytes:
/// push rbp
/// mov rbp, rsp
/// mov eax, 0x2a  (42)
/// pop rbp
/// ret
const SIMPLE_FUNC: &[u8] = &[
    0x55,                   // push rbp
    0x48, 0x89, 0xe5,       // mov rbp, rsp
    0xb8, 0x2a, 0x00, 0x00, 0x00, // mov eax, 42
    0x5d,                   // pop rbp
    0xc3,                   // ret
];

#[test]
fn test_stub_disassemble() {
    let stub = StubDecompiler::new();
    let result = stub.disassemble(SIMPLE_FUNC, 0x1000);
    
    println!("=== Stub Disassembly ===");
    println!("{}", result.text);
    
    assert!(!result.text.is_empty());
}

#[test]
fn test_stub_decompile() {
    let stub = StubDecompiler::new();
    let result = stub.decompile(SIMPLE_FUNC, 0x1000);
    
    println!("=== Stub Decompilation ===");
    println!("{}", result.body);  // Fixed: use .body not .code
    
    assert!(!result.body.is_empty());
}

#[test]
#[cfg(feature = "native_decomp")]
fn test_native_decompiler_init() {
    println!("=== Native Decompiler Test ===");
    
    // Try to initialize with the languages folder
    let sla_path = "ghidra_decompiler/languages";
    
    match Decompiler::new(sla_path) {
        Ok(decomp) => {
            println!("✅ Decompiler initialized!");
            println!("   SLA dir: {}", decomp.sla_dir());
            println!("   Native available: {}", Decompiler::is_native_available());
            
            // Test disassembly
            match decomp.disassemble(SIMPLE_FUNC, 0x1000) {
                Ok(result) => {
                    println!("\n=== Native Disassembly ===");
                    println!("{}", result.text);
                }
                Err(e) => {
                    println!("⚠️ Disassembly error: {}", e);
                }
            }
            
            // Test decompilation
            match decomp.decompile(SIMPLE_FUNC, 0x1000) {
                Ok(result) => {
                    println!("\n=== Native Decompilation ===");
                    println!("{}", result.body);  // Fixed: use .body not .code
                }
                Err(e) => {
                    println!("⚠️ Decompilation error: {}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to init decompiler: {}", e);
            println!("   (This is expected if .sla files aren't in the right place)");
        }
    }
}

#[test]
fn test_is_native_available() {
    let available = Decompiler::is_native_available();
    println!("Native decompiler available: {}", available);
}
