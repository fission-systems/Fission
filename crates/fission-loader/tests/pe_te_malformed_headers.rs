//! A malformed PE or TE image must be refused or loaded, never panicked on.
//!
//! The sweep is the one `elf_malformed_headers.rs` runs; see
//! `support/mutation.rs` for what it checks.

#[path = "support/mutation.rs"]
mod mutation;

use fission_loader::LoadedBinary;
use mutation::{assert_clean, sweep, u16_at, u32_at};
use std::path::PathBuf;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name);
    std::fs::read(path).expect("fixture")
}

fn load(data: Vec<u8>) {
    let _ = LoadedBinary::from_bytes(data, "mutated.bin".to_string());
}

/// Every `(file offset, width)` of a PE header field worth overwriting, plus
/// the first words of each data directory and of each section's raw data
/// (import/export/exception/relocation/debug tables live there).
fn pe_mutation_sites(d: &[u8]) -> Vec<(usize, usize)> {
    let mut sites = vec![(0x3c, 4)];
    let Some(pe) = u32_at(d, 0x3c).map(|v| v as usize) else {
        return sites;
    };
    // COFF file header: NumberOfSections, PointerToSymbolTable,
    // NumberOfSymbols, SizeOfOptionalHeader.
    sites.extend([(pe + 6, 2), (pe + 12, 4), (pe + 16, 4), (pe + 20, 2)]);
    let optional = pe + 24;
    let optional_size = u16_at(d, pe + 20).unwrap_or(0) as usize;
    let is_plus = u16_at(d, optional) == Some(0x20b);
    // Every 32-bit word of the optional header, data directories included.
    for at in (optional..optional + optional_size).step_by(4) {
        sites.push((at, 4));
    }
    // ImageBase as the 64-bit field it is in PE32+.
    if is_plus {
        sites.push((optional + 24, 8));
    }

    let section_table = optional + optional_size;
    let section_count = u16_at(d, pe + 6).unwrap_or(0) as usize;
    let mut sections = Vec::new();
    for i in 0..section_count.min(32) {
        let base = section_table + i * 40;
        // VirtualSize, VirtualAddress, SizeOfRawData, PointerToRawData.
        sites.extend([
            (base + 8, 4),
            (base + 12, 4),
            (base + 16, 4),
            (base + 20, 4),
        ]);
        if let (Some(va), Some(raw_size), Some(raw)) = (
            u32_at(d, base + 12),
            u32_at(d, base + 16),
            u32_at(d, base + 20),
        ) {
            sections.push((va as usize, raw_size as usize, raw as usize));
        }
    }
    let file_offset = |rva: usize| {
        sections
            .iter()
            .find(|(va, size, _)| rva >= *va && rva < va + size)
            .map(|(va, _, raw)| raw + (rva - va))
    };

    let directories = optional + if is_plus { 112 } else { 96 };
    for i in 0..16 {
        let entry = directories + i * 8;
        let Some(rva) = u32_at(d, entry).map(|v| v as usize) else {
            break;
        };
        if rva == 0 {
            continue;
        }
        if let Some(offset) = file_offset(rva) {
            for at in (offset..offset + 96).step_by(4) {
                sites.push((at, 4));
            }
        }
    }
    for &(_, raw_size, raw) in &sections {
        for at in (raw..raw + raw_size.min(48)).step_by(4) {
            sites.push((at, 4));
        }
    }
    sites.sort_unstable();
    sites.dedup();
    sites
}

/// Sweep shard `index` of `of` over the PE fixture `name`.
fn assert_pe_sweep_clean(name: &str, index: usize, of: usize) {
    let base = fixture(name);
    let sites = pe_mutation_sites(&base);
    assert!(sites.len() > 80, "{name}: found only {} sites", sites.len());
    let sites = mutation::shard(&sites, index, of);
    assert_clean(name, &sweep(&base, &sites, false, load));
}

#[test]
fn mutated_x64_pe_never_panics() {
    assert_pe_sweep_clean("x64_pdb_struct_test.exe", 0, 1);
}

#[test]
fn mutated_x64_pe_with_exception_tables_never_panics_shard_0() {
    assert_pe_sweep_clean("x64_seh_guarded_test.exe", 0, 2);
}

#[test]
fn mutated_x64_pe_with_exception_tables_never_panics_shard_1() {
    assert_pe_sweep_clean("x64_seh_guarded_test.exe", 1, 2);
}

/// A PE32+ whose `ImageBase` leaves no room for a 32-bit RVA above it: every
/// `image_base + rva` in the loader used to overflow.
#[test]
fn a_pe_image_base_at_the_top_of_the_address_space_is_refused() {
    let mut data = fixture("x64_pdb_struct_test.exe");
    let pe = u32_at(&data, 0x3c).unwrap() as usize;
    assert_eq!(u16_at(&data, pe + 24), Some(0x20b), "fixture is PE32+");
    data[pe + 24 + 24..pe + 24 + 32].copy_from_slice(&(u64::MAX - 0x1000).to_le_bytes());
    let result = std::panic::catch_unwind(|| LoadedBinary::from_bytes(data, "top.exe".to_string()));
    let loaded = result.expect("the loader must not panic");
    assert!(
        loaded.is_err(),
        "an image that cannot be mapped must be refused"
    );
}

/// The unmutated fixtures still load: the sweeps above would pass trivially
/// if every input, including the original, were refused at the door.
#[test]
fn unmutated_pe_fixtures_load() {
    for name in ["x64_pdb_struct_test.exe", "x64_seh_guarded_test.exe"] {
        let binary = LoadedBinary::from_bytes(fixture(name), name.to_string())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(binary.format.starts_with("PE"), "{name}: {}", binary.format);
        assert!(!binary.sections.is_empty(), "{name}: no sections");
    }
}

// ── TE ──────────────────────────────────────────────────────────────────────

/// A minimal UEFI Terse Executable: one `.text` section, a base-relocation
/// block and a CodeView debug entry, so the relocation and debug-directory
/// parsers both run.
fn synthetic_te() -> Vec<u8> {
    let mut d = vec![0u8; 1024];
    d[0..2].copy_from_slice(b"VZ");
    d[2..4].copy_from_slice(&0x8664u16.to_le_bytes()); // Machine = x86-64
    d[4] = 1; // NumberOfSections
    d[6..8].copy_from_slice(&120u16.to_le_bytes()); // StrippedSize
    d[8..12].copy_from_slice(&0x1000u32.to_le_bytes()); // AddressOfEntryPoint
    d[12..16].copy_from_slice(&0x1000u32.to_le_bytes()); // BaseOfCode
    d[16..24].copy_from_slice(&0x40_0000u64.to_le_bytes()); // ImageBase
    d[24..28].copy_from_slice(&0x1080u32.to_le_bytes()); // reloc RVA
    d[28..32].copy_from_slice(&16u32.to_le_bytes()); // reloc size
    d[32..36].copy_from_slice(&0x10a0u32.to_le_bytes()); // debug RVA
    d[36..40].copy_from_slice(&28u32.to_le_bytes()); // debug size

    let s = 40;
    d[s..s + 5].copy_from_slice(b".text");
    d[s + 8..s + 12].copy_from_slice(&0x100u32.to_le_bytes()); // VirtualSize
    d[s + 12..s + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // VirtualAddress
    d[s + 16..s + 20].copy_from_slice(&0x100u32.to_le_bytes()); // SizeOfRawData
    d[s + 20..s + 24].copy_from_slice(&180u32.to_le_bytes()); // PointerToRawData
    d[s + 36..s + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes());

    // .text is at file offset 180 - 120 + 40 = 100; relocations at +0x80.
    d[228..232].copy_from_slice(&0x1000u32.to_le_bytes()); // page RVA
    d[232..236].copy_from_slice(&16u32.to_le_bytes()); // block size
    d[236..238].copy_from_slice(&((3u16 << 12) | 0x10).to_le_bytes());
    d[238..240].copy_from_slice(&((10u16 << 12) | 0x20).to_le_bytes());

    // Debug directory at +0xa0, CodeView record at +0xc0.
    d[260 + 12..260 + 16].copy_from_slice(&2u32.to_le_bytes());
    d[260 + 16..260 + 20].copy_from_slice(&36u32.to_le_bytes());
    d[260 + 20..260 + 24].copy_from_slice(&0x10c0u32.to_le_bytes());
    d[260 + 24..260 + 28].copy_from_slice(&372u32.to_le_bytes());
    d[292..296].copy_from_slice(b"RSDS");
    d[296..312].fill(0xaa);
    d[312..316].copy_from_slice(&1u32.to_le_bytes());
    d[316..325].copy_from_slice(b"test.pdb\0");
    d
}

fn te_mutation_sites() -> Vec<(usize, usize)> {
    // The 40-byte header (count and stripped size as their own widths, the
    // image base as 64 bits), the section header, the relocation block and
    // the debug directory with its CodeView record.
    let mut sites = vec![(4, 1), (6, 2), (16, 8)];
    for at in (8..80).step_by(4) {
        sites.push((at, 4));
    }
    for at in (228..244).step_by(2) {
        sites.push((at, 2));
        sites.push((at, 4));
    }
    for at in (260..300).step_by(4) {
        sites.push((at, 4));
    }
    sites
}

#[test]
fn unmutated_te_fixture_loads() {
    let binary =
        LoadedBinary::from_bytes(synthetic_te(), "synthetic.efi".to_string()).expect("TE loads");
    assert!(binary.format.contains("TE"), "{}", binary.format);
    assert_eq!(binary.image_base, 0x40_0000);
    assert_eq!(binary.sections.len(), 1);
}

#[test]
fn mutated_te_never_panics() {
    let base = synthetic_te();
    assert_clean(
        "synthetic TE",
        &sweep(&base, &te_mutation_sites(), false, load),
    );
}
