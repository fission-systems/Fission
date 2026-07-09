use fission_loader::loader::LoadedBinary;
use std::path::PathBuf;

#[test]
fn dump_static_printf_sections() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let b = LoadedBinary::from_file(&path).unwrap();
    let inner = b.inner();
    for s in &inner.sections {
        println!(
            "sec {:20} va=0x{:08x} vsz=0x{:x} writable={} exec={}",
            s.name, s.virtual_address, s.virtual_size, s.is_writable, s.is_executable
        );
    }
    match b.view_bytes(0x1007f68, 64) {
        Some(bytes) => println!("bins@0x1007f68 file view: {:02x?}", &bytes[..32]),
        None => println!("bins@0x1007f68: no file bytes (BSS)"),
    }
    // After load into emulator
    let mut state = fission_emulator::MachineState::new();
    let _ = fission_emulator::os::linux::loader::load_elf(&mut state, &b).unwrap();
    let ram = state.ram_space();
    let got = state.read_space(ram, 0x1007f68, 32).unwrap();
    println!("bins@0x1007f68 after load: {:02x?}", got);
    assert!(got.iter().all(|&b| b == 0), "malloc bin heads must be zeroed BSS");
}
