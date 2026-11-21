//! Decode and lift arbitrary bytes with the runtime SLEIGH frontends.
//!
//! The decoder sees untrusted bytes on every binary Fission opens; a panic here
//! is a crash in `decomp`, `disasm` and `raw-pcode`. Byte 0 picks the language,
//! bytes 1..9 the address (alignment and context can depend on it), and the
//! rest is the instruction stream.
//!
//! Run from `crates/fission-sleigh`: `cargo fuzz run fuzz_sleigh_decode`.

#![no_main]

use std::sync::OnceLock;

use fission_sleigh::runtime::RuntimeSleighFrontend;
use libfuzzer_sys::fuzz_target;

const LANGUAGES: [&str; 4] = ["x86-64", "x86", "AARCH64", "ARM8_le"];

fn frontend(index: usize) -> Option<&'static RuntimeSleighFrontend> {
    static FRONTENDS: [OnceLock<Option<RuntimeSleighFrontend>>; 4] = [
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
    ];
    FRONTENDS[index]
        .get_or_init(|| RuntimeSleighFrontend::new_for_language(LANGUAGES[index]).ok())
        .as_ref()
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 10 {
        return;
    }
    let Some(frontend) = frontend(usize::from(data[0]) % LANGUAGES.len()) else {
        return;
    };
    let address = u64::from_le_bytes(data[1..9].try_into().unwrap());
    let _ = frontend.decode_and_lift(&data[9..], address);
});
