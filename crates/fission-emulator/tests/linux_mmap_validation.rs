use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{HleResult, LinuxEnv, OsEnvironment};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

const PROT_NONE: u64 = 0x0;
const PROT_READ: u64 = 0x1;
const PROT_WRITE: u64 = 0x2;

const MAP_SHARED: u64 = 0x01;
const MAP_PRIVATE: u64 = 0x02;
const MAP_SHARED_VALIDATE: u64 = 0x03;
const MAP_FIXED: u64 = 0x10;
const MAP_ANONYMOUS: u64 = 0x20;
const MAP_HUGETLB: u64 = 0x40000;
const MAP_FIXED_NOREPLACE: u64 = 0x100000;

const EBADF: i64 = 9;
const ENOMEM: i64 = 12;
const EACCES: i64 = 13;
const EEXIST: i64 = 17;
const ENODEV: i64 = 19;
const EINVAL: i64 = 22;
const EOPNOTSUPP: i64 = 95;

const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;

fn build_x64_linux_emu() -> (LinuxEnv, Emulator) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load x64 ELF fixture");
    let mut state = MachineState::new();
    let image =
        fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("load elf");
    let load_spec = binary.load_spec().expect("load_spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("sleigh candidates")
        .into_iter()
        .next()
        .expect("sleigh frontend");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("construct emulator");
    emu.apply_linux_image(image).expect("apply image");
    (LinuxEnv::new(), emu)
}

fn call_sys_mmap(
    env: &LinuxEnv,
    emu: &mut Emulator,
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    fd: u64,
    offset: u64,
) -> i64 {
    for (reg, val) in [
        ("RDI", addr),
        ("RSI", len),
        ("RDX", prot),
        ("R10", flags),
        ("R8", fd),
        ("R9", offset),
    ] {
        emu.write_register_u64(reg, val).expect("write syscall arg");
    }
    emu.write_register_u64("RAX", 9).expect("write mmap sysnum");
    let outcome = env.dispatch_hle(emu, "syscall").expect("dispatch syscall");
    assert!(matches!(outcome, HleResult::Continue));
    emu.read_register_u64("RAX").expect("read RAX") as i64
}

fn call_libc_mmap(
    env: &LinuxEnv,
    emu: &mut Emulator,
    addr: u64,
    len: u64,
    prot: u64,
    flags: u64,
    fd: u64,
    offset: u64,
) -> u64 {
    for (reg, val) in [
        ("RDI", addr),
        ("RSI", len),
        ("RDX", prot),
        ("RCX", flags),
        ("R8", fd),
        ("R9", offset),
    ] {
        emu.write_register_u64(reg, val).expect("write SysV arg");
    }
    let outcome = env.dispatch_hle(emu, "mmap").expect("dispatch libc mmap");
    assert!(matches!(outcome, HleResult::Continue));
    emu.read_register_u64("RAX").expect("read return RAX")
}

fn call_sys_openat(env: &LinuxEnv, emu: &mut Emulator, guest_name: &str, flags: u64) -> i64 {
    let mut bytes = guest_name.as_bytes().to_vec();
    bytes.push(0);
    let ptr = emu.heap_alloc(bytes.len() as u64).expect("alloc path");
    emu.state
        .write_space(emu.state.ram_space(), ptr, &bytes)
        .expect("write path");
    for (reg, val) in [
        ("RDI", (-100i64) as u64), // AT_FDCWD
        ("RSI", ptr),
        ("RDX", flags),
        ("R10", 0),
    ] {
        emu.write_register_u64(reg, val).expect("write openat arg");
    }
    emu.write_register_u64("RAX", 257)
        .expect("write openat sysnum");
    let _ = env.dispatch_hle(emu, "syscall").expect("dispatch openat");
    emu.read_register_u64("RAX").expect("read fd") as i64
}

#[test]
fn invalid_fd_access_offsets_and_flags_return_typed_errors_without_mutating_state() {
    let (env, mut emu) = build_x64_linux_emu();
    emu.vfs
        .seed_path("seeded.bin", b"0123456789abcdef".to_vec());

    let rw_fd = call_sys_openat(&env, &mut emu, "seeded.bin", O_RDWR) as u64;
    let wronly_fd = call_sys_openat(&env, &mut emu, "seeded.bin", O_WRONLY) as u64;

    let before_mappings = emu.state.page_map.mappings().to_vec();

    // 1. Negative / non-existent fd on file-backed mmap -> -EBADF / MAP_FAILED.
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            u64::MAX,
            0
        ),
        -EBADF
    );
    assert_eq!(
        call_sys_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_PRIVATE, 9999, 0),
        -EBADF
    );
    assert_eq!(
        call_libc_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_PRIVATE, 9999, 0),
        u64::MAX
    );

    // 2. Non-mappable stdio character stream (fd 0, 1) -> -ENODEV / MAP_FAILED.
    assert_eq!(
        call_sys_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_PRIVATE, 0, 0),
        -ENODEV
    );
    assert_eq!(
        call_libc_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_PRIVATE, 1, 0),
        u64::MAX
    );

    // 3. Write-only descriptor cannot back mmap -> -EACCES / MAP_FAILED.
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            wronly_fd,
            0
        ),
        -EACCES
    );
    assert_eq!(
        call_libc_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            wronly_fd,
            0
        ),
        u64::MAX
    );

    // 4. Unaligned file offset -> -EINVAL / MAP_FAILED.
    assert_eq!(
        call_sys_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_PRIVATE, rw_fd, 4),
        -EINVAL
    );
    assert_eq!(
        call_libc_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            rw_fd,
            0x1001
        ),
        u64::MAX
    );

    // 5. Fixed null or unaligned address, invalid prot, or missing MAP_PRIVATE/MAP_SHARED -> -EINVAL.
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED,
            u64::MAX,
            0,
        ),
        -EINVAL
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0x2000_0123,
            0x1000,
            PROT_READ,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
            u64::MAX,
            0,
        ),
        -EINVAL
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_ANONYMOUS, // missing MAP_PRIVATE / MAP_SHARED
            u64::MAX,
            0,
        ),
        -EINVAL
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            0x100, // unknown prot bit
            MAP_PRIVATE | MAP_ANONYMOUS,
            u64::MAX,
            0,
        ),
        -EINVAL
    );

    // 6. Unmodeled MAP_SHARED / MAP_SHARED_VALIDATE / MAP_HUGETLB -> explicit -EOPNOTSUPP.
    assert_eq!(
        call_sys_mmap(&env, &mut emu, 0, 0x1000, PROT_READ, MAP_SHARED, rw_fd, 0),
        -EOPNOTSUPP
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_SHARED | MAP_ANONYMOUS,
            u64::MAX,
            0,
        ),
        -EOPNOTSUPP
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_SHARED_VALIDATE | MAP_ANONYMOUS,
            u64::MAX,
            0,
        ),
        -EOPNOTSUPP
    );
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB,
            u64::MAX,
            0,
        ),
        -EOPNOTSUPP
    );

    // None of the rejected requests modified existing mappings.
    assert_eq!(emu.state.page_map.mappings(), before_mappings.as_slice());
}

#[test]
fn fixed_noreplace_preserves_existing_bytes_permissions_and_allows_empty_slots() {
    let (env, mut emu) = build_x64_linux_emu();
    let ram = emu.state.ram_space();

    // Establish an initial anonymous RW mapping at an explicit free aligned slot.
    let target = 0x0000_0000_7200_0000_u64;
    let mapped = call_sys_mmap(
        &env,
        &mut emu,
        target,
        0x2000,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
        u64::MAX,
        0,
    );
    assert_eq!(mapped, target as i64);
    emu.state
        .write_space(ram, target + 0x1000, b"DO_NOT_CLOBBER")
        .expect("write sentinel");

    let before_prot = emu.state.page_map.page_flags(target + 0x1000);
    let before_mappings = emu.state.page_map.mappings().to_vec();

    // Attempt overlapping MAP_FIXED_NOREPLACE -> must return -EEXIST without changing bytes or prot!
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            target + 0x1000,
            0x1000,
            PROT_NONE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
            u64::MAX,
            0,
        ),
        -EEXIST
    );
    assert_eq!(
        call_libc_mmap(
            &env,
            &mut emu,
            target,
            0x1000,
            PROT_READ,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
            u64::MAX,
            0,
        ),
        u64::MAX
    );

    assert_eq!(emu.state.page_map.mappings(), before_mappings.as_slice());
    assert_eq!(emu.state.page_map.page_flags(target + 0x1000), before_prot);
    assert_eq!(
        emu.state
            .read_space(ram, target + 0x1000, b"DO_NOT_CLOBBER".len())
            .expect("read sentinel"),
        b"DO_NOT_CLOBBER"
    );

    // Adjacent unmapped page still succeeds via MAP_FIXED_NOREPLACE and libc::Mmap.
    let adj = target + 0x2000;
    let adj_res = call_libc_mmap(
        &env,
        &mut emu,
        adj,
        0x1000,
        PROT_READ,
        MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
        u64::MAX,
        0,
    );
    assert_eq!(adj_res, adj);
    let anon_mapping = emu
        .state
        .page_map
        .mappings()
        .iter()
        .find(|m| m.start <= adj && adj < m.end)
        .expect("anonymous mapping");
    assert!(anon_mapping.anon);
}

#[test]
fn private_file_mapping_copies_bytes_zero_fills_partial_eof_page_and_rejects_beyond_eof() {
    let (env, mut emu) = build_x64_linux_emu();
    let ram = emu.state.ram_space();

    // 4112 bytes = 1 full page + 16 bytes in page 2.
    let mut content = vec![0xAAu8; 0x1000];
    content.extend_from_slice(b"SECOND_PAGE_DATA");
    let fd = emu.vfs.open("payload.bin", content);

    // Mapping 2 pages cleanly covers [0, 0x2000) within `page_align_up(4112) == 0x2000`.
    let base_val = call_libc_mmap(
        &env,
        &mut emu,
        0,
        0x1010,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE,
        fd,
        0,
    );
    assert_ne!(base_val, u64::MAX);

    let file_mapping = emu
        .state
        .page_map
        .mappings()
        .iter()
        .find(|m| m.start <= base_val && base_val < m.end)
        .expect("file mapping entry");
    assert!(
        !file_mapping.anon,
        "file-backed mapping must preserve anon == false"
    );
    assert_eq!(
        emu.state.read_space(ram, base_val + 0x1000, 16).unwrap(),
        b"SECOND_PAGE_DATA"
    );
    // Remaining bytes in partial EOF page are legitimately zero-filled.
    assert_eq!(
        emu.state.read_space(ram, base_val + 0x1010, 16).unwrap(),
        [0u8; 16]
    );

    // Private copy semantics: guest write does not mutate VFS file; closing fd leaves mapping intact.
    emu.state
        .write_space(ram, base_val + 0x1000, b"GUEST_PRIVATE___")
        .expect("write private page");
    assert_eq!(
        emu.vfs.file_bytes(fd, 0x1000, 16).unwrap(),
        b"SECOND_PAGE_DATA"
    );
    emu.vfs.close(fd).expect("close fd");
    assert_eq!(
        emu.state.read_space(ram, base_val + 0x1000, 16).unwrap(),
        b"GUEST_PRIVATE___"
    );

    // Reopen a 16-byte file and request a 2-page mapping (2nd page is strictly beyond EOF -> EOPNOTSUPP).
    let small_fd = emu.vfs.open("small.bin", b"0123456789abcdef".to_vec());
    let before = emu.state.page_map.mappings().to_vec();
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x2000,
            PROT_READ,
            MAP_PRIVATE,
            small_fd,
            0
        ),
        -EOPNOTSUPP
    );
    // Page-aligned offset >= EOF (0x1000 >= 16) -> also EOPNOTSUPP (never fabricate zero pages!).
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            small_fd,
            0x1000
        ),
        -EOPNOTSUPP
    );
    // Empty file mapped -> EOPNOTSUPP.
    let empty_fd = emu.vfs.open("empty.bin", Vec::new());
    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0,
            0x1000,
            PROT_READ,
            MAP_PRIVATE,
            empty_fd,
            0
        ),
        -EOPNOTSUPP
    );
    assert_eq!(emu.state.page_map.mappings(), before.as_slice());
}

#[test]
fn oversized_length_returns_enomem_and_never_wraps_address_space() {
    let (env, mut emu) = build_x64_linux_emu();
    let before = emu.state.page_map.mappings().to_vec();

    assert_eq!(
        call_sys_mmap(
            &env,
            &mut emu,
            0x7FFF_0000_0000,
            0x10_0000,
            PROT_READ,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
            u64::MAX,
            0,
        ),
        -ENOMEM
    );
    assert_eq!(emu.state.page_map.mappings(), before.as_slice());
}
