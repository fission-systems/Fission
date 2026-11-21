//! Mach-O stubs and symbol pointers are named through the indirect symbol
//! table, each section starting at the index its header gives.

use fission_loader::LoadedBinary;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixture() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/macho_x64_calls");
    std::fs::read(path).expect("fixture")
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

fn put_u32(d: &mut [u8], at: usize, value: u32) {
    d[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// File offset of the `section_64` header named `name`.
fn section_header(d: &[u8], name: &str) -> usize {
    const LC_SEGMENT_64: u32 = 0x19;
    let ncmds = u32_at(d, 16) as usize;
    let mut cursor = 32;
    for _ in 0..ncmds {
        let (cmd, size) = (u32_at(d, cursor), u32_at(d, cursor + 4) as usize);
        if cmd == LC_SEGMENT_64 {
            let nsects = u32_at(d, cursor + 64) as usize;
            for i in 0..nsects {
                let header = cursor + 72 + i * 80;
                let sectname = &d[header..header + 16];
                let end = sectname.iter().position(|b| *b == 0).unwrap_or(16);
                if &sectname[..end] == name.as_bytes() {
                    return header;
                }
            }
        }
        cursor += size;
    }
    panic!("no section {name}");
}

/// File offset of the indirect symbol table (`LC_DYSYMTAB::indirectsymoff`).
fn indirect_table(d: &[u8]) -> usize {
    const LC_DYSYMTAB: u32 = 0xb;
    let ncmds = u32_at(d, 16) as usize;
    let mut cursor = 32;
    for _ in 0..ncmds {
        let (cmd, size) = (u32_at(d, cursor), u32_at(d, cursor + 4) as usize);
        if cmd == LC_DYSYMTAB {
            return u32_at(d, cursor + 56) as usize;
        }
        cursor += size;
    }
    panic!("no LC_DYSYMTAB");
}

const RESERVED1: usize = 68;

/// Import name by address, for the addresses the loader named.
fn imports(data: Vec<u8>) -> BTreeMap<u64, String> {
    let binary = LoadedBinary::from_bytes(data, "macho".to_string()).expect("loads");
    binary
        .functions
        .iter()
        .filter(|f| f.is_import)
        .map(|f| (f.address, f.name.clone()))
        .collect()
}

const STUB_PRINTF: u64 = 0x1_0000_060a;
const STUB_PUTS: u64 = 0x1_0000_0610;
const GOT_BINDER: u64 = 0x1_0000_1000;
const LAZY_PRINTF: u64 = 0x1_0000_1008;
const LAZY_PUTS: u64 = 0x1_0000_1010;

fn expected() -> BTreeMap<u64, String> {
    [
        (STUB_PRINTF, "printf"),
        (STUB_PUTS, "puts"),
        (GOT_BINDER, "dyld_stub_binder"),
        (LAZY_PRINTF, "printf"),
        (LAZY_PUTS, "puts"),
    ]
    .into_iter()
    .map(|(address, name)| (address, name.to_string()))
    .collect()
}

#[test]
fn stubs_and_symbol_pointers_are_named_from_the_indirect_table() {
    let data = fixture();
    // The linker's layout: stubs first, then the GOT, then the lazy pointers.
    assert_eq!(
        u32_at(&data, section_header(&data, "__stubs") + RESERVED1),
        0
    );
    assert_eq!(u32_at(&data, section_header(&data, "__got") + RESERVED1), 2);
    assert_eq!(
        u32_at(&data, section_header(&data, "__la_symbol_ptr") + RESERVED1),
        3
    );
    assert_eq!(imports(data), expected());
}

/// The same image with the GOT's entry moved to the front of the table and
/// the section headers updated to match: a layout as valid as the linker's.
/// The table was indexed as if stubs always came first and the GOT straight
/// after them, so here every stub and the GOT slot got another symbol's name.
#[test]
fn each_section_starts_at_the_table_index_its_header_gives() {
    let mut data = fixture();
    let table = indirect_table(&data);
    let entries: Vec<u32> = (0..5).map(|i| u32_at(&data, table + i * 4)).collect();
    let (printf, puts, binder) = (entries[0], entries[1], entries[2]);
    for (i, symbol) in [binder, printf, puts, printf, puts].into_iter().enumerate() {
        put_u32(&mut data, table + i * 4, symbol);
    }
    let got = section_header(&data, "__got");
    let stubs = section_header(&data, "__stubs");
    put_u32(&mut data, got + RESERVED1, 0);
    put_u32(&mut data, stubs + RESERVED1, 1);
    assert_eq!(imports(data), expected());
}

/// A stub is as wide as its section header says (`reserved2`), not six bytes
/// because the image is x86. Declared twelve bytes wide, the same twelve-byte
/// section holds one stub, not two.
#[test]
fn a_stub_is_as_wide_as_its_section_says() {
    let mut data = fixture();
    let stubs = section_header(&data, "__stubs");
    assert_eq!(u32_at(&data, stubs + RESERVED1 + 4), 6);
    put_u32(&mut data, stubs + RESERVED1 + 4, 12);
    let named = imports(data);
    assert_eq!(named.get(&STUB_PRINTF).map(String::as_str), Some("printf"));
    assert_eq!(named.get(&STUB_PUTS), None);
}
