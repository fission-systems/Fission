//! A malformed ELF must be refused, not panicked on.

use fission_loader::LoadedBinary;
use std::path::PathBuf;

/// `fuzz_elf_parser` crash `ad595e24`: a section header claiming
/// `sh_offset + sh_size` past `u64::MAX`. The bounds test added the two
/// directly, so with overflow checks on -- which is how `cargo fuzz` builds --
/// the parser panicked before it ever reached the machine-type check that
/// rejects this file. In release the sum wrapped instead, and the test passed
/// or failed by accident.
#[test]
fn a_section_header_whose_offset_plus_size_overflows_is_refused() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/elf_fuzz_crash_ad595e24.bin");
    let data = std::fs::read(&path).expect("fuzz crash fixture");
    let result = LoadedBinary::from_bytes(data, "elf_fuzz_crash_ad595e24.bin".to_string());
    assert!(
        result.is_err(),
        "a malformed ELF should be refused, not parsed"
    );
}
