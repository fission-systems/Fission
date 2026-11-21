//! Every loader path from raw bytes: format detection, then whichever parser
//! claims the input (ELF, PE, Mach-O, COFF, TE, a.out, Intel HEX, MZ/NE, raw),
//! with its symbol, debug-info and identity analysis. No magic gate, so the
//! detector itself is fuzzed too.
//!
//! Run from `crates/fission-loader`: `cargo fuzz run fuzz_any_format`.

#![no_main]

use fission_loader::LoadedBinary;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = LoadedBinary::from_bytes(data.to_vec(), "fuzz_input".to_string());
});
