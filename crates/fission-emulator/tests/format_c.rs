//! The C formatting engine, checked against the one every program was
//! written for.
//!
//! Every expectation in this file was produced by compiling the same format
//! and arguments with the host's C compiler and running it. A formatter is
//! exactly as useful as its agreement with the libc a program was linked
//! against, and a hand-written expectation only proves the formatter agrees
//! with whoever wrote the test.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::BareMetalEnv;
use fission_emulator::os::format::{VaList, format_c};
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn emulator() -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    Emulator::new(
        MachineState::new(),
        binary,
        sleigh,
        arch,
        Box::new(BareMetalEnv::new()),
    )
    .expect("emulator")
}

/// One argument, the way a variadic call leaves it: an eight-byte slot.
enum Arg {
    Word(u64),
    Double(f64),
    Text(&'static str),
}

/// Lay the arguments out in guest memory and format through a `va_list`,
/// which is how `__stdio_common_vfprintf` receives them.
fn formatted(emu: &mut Emulator, fmt: &str, args: &[Arg]) -> String {
    let page = emu.state.page_map.mmap_anon(0x2000, prot::RW);
    assert_ne!(page, 0, "no guest memory for the argument list");
    let space = emu.state.ram_space();

    // Strings first, above the slot array, so a pointer has something to
    // point at.
    let mut text_cursor = page + 0x1000;
    let mut slots = Vec::new();
    for arg in args {
        slots.push(match arg {
            Arg::Word(value) => *value,
            Arg::Double(value) => value.to_bits(),
            Arg::Text(text) => {
                let address = text_cursor;
                let mut bytes = text.as_bytes().to_vec();
                bytes.push(0);
                emu.state
                    .write_space(space, address, &bytes)
                    .expect("write string");
                text_cursor += bytes.len() as u64;
                address
            }
        });
    }
    for (index, slot) in slots.iter().enumerate() {
        emu.state
            .write_space(space, page + index as u64 * 8, &slot.to_le_bytes())
            .expect("write slot");
    }

    let mut va = VaList::new(page, true);
    format_c(emu, fmt, &mut va).expect("format")
}

#[test]
fn integers_match_the_host_libc() {
    let mut emu = emulator();
    let cases: &[(&str, &[Arg], &str)] = &[
        ("[%d]", &[Arg::Word(-42i64 as u64)], "[-42]"),
        ("[%5d]", &[Arg::Word(42)], "[   42]"),
        ("[%-5d|]", &[Arg::Word(42)], "[42   |]"),
        ("[%05d]", &[Arg::Word(42)], "[00042]"),
        ("[%+d]", &[Arg::Word(42)], "[+42]"),
        ("[% d]", &[Arg::Word(42)], "[ 42]"),
        ("[%.5d]", &[Arg::Word(42)], "[00042]"),
        ("[%8.3d]", &[Arg::Word(42)], "[     042]"),
        ("[%x]", &[Arg::Word(48879)], "[beef]"),
        ("[%#x]", &[Arg::Word(48879)], "[0xbeef]"),
        ("[%#X]", &[Arg::Word(48879)], "[0XBEEF]"),
        ("[%08x]", &[Arg::Word(48879)], "[0000beef]"),
        ("[%o]", &[Arg::Word(8)], "[10]"),
        ("[%#o]", &[Arg::Word(8)], "[010]"),
        ("[%u]", &[Arg::Word(4294967295)], "[4294967295]"),
        ("[%lu]", &[Arg::Word(u64::MAX)], "[18446744073709551615]"),
        (
            "[%lld]",
            &[Arg::Word(-9223372036854775807i64 as u64)],
            "[-9223372036854775807]",
        ),
        ("[%zu]", &[Arg::Word(1234)], "[1234]"),
        ("[%hhd]", &[Arg::Word(0xFF)], "[-1]"),
        ("[%hd]", &[Arg::Word(0xFFFF)], "[-1]"),
        ("[%*d]", &[Arg::Word(6), Arg::Word(42)], "[    42]"),
        ("[%%]", &[], "[%]"),
    ];
    for (fmt, args, want) in cases {
        assert_eq!(&formatted(&mut emu, fmt, args), want, "format {fmt}");
    }
}

#[test]
fn strings_and_characters_match_the_host_libc() {
    let mut emu = emulator();
    let cases: &[(&str, &[Arg], &str)] = &[
        ("[%s]", &[Arg::Text("hello")], "[hello]"),
        ("[%10s|]", &[Arg::Text("hi")], "[        hi|]"),
        ("[%-10s|]", &[Arg::Text("hi")], "[hi        |]"),
        ("[%.2s]", &[Arg::Text("hello")], "[he]"),
        ("[%c]", &[Arg::Word(b'A' as u64)], "[A]"),
    ];
    for (fmt, args, want) in cases {
        assert_eq!(&formatted(&mut emu, fmt, args), want, "format {fmt}");
    }
}

#[test]
fn floating_point_matches_the_host_libc() {
    let mut emu = emulator();
    const PI: f64 = std::f64::consts::PI;
    let cases: &[(&str, &[Arg], &str)] = &[
        ("[%f]", &[Arg::Double(PI)], "[3.141593]"),
        ("[%.2f]", &[Arg::Double(PI)], "[3.14]"),
        ("[%10.2f]", &[Arg::Double(PI)], "[      3.14]"),
        ("[%-10.2f|]", &[Arg::Double(PI)], "[3.14      |]"),
        ("[%e]", &[Arg::Double(31415.9265)], "[3.141593e+04]"),
        ("[%E]", &[Arg::Double(0.00031415)], "[3.141500E-04]"),
        ("[%g]", &[Arg::Double(100000.0)], "[100000]"),
        ("[%g]", &[Arg::Double(1000000.0)], "[1e+06]"),
        ("[%g]", &[Arg::Double(0.0001)], "[0.0001]"),
        ("[%g]", &[Arg::Double(0.00001)], "[1e-05]"),
        ("[%.3g]", &[Arg::Double(PI)], "[3.14]"),
        ("[%f]", &[Arg::Double(0.0)], "[0.000000]"),
        ("[%g]", &[Arg::Double(0.0)], "[0]"),
        ("[%e]", &[Arg::Double(0.0)], "[0.000000e+00]"),
        ("[%.*f]", &[Arg::Word(3), Arg::Double(PI)], "[3.142]"),
    ];
    for (fmt, args, want) in cases {
        assert_eq!(&formatted(&mut emu, fmt, args), want, "format {fmt}");
    }
}

/// A conversion nobody recognises must not eat an argument: every later
/// conversion would then read the wrong slot, which prints plausible garbage
/// rather than an obvious error.
#[test]
fn an_unknown_conversion_consumes_nothing() {
    let mut emu = emulator();
    let out = formatted(&mut emu, "%q %d", &[Arg::Word(7)]);
    assert_eq!(out, "%q 7");
}

/// The arguments come out of guest memory in order, so a format that reads
/// more slots than were written must not read the earlier ones again.
#[test]
fn the_argument_list_advances_once_per_conversion() {
    let mut emu = emulator();
    let out = formatted(
        &mut emu,
        "%d %d %d",
        &[Arg::Word(1), Arg::Word(2), Arg::Word(3)],
    );
    assert_eq!(out, "1 2 3");
}
