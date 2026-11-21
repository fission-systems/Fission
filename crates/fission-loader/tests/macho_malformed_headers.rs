//! A malformed Mach-O must be refused or loaded: never panicked on, looped
//! over without end, or recursed into.
//!
//! The sweep is the one `elf_malformed_headers.rs` runs; see
//! `support/mutation.rs` for what it checks.

#[path = "support/mutation.rs"]
mod mutation;

use fission_loader::LoadedBinary;
use mutation::{assert_clean, sweep, sweep_inputs, u32_at};
use std::path::PathBuf;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name);
    std::fs::read(path).expect("fixture")
}

fn load(data: Vec<u8>) {
    let _ = LoadedBinary::from_bytes(data, "mutated.macho".to_string());
}

const LC_SEGMENT: u32 = 0x1;
const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xb;
const LC_SEGMENT_64: u32 = 0x19;
const LC_FUNCTION_STARTS: u32 = 0x26;

/// Every `(file offset, width)` worth overwriting in a little-endian Mach-O:
/// the header, every word of every load command (and every aligned 64-bit
/// field in a 64-bit image), and the first words of the tables the commands
/// point at.
fn macho_mutation_sites(d: &[u8]) -> Vec<(usize, usize)> {
    let is_64 = u32_at(d, 0) == Some(0xfeed_facf);
    let header = if is_64 { 32 } else { 28 };
    let ncmds = u32_at(d, 16).unwrap_or(0) as usize;
    let sizeofcmds = u32_at(d, 20).unwrap_or(0) as usize;

    let mut sites: Vec<(usize, usize)> = (4..header).step_by(4).map(|at| (at, 4)).collect();
    for at in (header..header + sizeofcmds).step_by(4) {
        sites.push((at, 4));
    }
    if is_64 {
        for at in (header..header + sizeofcmds).step_by(8) {
            sites.push((at, 8));
        }
    }

    // Tables named by the load commands: symbols, strings, indirect symbols,
    // function starts, and the start of each section's contents.
    let mut tables = Vec::new();
    let mut cursor = header;
    for _ in 0..ncmds.min(64) {
        let (Some(cmd), Some(size)) = (u32_at(d, cursor), u32_at(d, cursor + 4)) else {
            break;
        };
        match cmd {
            LC_SYMTAB => {
                tables.extend(u32_at(d, cursor + 8)); // symoff
                tables.extend(u32_at(d, cursor + 16)); // stroff
            }
            LC_DYSYMTAB => tables.extend(u32_at(d, cursor + 56)), // indirectsymoff
            LC_FUNCTION_STARTS => tables.extend(u32_at(d, cursor + 8)),
            LC_SEGMENT_64 => {
                let nsects = u32_at(d, cursor + 64).unwrap_or(0) as usize;
                for i in 0..nsects.min(32) {
                    tables.extend(u32_at(d, cursor + 72 + i * 80 + 48)); // offset
                }
            }
            LC_SEGMENT => {
                let nsects = u32_at(d, cursor + 48).unwrap_or(0) as usize;
                for i in 0..nsects.min(32) {
                    tables.extend(u32_at(d, cursor + 56 + i * 68 + 40)); // offset
                }
            }
            _ => {}
        }
        if size < 8 {
            break;
        }
        cursor += size as usize;
    }
    for table in tables {
        let table = table as usize;
        if table == 0 || table >= d.len() {
            continue;
        }
        for at in (table..(table + 48).min(d.len())).step_by(4) {
            sites.push((at, 4));
        }
    }
    sites.sort_unstable();
    sites.dedup();
    sites
}

/// Sweep shard `index` of `of` over the Mach-O fixture `name`.
fn assert_macho_sweep_clean(name: &str, index: usize, of: usize) {
    let base = fixture(name);
    let sites = macho_mutation_sites(&base);
    assert!(
        sites.len() > 100,
        "{name}: found only {} sites",
        sites.len()
    );
    let sites = mutation::shard(&sites, index, of);
    assert_clean(name, &sweep(&base, &sites, false, load));
}

#[test]
fn mutated_x64_macho_never_panics_shard_0() {
    assert_macho_sweep_clean("macho_x64_calls", 0, 3);
}

#[test]
fn mutated_x64_macho_never_panics_shard_1() {
    assert_macho_sweep_clean("macho_x64_calls", 1, 3);
}

#[test]
fn mutated_x64_macho_never_panics_shard_2() {
    assert_macho_sweep_clean("macho_x64_calls", 2, 3);
}

#[test]
fn mutated_x86_macho_object_never_panics() {
    assert_macho_sweep_clean("macho_x86_calls.o", 0, 1);
}

/// The unmutated fixtures load, with the imports and functions the sweeps are
/// meant to reach.
#[test]
fn unmutated_macho_fixtures_load() {
    let binary = LoadedBinary::from_bytes(fixture("macho_x64_calls"), "macho_x64_calls".into())
        .expect("x86-64 Mach-O loads");
    assert!(binary.format.contains("Mach-O"), "{}", binary.format);
    assert!(binary.is_64bit);
    for name in ["main", "bump"] {
        assert!(
            binary.functions.iter().any(|f| f.name == name),
            "`{name}` not found"
        );
    }
    assert!(
        binary
            .functions
            .iter()
            .any(|f| f.is_import && f.name.contains("printf")),
        "no `printf` import"
    );

    let object = LoadedBinary::from_bytes(fixture("macho_x86_calls.o"), "macho_x86_calls.o".into())
        .expect("i386 Mach-O object loads");
    assert!(!object.is_64bit);
    assert!(
        object.functions.iter().any(|f| f.name == "bump"),
        "`bump` not found in the object"
    );
}

// ── Load commands ───────────────────────────────────────────────────────────

/// `cmdsize == 0` leaves the cursor where it is, so the loader re-read the
/// same command `ncmds` times: four billion iterations, each appending the
/// segment's sections again when the command is a segment.
#[test]
fn a_load_command_of_size_zero_is_refused_not_looped_over() {
    for name in ["macho_x64_calls", "macho_x86_calls.o"] {
        let base = fixture(name);
        let header = if u32_at(&base, 0) == Some(0xfeed_facf) {
            32
        } else {
            28
        };
        let ncmds = u32_at(&base, 16).unwrap() as usize;
        let mut inputs = Vec::new();
        let mut cursor = header;
        for index in 0..ncmds {
            let size = u32_at(&base, cursor + 4).unwrap() as usize;
            for cmdsize in [0u32, 4, 7] {
                let mut data = base.clone();
                data[cursor + 4..cursor + 8].copy_from_slice(&cmdsize.to_le_bytes());
                data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
                inputs.push((
                    format!("{name}: command {index} cmdsize {cmdsize}, ncmds u32::MAX"),
                    data,
                ));
            }
            cursor += size;
        }
        assert!(inputs.len() >= 9, "{name}: only {} inputs", inputs.len());
        assert_clean(name, &sweep_inputs(inputs, load));
    }
}

#[test]
fn a_load_command_shorter_than_its_header_is_an_error() {
    let mut data = fixture("macho_x64_calls");
    data[32 + 4..32 + 8].copy_from_slice(&0u32.to_le_bytes());
    let result = LoadedBinary::from_bytes(data, "zero-cmdsize".to_string());
    assert!(result.is_err(), "a zero-sized load command must be refused");
}

// ── Universal files ─────────────────────────────────────────────────────────

const FAT_MAGIC: u32 = 0xcafe_babe;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const SLICE_OFFSET: usize = 0x1000;

/// A universal file holding `slice` as its one x86-64 architecture, with the
/// header in the given byte order.
fn universal(slice: &[u8], big_endian: bool) -> Vec<u8> {
    let word = |v: u32| {
        if big_endian {
            v.to_be_bytes()
        } else {
            v.to_le_bytes()
        }
    };
    let mut out = Vec::new();
    out.extend(word(FAT_MAGIC));
    out.extend(word(1)); // nfat_arch
    out.extend(word(CPU_TYPE_X86_64));
    out.extend(word(3)); // cpusubtype
    out.extend(word(SLICE_OFFSET as u32));
    out.extend(word(slice.len() as u32));
    out.extend(word(12)); // align
    out.resize(SLICE_OFFSET, 0);
    out.extend_from_slice(slice);
    out
}

#[test]
fn a_universal_file_loads_its_slice() {
    let slice = fixture("macho_x64_calls");
    let binary = LoadedBinary::from_bytes(universal(&slice, true), "fat".to_string())
        .expect("universal file loads");
    assert!(binary.format.contains("Mach-O"), "{}", binary.format);
    assert!(binary.functions.iter().any(|f| f.name == "main"));
}

/// `FAT_CIGAM` is a universal header written little-endian. The loader
/// accepted the magic and then read the rest of the header big-endian.
#[test]
fn a_byte_swapped_universal_header_is_read_in_its_own_byte_order() {
    let slice = fixture("macho_x64_calls");
    let slices = fission_loader::loader::macho::universal_slices(&universal(&slice, false));
    assert_eq!(slices.len(), 1, "{slices:?}");
    assert_eq!(slices[0].arch, "x86_64");
    assert_eq!(slices[0].offset, SLICE_OFFSET);
    assert_eq!(slices[0].size, slice.len());

    let binary = LoadedBinary::from_bytes(universal(&slice, false), "fat-swapped".to_string())
        .expect("byte-swapped universal file loads");
    assert!(binary.functions.iter().any(|f| f.name == "main"));
}

/// A slice that starts at offset 0 is the universal file itself: the loader
/// handed those same bytes back to the pipeline and recursed until the stack
/// ran out.
#[test]
fn a_universal_slice_that_is_the_file_itself_is_refused() {
    let mut data = Vec::new();
    data.extend(FAT_MAGIC.to_be_bytes());
    data.extend(1u32.to_be_bytes());
    data.extend(CPU_TYPE_X86_64.to_be_bytes());
    data.extend(3u32.to_be_bytes());
    data.extend(0u32.to_be_bytes()); // offset: the file itself
    data.extend(28u32.to_be_bytes()); // size: all of it
    data.extend(0u32.to_be_bytes());
    assert_eq!(data.len(), 28);
    let result = LoadedBinary::from_bytes(data, "self-fat".to_string());
    assert!(result.is_err(), "a self-referential slice must be refused");
}

/// A slice that is itself a universal file, a few thousand levels deep: each
/// level is smaller than the last, so the recursion does end -- after the
/// stack has.
#[test]
fn nested_universal_files_are_refused_not_recursed_into() {
    const LEVELS: usize = 20_000;
    const HEADER: usize = 28;
    let total = LEVELS * HEADER + 64;
    let mut data = Vec::with_capacity(total);
    for level in 0..LEVELS {
        let rest = total - (level + 1) * HEADER;
        data.extend(FAT_MAGIC.to_be_bytes());
        data.extend(1u32.to_be_bytes());
        data.extend(CPU_TYPE_X86_64.to_be_bytes());
        data.extend(3u32.to_be_bytes());
        data.extend((HEADER as u32).to_be_bytes()); // the slice follows this header
        data.extend((rest as u32).to_be_bytes());
        data.extend(0u32.to_be_bytes());
    }
    data.resize(total, 0);
    let result = LoadedBinary::from_bytes(data, "nested-fat".to_string());
    assert!(result.is_err(), "a nested universal file must be refused");
}

#[test]
fn mutated_universal_header_never_panics() {
    let slice = fixture("macho_x64_calls");
    for big_endian in [true, false] {
        let base = universal(&slice, big_endian);
        let sites: Vec<(usize, usize)> = (4..28).step_by(4).map(|at| (at, 4)).collect();
        assert_clean("universal header", &sweep(&base, &sites, big_endian, load));
    }
}
