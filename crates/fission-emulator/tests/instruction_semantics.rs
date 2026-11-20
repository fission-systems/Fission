//! Single x86-64 instructions, run from their machine-code bytes in both
//! engines and checked against what the processor does.
//!
//! The lifter's control flow *inside* one instruction -- the `goto <label>`
//! constructs in SLEIGH -- is where both engines share code and so cannot
//! catch each other, so these check against the architecture instead.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Where the instructions go, and a data page next to them.
const CODE: u64 = 0x5000_0000;
const DATA: u64 = 0x5001_0000;

/// An x86-64 machine with `code` at `CODE` (followed by `hlt`-free padding)
/// and nothing else of interest, set to run exactly `count` instructions.
fn machine(code: &[u8], count: u64, interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(count));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");

    emu.state.page_map.map_region(CODE, 0x1000, prot::RX, true);
    emu.state.page_map.map_region(DATA, 0x1000, prot::RW, true);
    let ram = emu.state.ram_space();
    let mut bytes = code.to_vec();
    bytes.resize(0x40, 0x90); // nops after, so a block can run on
    let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
    emu.state.write_space(ram, CODE, &bytes).expect("code");
    emu.state.enforce_page_faults = enforce;
    emu.pc = CODE;
    emu
}

fn write_u32(emu: &mut Emulator, address: u64, value: u32) {
    let ram = emu.state.ram_space();
    emu.state
        .write_space(ram, address, &value.to_le_bytes())
        .expect("write");
}

fn read_u32(emu: &mut Emulator, address: u64) -> u32 {
    let ram = emu.state.ram_space();
    u32::from_le_bytes(
        emu.state
            .read_space(ram, address, 4)
            .unwrap()
            .try_into()
            .unwrap(),
    )
}

/// `cmpxchg [rdi], esi` when `[rdi]` is not `eax`: the comparison fails, so
/// `eax` takes the memory value, memory is untouched and ZF is clear. That
/// path ends with `goto <end>` -- a branch to one past the instruction's last
/// p-code op -- and the step that resolves in-instruction branches only
/// accepted targets strictly inside the instruction.
#[test]
fn a_failed_cmpxchg_loads_the_memory_value_and_stores_nothing() {
    // mov eax, 1 ; cmpxchg [rdi], esi ; mov ebx, 7
    let code = [
        0xB8, 0x01, 0x00, 0x00, 0x00, //
        0x0F, 0xB1, 0x37, //
        0xBB, 0x07, 0x00, 0x00, 0x00,
    ];
    for interpret in [false, true] {
        let mut emu = machine(&code, 3, interpret);
        emu.write_register_u64("RDI", DATA).unwrap();
        emu.write_register_u64("RSI", 5).unwrap();
        write_u32(&mut emu, DATA, 2);
        let _ = emu.run();
        let engine = if interpret { "interpreter" } else { "JIT" };
        assert_eq!(emu.inst_count, 3, "{engine}: ran three instructions");
        assert_eq!(read_u32(&mut emu, DATA), 2, "{engine}: memory untouched");
        assert_eq!(
            emu.read_register_u64("RAX").unwrap(),
            2,
            "{engine}: eax = memory"
        );
        assert_eq!(
            emu.read_register_u64("ZF").unwrap() & 1,
            0,
            "{engine}: ZF clear"
        );
        assert_eq!(
            emu.read_register_u64("RBX").unwrap(),
            7,
            "{engine}: and on to the next"
        );
    }
}

/// The succeeding comparison, for contrast: memory takes `esi`, ZF is set.
#[test]
fn a_successful_cmpxchg_stores_the_new_value() {
    // mov eax, 2 ; cmpxchg [rdi], esi ; mov ebx, 7
    let code = [
        0xB8, 0x02, 0x00, 0x00, 0x00, //
        0x0F, 0xB1, 0x37, //
        0xBB, 0x07, 0x00, 0x00, 0x00,
    ];
    for interpret in [false, true] {
        let mut emu = machine(&code, 3, interpret);
        emu.write_register_u64("RDI", DATA).unwrap();
        emu.write_register_u64("RSI", 5).unwrap();
        write_u32(&mut emu, DATA, 2);
        let _ = emu.run();
        let engine = if interpret { "interpreter" } else { "JIT" };
        assert_eq!(read_u32(&mut emu, DATA), 5, "{engine}: memory = esi");
        assert_eq!(
            emu.read_register_u64("ZF").unwrap() & 1,
            1,
            "{engine}: ZF set"
        );
        assert_eq!(
            emu.read_register_u64("RBX").unwrap(),
            7,
            "{engine}: and on to the next"
        );
    }
}

/// A snapshot saved to disk and loaded back restores the registers the JIT
/// wrote. The JIT keeps registers in the host-side mirror, which the
/// serialiser skipped: the loaded mirror was empty while its bounds check used
/// the full size, so the first register read after `restore_into` panicked.
#[test]
fn a_snapshot_round_trips_through_disk_with_its_registers() {
    // mov eax, 0x1234 ; mov ebx, 0x5678
    let code = [0xB8, 0x34, 0x12, 0x00, 0x00, 0xBB, 0x78, 0x56, 0x00, 0x00];
    let mut emu = machine(&code, 2, false);
    let _ = emu.run();
    assert_eq!(emu.read_register_u64("RAX").unwrap(), 0x1234);

    let snapshot = fission_emulator::EmulatorSnapshot::capture(&emu, emu.pc);
    let dir = std::env::temp_dir().join(format!("fission-snapshot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snapshot.bin");
    snapshot.save_to_disk(&path).expect("save");
    let loaded = fission_emulator::EmulatorSnapshot::load_from_disk(&path).expect("load");
    std::fs::remove_dir_all(&dir).ok();

    let mut fresh = machine(&[0x90], 1, false);
    loaded.restore_into(&mut fresh);
    assert_eq!(fresh.read_register_u64("RAX").unwrap(), 0x1234);
    assert_eq!(fresh.read_register_u64("RBX").unwrap(), 0x5678);
}

/// With no OS image to set it, the stack pointer starts inside the stack
/// `map_binary_to_state` maps, so the first push lands in it.
#[test]
fn the_default_stack_pointer_is_inside_the_default_stack() {
    use fission_emulator::loader::{
        DEFAULT_STACK_SIZE, DEFAULT_STACK_TOP_32, DEFAULT_STACK_TOP_64, map_binary_to_state,
    };
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    map_binary_to_state(&mut state, &binary).expect("map");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(
        state,
        binary,
        sleigh,
        arch,
        Box::new(fission_emulator::os::BareMetalEnv::new()),
    )
    .expect("emulator");
    let sp = emu.read_register_u64("RSP").unwrap();
    let ram = emu.state.ram_space();
    emu.state
        .write_space(ram, sp - 8, &[0xAB; 8])
        .expect("a push at the default stack pointer is in mapped memory");

    // The 32-bit one has no 32-bit fixture to run on; it must at least be
    // clear of the HLE trampolines (0x7FFF_0000..0x8000_0000), which is where
    // it used to be.
    let trampolines = 0x7FFF_0000u64..0x8000_0000;
    assert!(!trampolines.contains(&(DEFAULT_STACK_TOP_32 - 8)));
    assert!(!trampolines.contains(&(DEFAULT_STACK_TOP_32 - DEFAULT_STACK_SIZE)));
    assert_eq!(sp, DEFAULT_STACK_TOP_64);
}
