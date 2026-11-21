//! The Linux layer answers each guest in that guest's own ABI.
//!
//! Structures the kernel writes into guest memory (`struct stat`, `struct
//! iovec`), the registers a syscall and a call use, and the auxiliary vector
//! all differ by architecture. Every fixture here is built from source in
//! this repository (`testdata/src`); none is a corpus binary.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::linux::image_info::at;
use fission_emulator::os::{HleResult, LinuxEnv, OsEnvironment};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

/// A loaded guest, not yet run.
fn guest(name: &str) -> Emulator {
    let path = fixture(name);
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let mut state = MachineState::new();
    let image = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)
        .expect("load fixture image");
    let load_spec = binary.load_spec().expect("load spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("Sleigh frontends")
        .into_iter()
        .next()
        .expect("a Sleigh frontend");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("architecture");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(10_000));
    emu.set_host_stdout_echo(false);
    emu.apply_linux_image(image).expect("apply image");
    emu
}

fn symbol(emu: &Emulator, name: &str) -> u64 {
    emu.binary
        .inner()
        .global_symbols
        .iter()
        .find_map(|(&address, symbol)| (symbol == name).then_some(address))
        .unwrap_or_else(|| panic!("fixture symbol {name} missing"))
}

fn read(emu: &mut Emulator, address: u64, size: usize) -> Vec<u8> {
    let ram = emu.state.ram_space();
    emu.state
        .read_space(ram, address, size)
        .expect("read guest")
}

fn read_u32(emu: &mut Emulator, address: u64) -> u32 {
    u32::from_le_bytes(read(emu, address, 4).try_into().unwrap())
}

fn read_u64(emu: &mut Emulator, address: u64) -> u64 {
    u64::from_le_bytes(read(emu, address, 8).try_into().unwrap())
}

/// The value the auxiliary vector gives for `key`, on a 64-bit guest.
fn auxv(emu: &mut Emulator, key: u64) -> Option<u64> {
    let info = emu.image_info.clone().expect("image info");
    let bytes = read(emu, info.auxv, info.auxv_len as usize);
    bytes.chunks_exact(16).find_map(|pair| {
        (u64::from_le_bytes(pair[..8].try_into().unwrap()) == key)
            .then(|| u64::from_le_bytes(pair[8..].try_into().unwrap()))
    })
}

/// An i386 guest's `writev` reads 8-byte `struct iovec`s and its `fstat64`
/// fills a 96-byte `struct stat64`.
///
/// Both were served in the x86-64 layout: the vector was read with a 16-byte
/// stride and 8-byte fields, so two entries became one with a pointer made of
/// a pointer and a length; and 144 bytes of `struct stat` went into the
/// 96-byte buffer, with the mode where a 32-bit guest keeps something else.
#[test]
fn an_i386_guest_gets_i386_iovec_and_stat64() {
    let mut emu = guest("linux_x86_32_abi.elf");
    emu.run().expect("run to exit");

    let writev_result = symbol(&emu, "writev_result");
    assert_eq!(read_u32(&mut emu, writev_result), 5, "writev returns 5");
    assert_eq!(emu.vfs.files[&1].content, b"abcd\n");

    let fstat_result = symbol(&emu, "fstat_result");
    assert_eq!(read_u32(&mut emu, fstat_result), 0, "fstat64 succeeds");
    let stat = symbol(&emu, "stat_buffer");
    // i386 `struct stat64`: st_mode at 16, st_size (64-bit) at 44.
    assert_eq!(read_u32(&mut emu, stat + 16), 0x81B4, "st_mode");
    assert_eq!(read_u64(&mut emu, stat + 44), 5, "st_size");
    let guard = symbol(&emu, "stat_guard");
    assert_eq!(
        read_u32(&mut emu, guard),
        0x5AFE_C0DE,
        "the word after the 96-byte buffer is the guest's"
    );
}

/// AArch64 uses the generic 128-byte `struct stat`, with `st_mode` at 16.
#[test]
fn an_aarch64_guest_gets_the_generic_stat() {
    let mut emu = guest("linux_aarch64_readlinkat.elf");
    let env = LinuxEnv::new();
    let buffer = emu.heap_alloc(256).expect("guest buffer");
    let ram = emu.state.ram_space();
    emu.state.write_space(ram, buffer, &[0xEE; 256]).unwrap();
    emu.vfs.write(1, b"seven..").unwrap();

    // fstat(1, buffer): number 80 in the generic table.
    emu.write_register_u64("x8", 80).unwrap();
    emu.write_register_u64("x0", 1).unwrap();
    emu.write_register_u64("x1", buffer).unwrap();
    env.dispatch_hle(&mut emu, "syscall").expect("fstat");
    assert_eq!(emu.read_register_u64("x0").unwrap(), 0);

    assert_eq!(read_u32(&mut emu, buffer + 16), 0x81B4, "st_mode");
    assert_eq!(read_u64(&mut emu, buffer + 48), 7, "st_size");
    assert_eq!(
        read(&mut emu, buffer + 128, 16),
        [0xEE; 16],
        "nothing past the 128 bytes of the structure"
    );
}

/// A tainted buffer handed to `write` is a sink hit on every architecture.
/// The check read the arguments from x86-64's registers by name, which on any
/// other guest do not exist, so it saw six zeros and reported nothing.
#[test]
fn a_tainted_write_buffer_is_seen_on_aarch64() {
    let mut emu = guest("linux_aarch64_readlinkat.elf");
    let env = LinuxEnv::new();
    let buffer = emu.heap_alloc(64).expect("guest buffer");
    let ram = emu.state.ram_space();
    emu.state.write_space(ram, buffer, b"secret").unwrap();

    let write = |emu: &mut Emulator| {
        emu.write_register_u64("x8", 64).unwrap();
        emu.write_register_u64("x0", 1).unwrap();
        emu.write_register_u64("x1", buffer).unwrap();
        emu.write_register_u64("x2", 6).unwrap();
        env.dispatch_hle(emu, "syscall").expect("write");
    };

    // Control: an untainted buffer reports nothing.
    emu.set_shadow_mode(fission_emulator::ShadowMode::Taint);
    write(&mut emu);
    assert!(emu.taint.hits.is_empty(), "{:?}", emu.taint.hits);

    emu.taint_range(buffer, 6, "secret");
    write(&mut emu);
    let hits = &emu.taint.hits;
    assert!(
        hits.iter().any(|hit| hit.sink == "syscall buffer"),
        "the buffer's taint reaches the write: {hits:?}"
    );
}

/// `__libc_start_main` enters `main(argc, argv)` the way the guest's calling
/// convention passes arguments and returns. It wrote x86-64's `RDI`/`RSI`
/// and a return address on the stack whatever the guest was.
#[test]
fn libc_start_main_enters_main_in_the_guests_convention() {
    const MAIN: u64 = 0x40_1234;
    const ARGC: u64 = 3;
    const ARGV: u64 = 0x7000_1000;

    // AArch64: arguments in x0/x1, return address in the link register.
    let mut emu = guest("linux_aarch64_readlinkat.elf");
    let env = LinuxEnv::new();
    for (reg, value) in [("x0", MAIN), ("x1", ARGC), ("x2", ARGV), ("x30", 0xDEAD)] {
        emu.write_register_u64(reg, value).unwrap();
    }
    let outcome = env
        .dispatch_hle(&mut emu, "__libc_start_main")
        .expect("__libc_start_main");
    assert!(matches!(outcome, HleResult::JumpTo(MAIN)), "jumps to main");
    assert_eq!(emu.read_register_u64("x0").unwrap(), ARGC);
    assert_eq!(emu.read_register_u64("x1").unwrap(), ARGV);
    assert_eq!(
        emu.read_register_u64("x30").unwrap(),
        fission_emulator::os::linux::POST_MAIN_EXIT_STUB,
        "main returns to the exit stub"
    );

    // i386 cdecl: everything on the stack, above the return address.
    let mut emu = guest("linux_x86_32_abi.elf");
    let env = LinuxEnv::new();
    let sp = emu.read_register_u64("ESP").unwrap() - 0x40;
    emu.write_register_u64("ESP", sp).unwrap();
    let ram = emu.state.ram_space();
    for (slot, value) in [0xDEADu32, MAIN as u32, ARGC as u32, ARGV as u32]
        .into_iter()
        .enumerate()
    {
        emu.state
            .write_space(ram, sp + 4 * slot as u64, &value.to_le_bytes())
            .unwrap();
    }
    let outcome = env
        .dispatch_hle(&mut emu, "__libc_start_main")
        .expect("__libc_start_main");
    assert!(matches!(outcome, HleResult::JumpTo(MAIN)), "jumps to main");
    let sp = emu.read_register_u64("ESP").unwrap();
    assert_eq!(
        u64::from(read_u32(&mut emu, sp)),
        fission_emulator::os::linux::POST_MAIN_EXIT_STUB_32
    );
    assert_eq!(u64::from(read_u32(&mut emu, sp + 4)), ARGC, "main's argc");
    assert_eq!(u64::from(read_u32(&mut emu, sp + 8)), ARGV, "main's argv");
}

/// `AT_PHDR`/`AT_PHENT`/`AT_PHNUM` describe the program header table, which
/// is how a static libc finds its own `PT_TLS`. They were the image base and
/// the number of *sections*.
#[test]
fn the_auxiliary_vector_describes_the_program_headers() {
    let mut emu = guest("x64_static_printf_malloc.elf");
    let file = std::fs::read(fixture("x64_static_printf_malloc.elf")).unwrap();
    let e_phoff = u64::from_le_bytes(file[0x20..0x28].try_into().unwrap());
    let e_phentsize = u64::from(u16::from_le_bytes(file[0x36..0x38].try_into().unwrap()));
    let e_phnum = u64::from(u16::from_le_bytes(file[0x38..0x3a].try_into().unwrap()));

    let phdr = auxv(&mut emu, at::PHDR).expect("AT_PHDR");
    assert_eq!(auxv(&mut emu, at::PHNUM), Some(e_phnum));
    assert_eq!(auxv(&mut emu, at::PHENT), Some(e_phentsize));
    // What is at AT_PHDR in guest memory is the table from the file.
    let table = (e_phentsize * e_phnum) as usize;
    assert_eq!(
        read(&mut emu, phdr, table),
        file[e_phoff as usize..e_phoff as usize + table]
    );
}

/// A shared library whose lowest segment is not at address 0 -- a prelinked
/// one -- is mapped where it says, so its symbols and relocations need no
/// bias. It was given its own base as one, which put every symbol and every
/// relocated slot at twice the address: the call below went to nothing.
///
/// The guest calls the library's `answer` through its PLT, then through a
/// pointer the library's own relocation fills in, and exits with the sum.
#[test]
fn a_library_linked_at_a_non_zero_base_is_bound_where_it_is_mapped() {
    // The library search path is read from the environment. The variable
    // only adds a directory, and the other tests here load no libraries.
    // SAFETY: set before this test starts any thread of its own.
    unsafe {
        std::env::set_var(
            "FISSION_LIB_PATH",
            fixture("x64_prelinked_lib.so").parent().unwrap(),
        );
    }
    let mut emu = guest("x64_needs_prelinked.elf");
    let info = emu.image_info.clone().expect("image info");
    assert_eq!(
        info.dynlink.loaded_libs,
        vec![("x64_prelinked_lib.so".to_string(), 0x700_0000)],
        "the library is loaded, at its link address"
    );
    emu.run().expect("run to exit");
    assert_eq!(emu.exit_code, Some(84), "answer() + (*answer_ptr)()");
}
