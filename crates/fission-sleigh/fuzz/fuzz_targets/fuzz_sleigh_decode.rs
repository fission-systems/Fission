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

/// Language ids, not names: by name "x86" resolves to the x86-64 entry (its
/// arch is "x86" and it sorts first), so 32-bit x86 was never fuzzed.
const LANGUAGES: [&str; 4] = [
    "x86:LE:64:default",
    "x86:LE:32:default",
    "AARCH64:LE:64:v8A",
    "ARM:LE:32:v8",
];

fn frontend(index: usize) -> &'static RuntimeSleighFrontend {
    static FRONTENDS: [OnceLock<RuntimeSleighFrontend>; 4] = [
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
        OnceLock::new(),
    ];
    FRONTENDS[index]
        // A missing resource bundle must fail the run, not fuzz nothing.
        .get_or_init(|| {
            RuntimeSleighFrontend::new_for_language(LANGUAGES[index])
                .unwrap_or_else(|err| panic!("no SLEIGH frontend for {}: {err}", LANGUAGES[index]))
        })
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 10 {
        return;
    }
    let frontend = frontend(usize::from(data[0]) % LANGUAGES.len());
    let address = u64::from_le_bytes(data[1..9].try_into().unwrap());
    let _ = frontend.decode_and_lift(&data[9..], address);
});
