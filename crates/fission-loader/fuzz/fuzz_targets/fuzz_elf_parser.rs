//! Fuzz target for ELF binary parser
//!
//! This tests the ELF parser against arbitrary input to find crashes
//! or panics that could be triggered by malformed files.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only try parsing if we have at least the ELF header size
    if data.len() < 52 {
        return;
    }

    // Check for ELF magic before attempting parse
    if data.get(0..4) != Some(&[0x7F, 0x45, 0x4C, 0x46]) {
        return;
    }

    // Attempt to load the binary - should not panic
    let _ = fission_loader::loader::load_binary_from_bytes(data, "fuzz_input.elf");
});
