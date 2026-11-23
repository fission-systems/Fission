//! A Windows PE reaches `main` and exits.
//!
//! The fixtures are MinGW-built PEs compiled from `testdata/src/
//! mingw_control_flow.c`. Running a foreign OS's binaries is the point of the
//! emulator; running someone else's samples is not what this test is for.
//!
//! They used to come from a local dev corpus and skipped when it was absent --
//! which was everywhere, CI included, so these tests had not been running.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{BehaviorEvent, BehaviorLog, Coverage};
use fission_emulator::os::WindowsEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

/// The `[start, end)` of a function, from the PE's COFF symbols.
fn function_range(binary: &LoadedBinary, name: &str) -> (u64, u64) {
    let function = binary
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} in the symbol table"));
    assert!(function.size > 0, "{name} has no size");
    (function.address, function.address + function.size)
}

/// A dev-corpus program, for the one test that still needs one.
fn corpus(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries")
        .join(name);
    assert!(
        path.is_file(),
        "dev corpus program absent: {}",
        path.display()
    );
    path
}

#[test]
fn a_mingw_pe_runs_its_crt_and_reaches_main() {
    let path = testdata("x64_mingw_control_flow.exe");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let functions =
        ["main", "checksum", "classify_range"].map(|name| function_range(&binary, name));
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
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(WindowsEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(500_000));
    emu.apply_windows_image(info).expect("image");
    emu.add_observer(Box::new(BehaviorLog::new()));
    emu.add_observer(Box::new(Coverage::new()));

    emu.run().expect("run");

    // Ending on the instruction budget would mean it was still going round the
    // CRT, which is exactly what it used to do.
    assert!(
        emu.halt_requested,
        "the process did not exit: {}",
        emu.metrics.summary_line()
    );
    assert_ne!(
        emu.metrics.exit_reason.as_deref(),
        Some("max_inst"),
        "hit the instruction budget rather than exiting"
    );

    // Every API the CRT reaches for must be answered. A miss here is a stub
    // that has to be written, and the report names it -- that is how each of
    // these got written in the first place.
    assert!(
        emu.metrics.hle_misses.is_empty(),
        "unimplemented Windows APIs: {:?}",
        emu.metrics.hle_misses
    );

    let observers = emu.take_observers();
    let log = observers
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<BehaviorLog>())
        .expect("behaviour log");
    let calls: Vec<&str> = log
        .events
        .iter()
        .filter_map(|e| match e {
            BehaviorEvent::Hle { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();

    // mingw's start-up in order: initialiser tables, then the command line,
    // then `main`, then exit. The command line comes from `__getmainargs`
    // against msvcrt and `_configure_narrow_argv` against the UCRT, which is
    // what this fixture's toolchain links by default.
    for expected in ["_initterm", "exit"] {
        assert!(
            calls.contains(&expected),
            "{expected} never ran; calls were {calls:?}"
        );
    }
    assert!(
        calls
            .iter()
            .any(|c| *c == "__getmainargs" || *c == "_configure_narrow_argv"),
        "the command line was never built; calls were {calls:?}"
    );
    let initterm = calls.iter().position(|c| *c == "_initterm").unwrap();
    let exit = calls.iter().position(|c| *c == "exit").unwrap();
    assert!(initterm < exit, "start-up order lost: {calls:?}");

    // And the program's own code ran, not just the CRT around it. `_initterm`
    // has to actually call the constructors it walks, and `main` has to be
    // reached with a command line it can read.
    let coverage = observers
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<Coverage>())
        .expect("coverage");
    let executed = coverage.executed_instructions();
    let ran = |lo: u64, hi: u64| executed.iter().any(|pc| (lo..hi).contains(pc));
    for ((lo, hi), name) in functions
        .into_iter()
        .zip(["main", "checksum", "classify_range"])
    {
        assert!(ran(lo, hi), "{name} did not run");
    }
}

/// The same, 32 bits.
///
/// A 32-bit process is not a smaller 64-bit one: the trampolines have to fit
/// in four-byte IAT slots, the TEB hangs off FS rather than GS, `_fmode` and
/// friends arrive as *data* imports the program writes through, and the PE
/// headers have to be mapped because the relocator reads them. All 44 of the
/// dev corpus's 32-bit PEs stopped before their first instruction until they
/// were, so this pins the shape rather than one API.
#[test]
fn a_32_bit_mingw_pe_runs_its_crt_and_exits() {
    let path = testdata("x86_mingw_control_flow.exe");

    let binary = LoadedBinary::from_file(&path).expect("load");
    assert!(!binary.inner().is_64bit, "fixture should be a 32-bit PE");
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
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(WindowsEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(500_000));
    emu.apply_windows_image(info).expect("image");

    emu.run().expect("run");

    assert!(
        emu.halt_requested,
        "the process did not exit: {}",
        emu.metrics.summary_line()
    );
    assert_ne!(
        emu.metrics.exit_reason.as_deref(),
        Some("max_inst"),
        "hit the instruction budget rather than exiting"
    );
    assert!(
        emu.metrics.hle_misses.is_empty(),
        "unimplemented Windows APIs: {:?}",
        emu.metrics.hle_misses
    );
}

/// A cgo Go program: the runtime starts threads before `main` (`_beginthread`
/// through cgo), parks the main thread on an event they signal, and sleeps
/// its monitor thread by waiting on the process handle. With threads that
/// never ran, the main thread waited forever; before that, five start-up
/// queries aborted it outright.
///
/// Still reads the local dev corpus, so it is opt-in: a cgo Windows build is
/// a multi-megabyte fixture this repository does not carry yet.
#[test]
#[ignore = "needs a cgo Go program from the local dev corpus; run with --ignored"]
fn a_cgo_go_program_starts_its_threads_and_runs_main() {
    let path = corpus("go/patterns_go_default.exe");

    let binary = LoadedBinary::from_file(&path).expect("load");
    let main = {
        let inner = binary.inner();
        let index = *inner
            .function_name_index
            .get("main.main")
            .expect("main.main in the symbol table");
        inner.functions[index].address
    };
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
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(WindowsEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(5_000_000));
    emu.apply_windows_image(info).expect("image");
    emu.add_observer(Box::new(Coverage::new()));

    emu.run().expect("run");

    assert!(
        emu.halt_requested,
        "the process did not exit: {}",
        emu.metrics.summary_line()
    );
    // `main` computes and discards; a Go runtime abort exits with 2.
    assert_eq!(emu.exit_code, Some(0), "{}", emu.metrics.summary_line());
    let observers = emu.take_observers();
    let coverage = observers
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<Coverage>())
        .expect("coverage");
    assert!(
        coverage.executed_instructions().contains(&main),
        "main.main at 0x{main:X} never ran"
    );
}
