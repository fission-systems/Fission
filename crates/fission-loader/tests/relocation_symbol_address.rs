//! A relocation that names a symbol defined in the binary carries that
//! symbol's address, so a consumer can tell same-named symbols apart.
use fission_loader::loader::LoadedBinary;
use std::path::Path;

fn load(name: &str) -> LoadedBinary {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name);
    LoadedBinary::from_file(&path).expect("load fixture")
}

#[test]
fn a_copy_relocation_carries_the_address_of_the_symbol_it_defines() {
    // `typeinfo for std::runtime_error` is copied into the executable's .bss,
    // where the symbol table defines it; the relocation names that symbol.
    let binary = load("x64_dyn_lsda_test.elf");
    let relocation = binary
        .relocations
        .iter()
        .find(|relocation| relocation.symbol_name.as_deref() == Some("_ZTISt13runtime_error"))
        .expect("the copy relocation");
    let address = relocation
        .symbol_address
        .expect("a defined symbol has an address");
    // The copy lives where the relocation points; the loader names it there.
    assert_eq!(address, relocation.address);
    assert!(
        binary
            .global_symbols
            .get(&address)
            .is_some_and(|name| name.contains("runtime_error")),
        "{address:#x} should be where the symbol is defined"
    );
}

#[test]
fn a_relocation_against_an_undefined_symbol_has_no_address() {
    let binary = load("x64_dyn_puts.elf");
    assert!(
        binary
            .relocations
            .iter()
            .filter(|relocation| relocation.symbol_name.is_some())
            .all(|relocation| relocation.symbol_address.is_none()),
        "{:?}",
        binary.relocations
    );
}
