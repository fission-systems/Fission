//! Regression tests for malformed COFF objects and detector UTF-8 boundaries.

use fission_loader::loader::DataBuffer;
use fission_loader::loader::coff::CoffLoader;
use fission_loader::{DetectionType, LoadedBinary, detect};

const IMAGE_FILE_MACHINE_ARM: u16 = 0x01c0;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;

/// Exact 99-byte `fuzz_coff_object` crash input (`crash-93ad7b60606c026b035660a7c1cb04d1f9ef4417`)
/// from scheduled Fuzz run `45980141928`: an ARM COFF header followed by `GCC:` and invalid
/// non-UTF-8 bytes with no NUL terminator so `from_utf8_lossy` placed `\u{FFFD}` across byte 50.
const FUZZ_COFF_CRASH_GCC_UTF8: [u8; 99] = [
    0xc0, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x41, 0x75, 0x74, 0x6f, 0x49, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x6a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfb, 0xdb, 0xc0, 0x05,
    0x01, 0xfb, 0x00, 0x41, 0x75, 0x74, 0x6f, 0x49, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xff, 0xff, 0x47, 0x43, 0x43, 0x3a, 0xff, 0xfc, 0xfb, 0x3d, 0x3d, 0x2b, 0xff,
    0xff, 0xa0, 0xff, 0xfc, 0xfb, 0x24, 0xc0, 0x05, 0xff, 0xa0, 0xff, 0xfc, 0xfb, 0x24, 0xc0, 0x05,
    0x01, 0xfb, 0x61,
];

#[test]
fn fuzz_coff_gcc_utf8_boundary_crash_input_does_not_panic() {
    let binary = LoadedBinary::from_bytes(
        FUZZ_COFF_CRASH_GCC_UTF8.to_vec(),
        "fuzz_input.obj".to_string(),
    )
    .expect("ARM COFF crash fixture should load cleanly without panicking");

    let detection = detect(&binary);
    assert!(
        detection
            .by_type(DetectionType::Compiler)
            .iter()
            .any(|d| d.name == "GCC"),
        "expected GCC compiler detection on fixture"
    );
}

#[test]
fn coff_gcc_marker_with_varying_invalid_utf8_padding_never_panics() {
    for &machine in &[IMAGE_FILE_MACHINE_ARM, IMAGE_FILE_MACHINE_AMD64] {
        for pad in 0usize..=64 {
            let mut bytes = vec![0u8; 64 + 4 + pad + 40];
            bytes[0..2].copy_from_slice(&machine.to_le_bytes());
            bytes[2..4].copy_from_slice(&1u16.to_le_bytes());
            bytes[20..25].copy_from_slice(b".text");
            bytes[36..40].copy_from_slice(&4u32.to_le_bytes());
            bytes[40..44].copy_from_slice(&60u32.to_le_bytes());
            let chars = IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;
            bytes[56..60].copy_from_slice(&chars.to_le_bytes());

            let marker_start = 64 + pad;
            bytes[marker_start..marker_start + 4].copy_from_slice(b"GCC:");
            for b in &mut bytes[marker_start + 4..] {
                *b = 0xff;
            }

            let binary = LoadedBinary::from_bytes(bytes, format!("pad_{machine:#x}_{pad}.obj"))
                .expect("synthetic COFF object should parse cleanly");
            let _ = detect(&binary);
        }
    }
}

#[test]
fn coff_without_symbol_table_does_not_read_header_as_string_table() {
    let mut bytes = vec![0u8; 128];
    bytes[0..2].copy_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    bytes[2..4].copy_from_slice(&1u16.to_le_bytes());
    // pointer_to_symbol_table == 0, number_of_symbols == 0: no string table exists.
    // Section name `/4` would otherwise index offset 4 inside the COFF header.
    bytes[20..22].copy_from_slice(b"/4");
    bytes[36..40].copy_from_slice(&4u32.to_le_bytes());
    bytes[40..44].copy_from_slice(&64u32.to_le_bytes());
    let chars = IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;
    bytes[56..60].copy_from_slice(&chars.to_le_bytes());

    let binary = CoffLoader::parse(DataBuffer::Heap(bytes), "no_symtab.obj".to_string())
        .expect("COFF without symbol table should still parse");
    assert_eq!(binary.sections.len(), 1);
    assert_eq!(
        binary.sections[0].name, "",
        "missing string table must yield empty long name instead of COFF header bytes"
    );
}

#[test]
fn malformed_coff_overflow_headers_are_refused_without_panicking() {
    let mut overflow_symtab = vec![0u8; 96];
    overflow_symtab[0..2].copy_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    overflow_symtab[2..4].copy_from_slice(&1u16.to_le_bytes());
    overflow_symtab[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    overflow_symtab[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    overflow_symtab[20..25].copy_from_slice(b".text");
    assert!(!CoffLoader::looks_like_coff_object(&overflow_symtab));
    let _ = LoadedBinary::from_bytes(overflow_symtab.clone(), "overflow_symtab.obj".to_string());
    let _ = CoffLoader::parse(
        DataBuffer::Heap(overflow_symtab),
        "overflow_symtab.obj".to_string(),
    );

    let mut huge_opt_header = vec![0u8; 96];
    huge_opt_header[0..2].copy_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    huge_opt_header[2..4].copy_from_slice(&1u16.to_le_bytes());
    huge_opt_header[16..18].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(!CoffLoader::looks_like_coff_object(&huge_opt_header));
    assert!(
        CoffLoader::parse(
            DataBuffer::Heap(huge_opt_header.clone()),
            "huge_opt.obj".to_string()
        )
        .is_err()
    );
    let _ = LoadedBinary::from_bytes(huge_opt_header, "huge_opt.obj".to_string());

    let mut max_aux_symbol = vec![0u8; 128];
    max_aux_symbol[0..2].copy_from_slice(&IMAGE_FILE_MACHINE_AMD64.to_le_bytes());
    max_aux_symbol[2..4].copy_from_slice(&1u16.to_le_bytes());
    max_aux_symbol[8..12].copy_from_slice(&60u32.to_le_bytes());
    max_aux_symbol[12..16].copy_from_slice(&1u32.to_le_bytes());
    max_aux_symbol[20..25].copy_from_slice(b".text");
    max_aux_symbol[36..40].copy_from_slice(&4u32.to_le_bytes());
    max_aux_symbol[40..44].copy_from_slice(&96u32.to_le_bytes());
    max_aux_symbol[60..64].copy_from_slice(b"fn0\0");
    max_aux_symbol[72..74].copy_from_slice(&1i16.to_le_bytes());
    max_aux_symbol[74..76].copy_from_slice(&(2u16 << 4).to_le_bytes());
    max_aux_symbol[76] = 2; // C_EXT
    max_aux_symbol[77] = 255; // max aux count extending past EOF
    let _ = CoffLoader::parse(DataBuffer::Heap(max_aux_symbol), "max_aux.obj".to_string());
}
