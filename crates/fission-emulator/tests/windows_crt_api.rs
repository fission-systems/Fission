//! What a real Windows program asks the C runtime for.
//!
//! Six programs built from published sources -- sqlite3, lua, minigzip,
//! bzip2, zstd, cJSON -- were run under this emulator, and the report of what
//! they were waiting on was the specification for these handlers. This file
//! is that list turned into assertions, so the next person to touch the
//! layer finds out at `cargo test` rather than from a program that silently
//! prints nothing.
//!
//! The fixture is a PE from this crate's own testdata, so these run
//! everywhere rather than only where the dev corpus is checked out.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{BareMetalEnv, WindowsEnv};
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Somewhere to put strings and output buffers the guest can reach.
const SCRATCH: u64 = 0x3000_0000;

fn windows_emulator() -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/win_x64_exit.exe");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::windows::loader::load_pe(&mut state, &binary).expect("pe");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    // `Emulator::new` runs `patch_imports`, which is what builds the CRT data
    // page these handlers hand out addresses inside.
    let mut emu =
        Emulator::new(state, binary, sleigh, arch, Box::new(WindowsEnv::new())).expect("emulator");
    emu.apply_windows_image(info).expect("image");
    emu.state
        .page_map
        .map_region(SCRATCH, 0x4000, prot::RW, true);
    emu
}

/// Call one API the way the dispatcher does, with the arguments where the
/// calling convention puts them. Returns the value in the return register.
fn call(emu: &mut Emulator, name: &str, args: &[u64]) -> u64 {
    // A real call has pushed a frame, so the stack argument area is inside
    // the mapped stack. Nothing has been pushed here, and the initial stack
    // pointer sits at the top of the region -- writing above it leaves the
    // mapping entirely.
    if args.len() > emu.arch.cc.arg_regs().len() {
        let sp = emu.read_register_u64(emu.arch.sp_reg).expect("sp");
        emu.write_register_u64(emu.arch.sp_reg, sp - 0x200)
            .expect("frame");
    }
    let registers = emu.arch.cc.arg_regs().len();
    for (index, value) in args.iter().enumerate() {
        if index < registers {
            let register = emu.arch.cc.arg_regs()[index];
            emu.write_register_u64(register, *value).expect("arg");
        } else {
            let offset = emu.arch.cc.stack_arg_offset(index - registers);
            let sp = emu
                .read_register_u64(emu.arch.sp_reg)
                .expect("stack pointer");
            let space = emu.state.ram_space();
            emu.state
                .write_space(space, sp + offset, &value.to_le_bytes())
                .expect("stack arg");
        }
    }

    // The environment and the emulator cannot both be borrowed, so it is
    // lifted out for the call and put back.
    let os = std::mem::replace(&mut emu.os, Box::new(BareMetalEnv::new()));
    os.dispatch_hle(emu, name).expect("dispatch");
    emu.os = os;

    emu.read_register_u64(emu.arch.cc.return_reg())
        .expect("return value")
}

/// Put a NUL-terminated string in guest memory and answer with its address.
fn plant(emu: &mut Emulator, at: u64, text: &str) -> u64 {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let space = emu.state.ram_space();
    emu.state.write_space(space, at, &bytes).expect("plant");
    at
}

fn read_back(emu: &mut Emulator, at: u64, len: usize) -> Vec<u8> {
    let space = emu.state.ram_space();
    emu.state.read_space(space, at, len).expect("read back")
}

/// What the guest has written to a standard stream so far.
fn stream_text(emu: &Emulator, fd: u64) -> String {
    emu.vfs
        .files
        .get(&fd)
        .map(|file| String::from_utf8_lossy(&file.content).into_owned())
        .unwrap_or_default()
}

// ── The streams ─────────────────────────────────────────────────────────────

#[test]
fn the_three_standard_streams_are_distinct_and_name_their_descriptors() {
    let mut emu = windows_emulator();
    let streams: Vec<u64> = (0..3)
        .map(|i| call(&mut emu, "__acrt_iob_func", &[i]))
        .collect();

    assert!(
        streams.iter().all(|s| *s != 0),
        "__acrt_iob_func answered with a null FILE*: {streams:?}"
    );
    assert_ne!(streams[0], streams[1], "stdin and stdout are the same FILE");
    assert_ne!(
        streams[1], streams[2],
        "stdout and stderr are the same FILE"
    );

    // `_fileno` has to invert it, or a program that mixes the two APIs
    // writes its output to the wrong place.
    for (fd, stream) in streams.iter().enumerate() {
        assert_eq!(
            call(&mut emu, "_fileno", &[*stream]),
            fd as u64,
            "_fileno disagreed with __acrt_iob_func for stream {fd}"
        );
    }
}

#[test]
fn the_descriptor_apis_agree_with_gethandle() {
    let mut emu = windows_emulator();
    for fd in 0..3u64 {
        // STD_INPUT_HANDLE is -10, and stdout and stderr follow it down.
        let from_get_std = call(&mut emu, "GetStdHandle", &[(-10i64 - fd as i64) as u64]);
        let from_osfhandle = call(&mut emu, "_get_osfhandle", &[fd]);
        assert_eq!(
            from_osfhandle, from_get_std,
            "fd {fd}: _get_osfhandle and GetStdHandle named different handles"
        );
        assert_eq!(
            call(&mut emu, "GetFileType", &[from_osfhandle]),
            2, // FILE_TYPE_CHAR
            "fd {fd} is not a console, so the CRT will buffer its output"
        );
        assert_eq!(call(&mut emu, "_isatty", &[fd]), 1, "fd {fd} is not a tty");
    }
    assert_eq!(
        call(&mut emu, "_isatty", &[7]),
        0,
        "an ordinary descriptor claimed to be a console"
    );
}

// ── Formatted output, which is the whole point ──────────────────────────────

/// Lay out a `va_list` and call `__stdio_common_vfprintf` the way the
/// `printf` inline in the UCRT headers does.
fn vfprintf(emu: &mut Emulator, stream: u64, fmt: &str, args: &[u64]) -> u64 {
    let format = plant(emu, SCRATCH, fmt);
    let arglist = SCRATCH + 0x800;
    let space = emu.state.ram_space();
    for (index, value) in args.iter().enumerate() {
        emu.state
            .write_space(space, arglist + index as u64 * 8, &value.to_le_bytes())
            .expect("va_list");
    }
    call(
        emu,
        "__stdio_common_vfprintf",
        &[0, stream, format, 0, arglist],
    )
}

#[test]
fn printf_reaches_standard_output() {
    let mut emu = windows_emulator();
    let stdout = call(&mut emu, "__acrt_iob_func", &[1]);
    let before = stream_text(&emu, 1).len();

    let written = vfprintf(&mut emu, stdout, "answer=%d\n", &[42]);

    assert_eq!(written, "answer=42\n".len() as u64);
    assert!(
        stream_text(&emu, 1)[before..].contains("answer=42"),
        "stdout holds {:?}",
        stream_text(&emu, 1)
    );
}

/// stderr is a different stream, and keeping it different is information: a
/// program's diagnostics and its results are not the same thing.
#[test]
fn printf_to_stderr_does_not_land_on_stdout() {
    let mut emu = windows_emulator();
    let stderr = call(&mut emu, "__acrt_iob_func", &[2]);
    let stdout_before = stream_text(&emu, 1);

    let name = plant(&mut emu, SCRATCH + 0x400, "x.txt");
    vfprintf(&mut emu, stderr, "cannot open %s\n", &[name]);

    assert!(stream_text(&emu, 2).contains("cannot open x.txt"));
    assert_eq!(
        stream_text(&emu, 1),
        stdout_before,
        "a diagnostic landed on standard output"
    );
}

#[test]
fn sprintf_truncates_but_reports_the_full_length() {
    let mut emu = windows_emulator();
    let format = plant(&mut emu, SCRATCH, "%s-%d");
    let text = plant(&mut emu, SCRATCH + 0x400, "abcdefgh");
    let arglist = SCRATCH + 0x800;
    let buffer = SCRATCH + 0xC00;
    let space = emu.state.ram_space();
    emu.state
        .write_space(space, arglist, &text.to_le_bytes())
        .expect("va_list");
    emu.state
        .write_space(space, arglist + 8, &7u64.to_le_bytes())
        .expect("va_list");

    let returned = call(
        &mut emu,
        "__stdio_common_vsprintf",
        &[0, buffer, 5, format, 0, arglist],
    );

    // "abcdefgh-7" is ten characters; four fit, plus the NUL.
    assert_eq!(returned, 10, "the return has to be the untruncated length");
    let written = read_back(&mut emu, buffer, 5);
    assert_eq!(&written, b"abcd\0", "got {written:?}");
}

#[test]
fn sscanf_assigns_the_fields_it_converted() {
    let mut emu = windows_emulator();
    let input = plant(&mut emu, SCRATCH, "12 beef hello");
    let format = plant(&mut emu, SCRATCH + 0x400, "%d %x %s");
    let arglist = SCRATCH + 0x800;
    let out_int = SCRATCH + 0xC00;
    let out_hex = SCRATCH + 0xC10;
    let out_text = SCRATCH + 0xC20;
    let space = emu.state.ram_space();
    for (index, destination) in [out_int, out_hex, out_text].iter().enumerate() {
        emu.state
            .write_space(
                space,
                arglist + index as u64 * 8,
                &destination.to_le_bytes(),
            )
            .expect("va_list");
    }

    let assigned = call(
        &mut emu,
        "__stdio_common_vsscanf",
        &[0, input, 13, format, 0, arglist],
    );

    assert_eq!(assigned, 3, "three conversions, three assignments");
    assert_eq!(
        u32::from_le_bytes(read_back(&mut emu, out_int, 4).try_into().unwrap()),
        12
    );
    assert_eq!(
        u32::from_le_bytes(read_back(&mut emu, out_hex, 4).try_into().unwrap()),
        0xbeef
    );
    assert_eq!(&read_back(&mut emu, out_text, 6), b"hello\0");
}

// ── Files ───────────────────────────────────────────────────────────────────

/// A file that is not there must not open. A stream for a missing file reads
/// as an empty one, and the program then does its work on nothing and
/// reports success -- which is worse than the failure it already handles.
#[test]
fn fopen_fails_for_a_file_that_is_not_there() {
    let mut emu = windows_emulator();
    let path = plant(&mut emu, SCRATCH, "nowhere.txt");
    let mode = plant(&mut emu, SCRATCH + 0x100, "rb");
    assert_eq!(call(&mut emu, "fopen", &[path, mode]), 0);
}

#[test]
fn a_seeded_file_opens_and_reads_back_what_it_holds() {
    let mut emu = windows_emulator();
    emu.vfs.seed_path("data.bin", b"hello file".to_vec());
    let path = plant(&mut emu, SCRATCH, "data.bin");
    let mode = plant(&mut emu, SCRATCH + 0x100, "rb");

    let stream = call(&mut emu, "fopen", &[path, mode]);
    assert_ne!(stream, 0, "a seeded file did not open");

    let buffer = SCRATCH + 0x1000;
    let items = call(&mut emu, "fread", &[buffer, 1, 10, stream]);
    assert_eq!(items, 10, "fread returns items, not bytes");
    assert_eq!(&read_back(&mut emu, buffer, 10), b"hello file");

    // And the second read finds the cursor where the first left it.
    assert_eq!(call(&mut emu, "feof", &[stream]), 1);
    assert_eq!(call(&mut emu, "fclose", &[stream]), 0);
}

#[test]
fn fwrite_to_stdout_is_the_same_stream_printf_uses() {
    let mut emu = windows_emulator();
    let stdout = call(&mut emu, "__acrt_iob_func", &[1]);
    let text = plant(&mut emu, SCRATCH, "direct");
    let before = stream_text(&emu, 1).len();

    let items = call(&mut emu, "fwrite", &[text, 1, 6, stdout]);

    assert_eq!(items, 6);
    assert!(stream_text(&emu, 1)[before..].contains("direct"));
}

#[test]
fn puts_adds_the_newline_and_fputs_does_not() {
    let mut emu = windows_emulator();
    let stdout = call(&mut emu, "__acrt_iob_func", &[1]);
    let text = plant(&mut emu, SCRATCH, "line");

    let before = stream_text(&emu, 1).len();
    call(&mut emu, "puts", &[text]);
    let after_puts = stream_text(&emu, 1);
    assert_eq!(&after_puts[before..], "line\n");

    let before = after_puts.len();
    call(&mut emu, "fputs", &[text, stdout]);
    assert_eq!(&stream_text(&emu, 1)[before..], "line");
}

// ── The string functions ────────────────────────────────────────────────────

#[test]
fn the_string_comparisons_answer_what_c_answers() {
    let mut emu = windows_emulator();
    let abc = plant(&mut emu, SCRATCH, "abc");
    let abd = plant(&mut emu, SCRATCH + 0x100, "abd");
    let abc_again = plant(&mut emu, SCRATCH + 0x200, "abc");

    assert_eq!(call(&mut emu, "strcmp", &[abc, abc_again]) as i64, 0);
    assert!((call(&mut emu, "strcmp", &[abc, abd]) as i64) < 0);
    assert!((call(&mut emu, "strcmp", &[abd, abc]) as i64) > 0);
    // The first two characters are equal, so a bounded compare says equal.
    assert_eq!(call(&mut emu, "strncmp", &[abc, abd, 2]) as i64, 0);
    assert!((call(&mut emu, "strncmp", &[abc, abd, 3]) as i64) < 0);

    let left = plant(&mut emu, SCRATCH + 0x300, "\x01\x02\x03");
    let right = plant(&mut emu, SCRATCH + 0x400, "\x01\x02\x04");
    assert_eq!(call(&mut emu, "memcmp", &[left, right, 2]) as i64, 0);
    assert!((call(&mut emu, "memcmp", &[left, right, 3]) as i64) < 0);
}

#[test]
fn the_string_searches_find_the_right_occurrence() {
    let mut emu = windows_emulator();
    let path = plant(&mut emu, SCRATCH, "a/b/c.txt");

    // A path split wants the *last* separator; a scheme check wants the first.
    assert_eq!(call(&mut emu, "strrchr", &[path, b'/' as u64]), path + 3);
    assert_eq!(call(&mut emu, "strchr", &[path, b'/' as u64]), path + 1);
    assert_eq!(call(&mut emu, "strchr", &[path, b'?' as u64]), 0);

    let needle = plant(&mut emu, SCRATCH + 0x100, "b/c");
    assert_eq!(call(&mut emu, "strstr", &[path, needle]), path + 2);
    let missing = plant(&mut emu, SCRATCH + 0x200, "zz");
    assert_eq!(call(&mut emu, "strstr", &[path, missing]), 0);
}

/// `strncpy` pads to `n` with NULs and does not terminate a truncated copy.
/// Both halves are what callers depend on, and both are easy to get wrong.
#[test]
fn strncpy_pads_and_truncates_the_way_c_does() {
    let mut emu = windows_emulator();
    let source = plant(&mut emu, SCRATCH, "ab");
    let destination = SCRATCH + 0x1000;
    let space = emu.state.ram_space();
    emu.state
        .write_space(space, destination, b"ZZZZZZ")
        .expect("poison");

    call(&mut emu, "strncpy", &[destination, source, 5]);
    assert_eq!(&read_back(&mut emu, destination, 6), b"ab\0\0\0Z");

    let long = plant(&mut emu, SCRATCH + 0x100, "abcdef");
    call(&mut emu, "strncpy", &[destination, long, 3]);
    assert_eq!(
        &read_back(&mut emu, destination, 4),
        b"abc\0",
        "a truncated strncpy must not write a terminator of its own"
    );
}

// ── The rest of the list ────────────────────────────────────────────────────

#[test]
fn getsysteminfo_describes_a_machine_this_emulator_can_be() {
    let mut emu = windows_emulator();
    let info = SCRATCH + 0x1000;
    call(&mut emu, "GetSystemInfo", &[info]);
    let bytes = read_back(&mut emu, info, 40);

    let page_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let processors = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
    assert_eq!(page_size, 0x1000);
    assert_eq!(
        processors, 1,
        "claiming more processors than this emulator runs invites a program \
         to start threads that never make progress"
    );
}

#[test]
fn errno_is_one_cell_the_program_can_read_back() {
    let mut emu = windows_emulator();
    let cell = call(&mut emu, "_errno", &[]);
    assert_ne!(cell, 0, "_errno answered with a null pointer");
    assert_eq!(
        call(&mut emu, "_errno", &[]),
        cell,
        "two calls named two different cells"
    );

    // A failed call has to be visible through it.
    let path = plant(&mut emu, SCRATCH, "nowhere.txt");
    call(&mut emu, "_access", &[path, 0]);
    let errno = u32::from_le_bytes(read_back(&mut emu, cell, 4).try_into().unwrap());
    assert_eq!(errno, 2, "ENOENT was not reported through errno");
}

/// A fixed clock, because a replay has to read the same one.
#[test]
fn time_is_the_same_on_every_run() {
    let mut emu = windows_emulator();
    let cell = SCRATCH + 0x1000;
    let returned = call(&mut emu, "_time64", &[cell]);
    assert_ne!(returned, 0);
    assert_eq!(
        u64::from_le_bytes(read_back(&mut emu, cell, 8).try_into().unwrap()),
        returned,
        "the value written through the pointer is not the value returned"
    );
    assert_eq!(call(&mut emu, "_time64", &[0]), returned);
}

// ── Standard input ──────────────────────────────────────────────────────────

/// With no mock, a console read is end-of-file -- never the host's stdin.
///
/// It used to read the host's. A sweep of real programs run as a background
/// job sat for twenty minutes at no CPU: lua and duktape had reached their
/// REPL prompt and were waiting on a pipe nobody would ever write to. If this
/// regresses under a harness whose stdin is open, the test does not fail; it
/// hangs -- and the flag assertion is there so a regression in the default
/// fails loudly instead.
#[test]
fn a_console_read_with_no_mock_is_end_of_file() {
    let mut emu = windows_emulator();
    assert!(
        !emu.host_stdin,
        "a library emulator must not read its host's stdin unless asked"
    );

    let handle = call(&mut emu, "GetStdHandle", &[(-10i64) as u64]);
    let buffer = SCRATCH + 0x1000;
    let count = SCRATCH + 0x1100;
    let ok = call(&mut emu, "ReadFile", &[handle, buffer, 64, count, 0]);

    assert_eq!(ok, 1, "end-of-file is a successful read of nothing");
    assert_eq!(
        u32::from_le_bytes(read_back(&mut emu, count, 4).try_into().unwrap()),
        0
    );
}

/// And a mock still arrives, through every path that reads stdin.
#[test]
fn mocked_input_reaches_readfile_and_fgets_alike() {
    let mut emu = windows_emulator();
    emu.seed_stdin(b"ab\ncd\n");

    let handle = call(&mut emu, "GetStdHandle", &[(-10i64) as u64]);
    let buffer = SCRATCH + 0x1000;
    let count = SCRATCH + 0x1100;
    call(&mut emu, "ReadFile", &[handle, buffer, 3, count, 0]);
    assert_eq!(&read_back(&mut emu, buffer, 3), b"ab\n");

    let stdin = call(&mut emu, "__acrt_iob_func", &[0]);
    let line = SCRATCH + 0x1200;
    assert_eq!(call(&mut emu, "fgets", &[line, 16, stdin]), line);
    assert_eq!(&read_back(&mut emu, line, 4), b"cd\n\0");
}

// ── The command line, in both encodings ─────────────────────────────────────

/// UTF-16 units from guest memory up to the NUL.
fn read_wide_text(emu: &mut Emulator, at: u64) -> String {
    let mut units = Vec::new();
    for i in 0..256u64 {
        let pair = read_back(emu, at + i * 2, 2);
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

fn read_pointer(emu: &mut Emulator, at: u64) -> u64 {
    u64::from_le_bytes(read_back(emu, at, 8).try_into().unwrap())
}

/// A `wmain` program reads `argv[0]` as UTF-16. Handed the narrow vector,
/// `"program.exe"` read as three CJK characters and whatever followed, and
/// libdeflate's gzip driver took that for a file operand.
#[test]
fn a_wmain_program_gets_utf16_argv() {
    let mut emu = windows_emulator();
    let argc_out = SCRATCH + 0x1000;
    let argv_out = SCRATCH + 0x1010;
    let envp_out = SCRATCH + 0x1020;

    call(
        &mut emu,
        "__wgetmainargs",
        &[argc_out, argv_out, envp_out, 0, 0],
    );

    let argv = read_pointer(&mut emu, argv_out);
    let argv0 = read_pointer(&mut emu, argv);
    assert_eq!(read_wide_text(&mut emu, argv0), "program.exe");
    assert_eq!(
        read_pointer(&mut emu, argv + 8),
        0,
        "argv is not terminated"
    );
    let envp = read_pointer(&mut emu, envp_out);
    assert_eq!(read_pointer(&mut emu, envp), 0, "environment is not empty");

    // And the `__p___wargv` accessor names the same vector.
    let cell = call(&mut emu, "__p___wargv", &[]);
    assert_eq!(read_pointer(&mut emu, cell), argv);
}

/// The narrow side is unchanged by it.
#[test]
fn a_main_program_still_gets_narrow_argv() {
    let mut emu = windows_emulator();
    let argc_out = SCRATCH + 0x1000;
    let argv_out = SCRATCH + 0x1010;
    let envp_out = SCRATCH + 0x1020;

    call(
        &mut emu,
        "__getmainargs",
        &[argc_out, argv_out, envp_out, 0, 0],
    );

    let argv = read_pointer(&mut emu, argv_out);
    let argv0 = read_pointer(&mut emu, argv);
    assert_eq!(&read_back(&mut emu, argv0, 12), b"program.exe\0");
    let wide_cell = call(&mut emu, "__p___wargv", &[]);
    assert_ne!(
        argv,
        read_pointer(&mut emu, wide_cell),
        "the narrow and wide vectors are the same storage again"
    );
}

// ── stat and fstat ──────────────────────────────────────────────────────────

fn stat_mode(emu: &mut Emulator, buffer: u64) -> u16 {
    u16::from_le_bytes(read_back(emu, buffer + 6, 2).try_into().unwrap())
}

/// An open descriptor exists. `_fstat64` used to share `_stat64`'s "nothing
/// exists", so stdin failed it -- and libdeflate's gzip driver stats stdin
/// before reading it.
#[test]
fn fstat_of_a_standard_stream_is_a_character_device() {
    let mut emu = windows_emulator();
    let buffer = SCRATCH + 0x1000;
    for fd in 0..3u64 {
        assert_eq!(call(&mut emu, "_fstat64", &[fd, buffer]), 0, "fd {fd}");
        assert_eq!(stat_mode(&mut emu, buffer) & 0xF000, 0x2000, "fd {fd}");
    }
    assert_eq!(
        call(&mut emu, "_fstat64", &[42, buffer]) as i64,
        -1,
        "a descriptor that was never opened must not stat"
    );
}

#[test]
fn stat_of_a_seeded_file_reports_its_size_in_both_layouts() {
    let mut emu = windows_emulator();
    emu.vfs.seed_path("data.bin", b"hello file".to_vec());
    let path = plant(&mut emu, SCRATCH, "data.bin");
    let buffer = SCRATCH + 0x1000;

    assert_eq!(call(&mut emu, "_stat64", &[path, buffer]), 0);
    assert_eq!(stat_mode(&mut emu, buffer) & 0xF000, 0x8000);
    assert_eq!(
        u64::from_le_bytes(read_back(&mut emu, buffer + 24, 8).try_into().unwrap()),
        10,
        "_stat64 carries a 64-bit size at offset 24"
    );

    assert_eq!(call(&mut emu, "_stat64i32", &[path, buffer]), 0);
    assert_eq!(
        u32::from_le_bytes(read_back(&mut emu, buffer + 20, 4).try_into().unwrap()),
        10,
        "_stat64i32 carries a 32-bit size at offset 20"
    );

    let missing = plant(&mut emu, SCRATCH + 0x100, "nowhere.bin");
    assert_eq!(call(&mut emu, "_stat64", &[missing, buffer]) as i64, -1);
}

/// A `-municode` program prints its own name with `%ls`.
#[test]
fn printf_reads_ls_as_a_wide_string() {
    let mut emu = windows_emulator();
    let stderr = call(&mut emu, "__acrt_iob_func", &[2]);
    let wide: Vec<u8> = "program.exe\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let space = emu.state.ram_space();
    emu.state
        .write_space(space, SCRATCH + 0x400, &wide)
        .expect("wide text");

    vfprintf(
        &mut emu,
        stderr,
        "%ls: %S|%lc\n",
        &[SCRATCH + 0x400, SCRATCH + 0x400, 0x4E2D],
    );

    assert!(
        stream_text(&emu, 2).contains("program.exe: program.exe|中"),
        "stderr holds {:?}",
        stream_text(&emu, 2)
    );
}
