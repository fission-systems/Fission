//! A Windows PE reaches `main` and exits.
//!
//! The fixture is a mingw-built PE from this repository's own dev corpus,
//! compiled from source we control. Running a foreign OS's binaries is the
//! point of the emulator; running someone else's samples is not what this
//! test is for.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{BehaviorEvent, BehaviorLog, Coverage};
use fission_emulator::os::WindowsEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture() -> Option<PathBuf> {
    corpus("c/control_flow_gcc_O0.exe")
}

fn corpus(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries")
        .join(name);
    path.is_file().then_some(path)
}

#[test]
fn a_mingw_pe_runs_its_crt_and_reaches_main() {
    // The corpus lives outside this repository, like the other corpus tests.
    let Some(path) = fixture() else {
        eprintln!("skipping: dev corpus not present");
        return;
    };

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
    // then `main`, then exit.
    for expected in ["_initterm", "__getmainargs", "exit"] {
        assert!(
            calls.contains(&expected),
            "{expected} never ran; calls were {calls:?}"
        );
    }
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
    assert!(ran(0x1400016AC, 0x14000172D), "main did not run");
    assert!(ran(0x1400015B4, 0x14000160B), "checksum did not run");
    assert!(ran(0x14000160B, 0x140001660), "classify_range did not run");
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
    let Some(path) = corpus("control_flow_gcc-m32_O0.exe") else {
        eprintln!("skipping: dev corpus not present");
        return;
    };

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
