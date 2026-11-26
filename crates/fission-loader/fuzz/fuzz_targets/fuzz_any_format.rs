//! Every loader path from raw bytes: format detection, then whichever parser
//! claims the input (ELF, PE, Mach-O, COFF, TE, a.out, Intel HEX, S-record,
//! MZ/NE, raw), with its symbol, debug-info and identity analysis. No magic
//! gate, so the detector itself is fuzzed too.
//!
//! Formats that name no processor (Intel HEX, S-record) stop at
//! `LoadSpecNotFound` without a language, before their image is mapped, so
//! such an input is loaded a second time under a fixed one to reach the
//! mapping as well.
//!
//! Run from `crates/fission-loader`: `cargo fuzz run fuzz_any_format`.

#![no_main]

use fission_loader::LoadedBinary;
use fission_loader::loader::pipeline::ExplicitLanguage;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Err(err) = LoadedBinary::from_bytes(data.to_vec(), "fuzz_input".to_string()) else {
        return;
    };
    if !err.to_string().contains("LoadSpecNotFound") {
        return;
    }
    let language = ExplicitLanguage {
        language_id: "x86:LE:32:default".to_string(),
        architecture: fission_loader::loader::ArchitectureDescriptor::new(
            "x86",
            "little",
            32,
            "default",
            None,
            "fuzz",
        ),
        source: "fuzz".to_string(),
    };
    let _ = LoadedBinary::from_bytes_with_language(
        data.to_vec(),
        "fuzz_input".to_string(),
        &language,
    );
});
