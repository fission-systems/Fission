//! `RDTSC` answers, and answers something a program can time itself with.
//!
//! The x86 specification is `tmp:8 = rdtsc(); EDX = tmp(4); EAX = tmp(0)` --
//! a value, not a pointer. Both OS layers had an arm naming `rdtsc` that only
//! logged and never wrote a result, and that arm sat *above* the processor
//! handler, so it shadowed it. Nothing reported a miss, because the arm
//! matched; the guest simply read whatever the previous CALLOTHER had left in
//! the result slot and called it a timestamp.
//!
//! That is the failure this file exists to keep out: two reads returning the
//! same count (a loop dividing by the delta divides by zero), or a count that
//! goes backwards because the last CALLOTHER happened to be larger.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{BareMetalEnv, LinuxEnv, OsEnvironment, WindowsEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn emulator(os: Box<dyn OsEnvironment>) -> Emulator {
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
    Emulator::new(MachineState::new(), binary, sleigh, arch, os).expect("emulator")
}

/// Ask the way the p-code evaluator does: through the environment, which is
/// where the shadowing arm lived.
fn read_tsc(emu: &mut Emulator, os: &dyn OsEnvironment) -> u64 {
    // A sentinel, so "answered" cannot be confused with "left alone". This is
    // exactly what the bug looked like from the guest's side.
    emu.callother_result = 0xDEAD_BEEF_DEAD_BEEF;
    os.dispatch_userop(emu, "rdtsc", &[], 8).expect("dispatch");
    let value = emu.callother_result;
    assert_ne!(
        value, 0xDEAD_BEEF_DEAD_BEEF,
        "rdtsc left the previous CALLOTHER's result in place"
    );
    value
}

#[test]
fn two_reads_with_nothing_between_them_still_differ() {
    let os = BareMetalEnv::new();
    let mut emu = emulator(Box::new(BareMetalEnv::new()));
    let first = read_tsc(&mut emu, &os);
    let second = read_tsc(&mut emu, &os);
    assert!(
        second > first,
        "a program measuring an interval divides by the delta: {first} then {second}"
    );
}

#[test]
fn the_count_follows_execution() {
    let os = BareMetalEnv::new();
    let mut emu = emulator(Box::new(BareMetalEnv::new()));
    let before = read_tsc(&mut emu, &os);
    emu.inst_count += 1_000;
    let after = read_tsc(&mut emu, &os);
    assert!(
        after - before >= 1_000,
        "a thousand instructions cost fewer than a thousand cycles: {before} then {after}"
    );
}

/// Both OS layers had the shadowing arm, so both are checked. `cpuid` was
/// named in the same arm and is checked with it.
#[test]
fn neither_os_layer_shadows_the_processor_handler() {
    let layers: [(&str, fn() -> Box<dyn OsEnvironment>); 2] = [
        ("windows", || Box::new(WindowsEnv::new())),
        ("linux", || Box::new(LinuxEnv::new())),
    ];
    for (what, make) in layers {
        let os = make();
        let mut emu = emulator(make());

        let first = read_tsc(&mut emu, os.as_ref());
        let second = read_tsc(&mut emu, os.as_ref());
        assert!(second > first, "{what}: rdtsc did not advance");

        emu.callother_result = 0xDEAD_BEEF_DEAD_BEEF;
        os.dispatch_userop(&mut emu, "cpuid_basic_info", &[0], 8)
            .expect("dispatch");
        assert_ne!(
            emu.callother_result, 0xDEAD_BEEF_DEAD_BEEF,
            "{what}: cpuid was shadowed by an arm that only logs"
        );
        assert_ne!(
            emu.callother_result, 0,
            "{what}: cpuid answered with a null pointer"
        );
    }
}
