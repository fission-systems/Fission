//! Fuzz target for ELF binary parser
//!
//! This tests the ELF parser against arbitrary input to find crashes
//! or panics that could be triggered by malformed files.
//!
//! Run from `crates/fission-loader`: `cargo fuzz run fuzz_elf_parser`.
//! Nightly fuzz job: `.github/workflows/fuzz.yml`.

#![no_main]

use fission_loader::LoadedBinary;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only try parsing if we have at least the ELF header size
    if data.len() < 52 {
        return;
    }

    // Check for ELF magic before attempting parse
    if data.get(0..4) != Some(&b"\x7fELF"[..]) {
        return;
    }

    // Attempt to load the binary - should not panic
    let _ = LoadedBinary::from_bytes(data.to_vec(), "fuzz_input.elf".to_string());
});
