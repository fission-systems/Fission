//! COFF object files: section table, relocations, symbol table and the string
//! table its long names point into. COFF has no magic, so the input must
//! start with a machine type the loader accepts.
//!
//! Run from `crates/fission-loader`: `cargo fuzz run fuzz_coff_object`.

#![no_main]

use fission_loader::LoadedBinary;
use libfuzzer_sys::fuzz_target;

/// The machines `CoffLoader::looks_like_coff_object` accepts:
/// IMAGE_FILE_MACHINE_{I386, AMD64, ARM, ARM64}, little-endian.
const MACHINES: [u16; 4] = [0x014c, 0x8664, 0x01c0, 0xaa64];

fuzz_target!(|data: &[u8]| {
    let Some(machine) = data.get(0..2) else {
        return;
    };
    if !MACHINES.contains(&u16::from_le_bytes([machine[0], machine[1]])) {
        return;
    }
    let _ = LoadedBinary::from_bytes(data.to_vec(), "fuzz_input.obj".to_string());
});
