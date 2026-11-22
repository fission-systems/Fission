//! Mach-O, thin and universal: load commands, segments, symbols, ObjC
//! metadata. Inputs without a Mach-O or fat magic are skipped so the time
//! goes to the Mach-O paths.
//!
//! Run from `crates/fission-loader`: `cargo fuzz run fuzz_macho_parser`.

#![no_main]

use fission_loader::LoadedBinary;
use libfuzzer_sys::fuzz_target;

const MAGICS: [[u8; 4]; 6] = [
    [0xfe, 0xed, 0xfa, 0xce], // MH_MAGIC, big-endian bytes
    [0xce, 0xfa, 0xed, 0xfe], // MH_CIGAM
    [0xfe, 0xed, 0xfa, 0xcf], // MH_MAGIC_64
    [0xcf, 0xfa, 0xed, 0xfe], // MH_CIGAM_64
    [0xca, 0xfe, 0xba, 0xbe], // FAT_MAGIC
    [0xbe, 0xba, 0xfe, 0xca], // FAT_CIGAM
];

fuzz_target!(|data: &[u8]| {
    let Some(magic) = data.get(0..4) else { return };
    if !MAGICS.iter().any(|m| m == magic) {
        return;
    }
    let _ = LoadedBinary::from_bytes(data.to_vec(), "fuzz_input.macho".to_string());
});
