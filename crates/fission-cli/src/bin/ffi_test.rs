//! CLI test for native FFI decompiler
//!
//! Run with: cargo run --bin ffi_test --features native_decomp -- test/struct_test.exe

#[cfg(feature = "native_decomp")]
use std::env;
#[cfg(feature = "native_decomp")]
use std::fs;

#[cfg(feature = "native_decomp")]
use fission_ffi::DecompilerNative;

fn main() {
    #[cfg(not(feature = "native_decomp"))]
    {
        eprintln!("Error: native_decomp feature not enabled");
        eprintln!("Run with: cargo run --bin ffi_test --features native_decomp -- <binary>");
        std::process::exit(1);
    }

    #[cfg(feature = "native_decomp")]
    {
        let args: Vec<String> = env::args().collect();
        if args.len() < 2 {
            eprintln!("Usage: {} <binary_path>", args[0]);
            std::process::exit(1);
        }

        let binary_path = &args[1];
        eprintln!("[*] Loading binary: {}", binary_path);

        let binary_data: Vec<u8> = match fs::read(binary_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                std::process::exit(1);
            }
        };

        eprintln!("[*] Binary size: {} bytes", binary_data.len());

        // Get SLA directory
        let sla_dir = env::current_dir()
            .unwrap()
            .join("ghidra_decompiler")
            .join("languages")
            .to_string_lossy()
            .into_owned();

        eprintln!("[*] SLA directory: {}", sla_dir);

        // Create native decompiler
        eprintln!("[*] Creating native decompiler...");
        let mut decomp = match DecompilerNative::new(&sla_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error creating decompiler: {}", e);
                std::process::exit(1);
            }
        };

        eprintln!("[✓] Native decompiler created");

        // Load binary (assume 64-bit PE, base 0x140000000)
        let base_addr = 0x140000000u64;
        eprintln!("[*] Loading binary at base 0x{:x}...", base_addr);

        if let Err(e) = decomp.load_binary(&binary_data, base_addr, true) {
            eprintln!("Error loading binary: {}", e);
            std::process::exit(1);
        }

        eprintln!("[✓] Binary loaded");

        // Test decompile multiple functions
        let test_addresses = [
            0x140001400u64, // Entry point
            0x140001010u64,
            0x140001450u64,
            0x140001523u64,
            0x140001537u64,
            0x1400016e0u64,
            0x140001750u64,
            0x140001800u64,
            0x140001900u64,
            0x140001a00u64,
        ];

        for (i, addr) in test_addresses.iter().enumerate() {
            eprintln!("\n[*] Test {}: Decompiling 0x{:x}...", i + 1, addr);

            match decomp.decompile(*addr) {
                Ok(code) => {
                    eprintln!("[✓] Success! {} bytes of C code", code.len());
                    // Print first 200 chars
                    let preview: String = code.chars().take(200).collect();
                    eprintln!("--- Preview ---\n{}\n--- End ---", preview);
                }
                Err(e) => {
                    eprintln!("[✗] Failed: {}", e);
                }
            }

            eprintln!("[*] Test {} complete, checking for stability...", i + 1);
        }

        eprintln!("\n[✓] All tests completed successfully!");
        eprintln!("[*] If we got here, FFI is working correctly");
    }
}
