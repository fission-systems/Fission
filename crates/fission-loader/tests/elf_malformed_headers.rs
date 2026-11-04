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

// ── Mutation sweep ──────────────────────────────────────────────────────────
//
// `cargo fuzz` finds these one crash per nightly run. This does the same job
// deterministically: it takes real fixtures, overwrites each header field and
// the first words of each table-like section with values chosen to overflow
// arithmetic or point outside the file, and requires that nothing panics.
// It runs under the test profile, where overflow checks are on, as the fuzzer's
// build is.

use std::collections::BTreeMap;
use std::panic;
use std::sync::Mutex;

static PANICS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

struct Layout;

impl Layout {
    fn u(&self, d: &[u8], at: usize, wide: bool) -> Option<u64> {
        if wide {
            Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
        } else {
            Some(u64::from(u32::from_le_bytes(
                d.get(at..at + 4)?.try_into().ok()?,
            )))
        }
    }
}

fn put(d: &mut [u8], at: usize, width: usize, value: u64) {
    if let Some(slot) = d.get_mut(at..at + width) {
        slot.copy_from_slice(&value.to_le_bytes()[..width]);
    }
}

/// Every `(file offset, width)` of a header field worth overwriting.
fn mutation_sites(d: &[u8]) -> Vec<(usize, usize)> {
    let is64 = d.get(4) == Some(&2);
    let l = Layout;
    let w = if is64 { 8 } else { 4 };
    let mut sites = Vec::new();
    // ELF header: e_entry, e_phoff, e_shoff, then the u16 counts and indices.
    if is64 {
        sites.extend([(0x18, 8), (0x20, 8), (0x28, 8)]);
        sites.extend([(0x36, 2), (0x38, 2), (0x3A, 2), (0x3C, 2), (0x3E, 2)]);
    } else {
        sites.extend([(0x18, 4), (0x1C, 4), (0x20, 4)]);
        sites.extend([(0x2A, 2), (0x2C, 2), (0x2E, 2), (0x30, 2), (0x32, 2)]);
    }
    let (shoff, shentsize, shnum) = if is64 {
        (0x28, 0x3A, 0x3C)
    } else {
        (0x20, 0x2E, 0x30)
    };
    let (phoff, phentsize, phnum) = if is64 {
        (0x20, 0x36, 0x38)
    } else {
        (0x1C, 0x2A, 0x2C)
    };
    let sh_off = l.u(d, shoff, is64).unwrap_or(0) as usize;
    let sh_ent = u16::from_le_bytes([d[shentsize], d[shentsize + 1]]) as usize;
    let sh_num = u16::from_le_bytes([d[shnum], d[shnum + 1]]) as usize;
    let ph_off = l.u(d, phoff, is64).unwrap_or(0) as usize;
    let ph_ent = u16::from_le_bytes([d[phentsize], d[phentsize + 1]]) as usize;
    let ph_num = u16::from_le_bytes([d[phnum], d[phnum + 1]]) as usize;

    for i in 0..sh_num.min(64) {
        let base = sh_off + i * sh_ent;
        // sh_name, sh_type, then every address/size sized field, link, info.
        let fields: &[usize] = if is64 {
            &[0, 4, 8, 16, 24, 32, 40, 44, 48, 56]
        } else {
            &[0, 4, 8, 12, 16, 20, 24, 28, 32, 36]
        };
        for &f in fields {
            let width = if is64 && matches!(f, 0 | 4 | 40 | 44) {
                4
            } else {
                w
            };
            let width = if !is64 { 4 } else { width };
            sites.push((base + f, width));
        }
    }
    for i in 0..ph_num.min(32) {
        let base = ph_off + i * ph_ent;
        let fields: &[usize] = if is64 {
            &[0, 4, 8, 16, 24, 32, 40, 48]
        } else {
            &[0, 4, 8, 12, 16, 20, 24, 28]
        };
        for &f in fields {
            let width = if is64 && matches!(f, 0 | 4) { 4 } else { w };
            let width = if !is64 { 4 } else { width };
            sites.push((base + f, width));
        }
    }
    // First words of table-like sections: symbols, relocations, dynamic,
    // version definitions / needs.
    for i in 0..sh_num.min(64) {
        let base = sh_off + i * sh_ent;
        let (ty, off, size) = if is64 {
            (
                l.u(d, base + 4, false).unwrap_or(0),
                l.u(d, base + 24, true).unwrap_or(0) as usize,
                l.u(d, base + 32, true).unwrap_or(0) as usize,
            )
        } else {
            (
                l.u(d, base + 4, false).unwrap_or(0),
                l.u(d, base + 16, false).unwrap_or(0) as usize,
                l.u(d, base + 20, false).unwrap_or(0) as usize,
            )
        };
        let interesting = matches!(ty, 2 | 4 | 6 | 9 | 11 | 0x6fff_fffd | 0x6fff_fffe);
        if !interesting || off.checked_add(size).is_none_or(|e| e > d.len()) {
            continue;
        }
        let step = if is64 { 8 } else { 4 };
        let mut at = off;
        while at + step <= off + size.min(160) {
            sites.push((at, step));
            at += 4;
        }
    }
    sites
}

/// Overflowing values; the file-length-relative ones are added per fixture.
const VALUES: [u64; 3] = [u64::MAX, u64::MAX - 7, 1 << 63];

fn sweep(fixture: &str) -> BTreeMap<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
    let base = std::fs::read(path).expect("fixture");
    let sites = mutation_sites(&base);
    assert!(
        sites.len() > 50,
        "{fixture}: found only {} sites",
        sites.len()
    );

    panic::set_hook(Box::new(|info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                info.payload()
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
            })
            .unwrap_or_default();
        PANICS.lock().unwrap().push((at, msg));
    }));
    // Overflowing values, and values on either side of the end of the file.
    let len = base.len() as u64;
    let mut values = VALUES.to_vec();
    values.extend([len - 1, len, len + 1]);
    for (at, width) in &sites {
        for &value in &values {
            let mut data = base.clone();
            put(&mut data, *at, *width, value);
            let _ = panic::catch_unwind(|| {
                let _ = LoadedBinary::from_bytes(data, "mutated.elf".to_string());
            });
        }
    }
    let _ = panic::take_hook();
    let mut found = BTreeMap::new();
    for (at, msg) in PANICS.lock().unwrap().drain(..) {
        found.entry(at).or_insert(msg);
    }
    found
}

fn assert_no_panics(fixture: &str) {
    let found = sweep(fixture);
    assert!(
        found.is_empty(),
        "{fixture}: {} distinct panic site(s):\n{}",
        found.len(),
        found
            .iter()
            .map(|(at, msg)| format!("  {at}: {msg}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn mutated_x64_dynamic_elf_never_panics() {
    assert_no_panics("x64_dyn_lsda_test.elf");
}

#[test]
fn mutated_x64_elf_with_debug_info_never_panics() {
    assert_no_panics("x64_dyn_split_debug_test.elf");
}

#[test]
fn mutated_arm32_elf_never_panics() {
    assert_no_panics("arm_mixed_mode.elf");
}
