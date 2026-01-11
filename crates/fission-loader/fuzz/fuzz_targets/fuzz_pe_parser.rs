//! Fuzz target for PE binary parser
//!
//! This tests the PE parser against arbitrary input to find crashes
//! or panics that could be triggered by malformed files.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only try parsing if we have at least the DOS header size
    if data.len() < 64 {
        return;
    }

    // Check for MZ signature before attempting parse
    if data.get(0..2) != Some(&[0x4D, 0x5A]) {
        return;
    }

    // Attempt to load the binary - should not panic
    let _ = fission_loader::loader::load_binary_from_bytes(data, "fuzz_input.exe");
});
