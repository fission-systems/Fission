//! Windows guest threads, events and waits (`os::windows::threads`).
//!
//! A switch between guest threads happens inside an API -- the one that
//! blocks -- and is carried out by the run loop from what the API leaves
//! behind: `HleResult::Schedule`, the next thread's registers, its TEB in the
//! segment base, and its resume address in `pc_override`. These tests drive
//! the APIs the way the dispatcher does and check exactly that, so each one
//! names the step that went wrong rather than a program that hung.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::env::HleResult;
use fission_emulator::os::{BareMetalEnv, WindowsEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

const INFINITE: u64 = 0xFFFF_FFFF;
const WAIT_TIMEOUT: u64 = 0x102;
/// Where the "main thread" returns to from a blocking call in these tests.
const MAIN_RETURN: u64 = 0x1400_0111;
/// A start routine; nothing executes there, the tests only check the switch.
const THREAD_START: u64 = 0x1400_0222;

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
    let mut emu =
        Emulator::new(state, binary, sleigh, arch, Box::new(WindowsEnv::new())).expect("emulator");
    emu.apply_windows_image(info).expect("image");
    emu
}

/// Call one API as the dispatcher does: arguments where the convention puts
/// them, a return address on the stack. Returns what the API decided.
fn dispatch(emu: &mut Emulator, name: &str, args: &[u64], return_to: u64) -> HleResult {
    let registers = emu.arch.cc.arg_regs().len();
    assert!(args.len() <= registers, "register arguments only");
    for (index, value) in args.iter().enumerate() {
        let register = emu.arch.cc.arg_regs()[index];
        emu.write_register_u64(register, *value).expect("arg");
    }
    // A call pushed the return address.
    let sp = emu.read_register_u64(emu.arch.sp_reg).expect("sp") - 8;
    emu.write_register_u64(emu.arch.sp_reg, sp).expect("sp");
    let space = emu.state.ram_space();
    emu.state
        .write_space(space, sp, &return_to.to_le_bytes())
        .expect("return address");

    let os = std::mem::replace(&mut emu.os, Box::new(BareMetalEnv::new()));
    let result = os.dispatch_hle(emu, name).expect("dispatch");
    emu.os = os;
    if matches!(result, HleResult::Continue) {
        emu.simulate_return_for(name).expect("return");
    }
    result
}

fn ret(emu: &mut Emulator) -> u64 {
    emu.read_register_u64(emu.arch.cc.return_reg())
        .expect("return value")
}

/// After a `Schedule`, the run loop resumes at `pc_override`.
fn resumes_at(emu: &mut Emulator) -> u64 {
    let pc = emu
        .pc_override
        .take()
        .expect("a switch leaves a resume address");
    emu.pc = pc;
    pc
}

#[test]
fn a_wait_runs_another_thread_and_its_set_event_wakes_the_waiter() {
    let mut emu = windows_emulator();
    let main_teb = emu.gs_base;

    let event = {
        dispatch(&mut emu, "CreateEventA", &[0, 0, 0, 0], MAIN_RETURN);
        ret(&mut emu)
    };
    assert_ne!(event, 0, "an event handle");
    dispatch(
        &mut emu,
        "CreateThread",
        &[0, 0, THREAD_START, 0x55],
        MAIN_RETURN,
    );
    let thread = ret(&mut emu);
    assert_ne!(thread, 0, "a thread handle");

    // The main thread waits; the new one runs, with its argument and its
    // own TEB.
    let result = dispatch(
        &mut emu,
        "WaitForSingleObject",
        &[event, INFINITE],
        MAIN_RETURN,
    );
    assert!(matches!(result, HleResult::Schedule), "the wait blocks");
    assert_eq!(resumes_at(&mut emu), THREAD_START);
    assert_eq!(emu.read_register_u64("RCX").unwrap(), 0x55, "its parameter");
    assert_ne!(emu.gs_base, main_teb, "its own TEB");
    dispatch(&mut emu, "GetCurrentThreadId", &[], 0);
    assert_ne!(ret(&mut emu), 1000, "a thread id of its own");

    // It signals the event and carries on; then it exits.
    let result = dispatch(&mut emu, "SetEvent", &[event], 0x1400_0333);
    assert!(matches!(result, HleResult::Continue));
    assert_eq!(ret(&mut emu), 1);
    let result = dispatch(&mut emu, "ExitThread", &[7], 0);
    assert!(matches!(result, HleResult::Schedule));

    // The waiter is back, from its call, with WAIT_OBJECT_0.
    assert_eq!(resumes_at(&mut emu), MAIN_RETURN);
    assert_eq!(ret(&mut emu), 0, "WAIT_OBJECT_0");
    assert_eq!(emu.gs_base, main_teb, "the main TEB again");

    // The thread's handle is signalled and carries its exit code.
    let result = dispatch(&mut emu, "WaitForSingleObject", &[thread, 0], MAIN_RETURN);
    assert!(matches!(result, HleResult::Continue));
    assert_eq!(ret(&mut emu), 0, "an exited thread is signalled");
    let out = emu.read_register_u64("RSP").unwrap() - 0x100;
    dispatch(&mut emu, "GetExitCodeThread", &[thread, out], MAIN_RETURN);
    let space = emu.state.ram_space();
    let code = emu.state.read_space(space, out, 4).unwrap();
    assert_eq!(u32::from_le_bytes(code.try_into().unwrap()), 7);
}

/// The Go runtime sleeps with `WaitForSingleObject(INVALID_HANDLE_VALUE, ms)`:
/// the process handle, which is signalled only when the process has exited.
/// Answering at once made its monitor thread spin forever.
#[test]
fn a_wait_on_the_process_handle_times_out_at_its_deadline() {
    let mut emu = windows_emulator();
    let before = emu.tick_count;
    let result = dispatch(
        &mut emu,
        "WaitForSingleObject",
        &[u64::MAX, 10],
        MAIN_RETURN,
    );
    assert!(
        matches!(result, HleResult::Schedule),
        "the wait blocks rather than answering at once"
    );
    assert_eq!(resumes_at(&mut emu), MAIN_RETURN);
    assert_eq!(ret(&mut emu), WAIT_TIMEOUT);
    assert_eq!(
        emu.tick_count - before,
        10,
        "the clock reached the deadline"
    );
}

/// With every thread blocked, time moves to the earliest deadline: a timed
/// wait expires before a longer sleep elsewhere.
#[test]
fn the_earliest_deadline_expires_first() {
    let mut emu = windows_emulator();
    dispatch(&mut emu, "CreateEventA", &[0, 0, 0, 0], MAIN_RETURN);
    let event = ret(&mut emu);
    dispatch(
        &mut emu,
        "CreateThread",
        &[0, 0, THREAD_START, 0],
        MAIN_RETURN,
    );
    let start = emu.tick_count;

    let result = dispatch(&mut emu, "WaitForSingleObject", &[event, 50], MAIN_RETURN);
    assert!(matches!(result, HleResult::Schedule));
    assert_eq!(resumes_at(&mut emu), THREAD_START);

    let result = dispatch(&mut emu, "Sleep", &[100], 0x1400_0444);
    assert!(matches!(result, HleResult::Schedule));
    assert_eq!(
        resumes_at(&mut emu),
        MAIN_RETURN,
        "the 50 ms wait goes first"
    );
    assert_eq!(ret(&mut emu), WAIT_TIMEOUT);
    assert_eq!(emu.tick_count - start, 50);
}

/// A program reads its TLS through the segment register as well as through
/// the API, so each thread's slots are in its own TEB.
#[test]
fn tls_slots_belong_to_each_thread() {
    let mut emu = windows_emulator();
    dispatch(&mut emu, "TlsAlloc", &[], MAIN_RETURN);
    let index = ret(&mut emu);
    assert!(index < 64);
    dispatch(&mut emu, "TlsSetValue", &[index, 0xAAAA], MAIN_RETURN);
    assert_eq!(ret(&mut emu), 1);

    // In the TEB, where `gs:[0x1480 + index * 8]` reads it.
    let space = emu.state.ram_space();
    let slot = emu.gs_base + 0x1480 + index * 8;
    let stored = emu.state.read_space(space, slot, 8).unwrap();
    assert_eq!(u64::from_le_bytes(stored.try_into().unwrap()), 0xAAAA);
    dispatch(&mut emu, "TlsGetValue", &[index], MAIN_RETURN);
    assert_eq!(ret(&mut emu), 0xAAAA);

    // Another thread sees its own, empty slot.
    dispatch(
        &mut emu,
        "CreateThread",
        &[0, 0, THREAD_START, 0],
        MAIN_RETURN,
    );
    dispatch(&mut emu, "SwitchToThread", &[], MAIN_RETURN);
    assert_eq!(resumes_at(&mut emu), THREAD_START);
    dispatch(&mut emu, "TlsGetValue", &[index], 0x1400_0555);
    assert_eq!(ret(&mut emu), 0);
}

/// A wait nothing can satisfy, with no timeout and no other thread, is a
/// deadlock -- reported as one rather than answered with a made-up result.
#[test]
fn an_infinite_wait_with_nothing_to_wake_it_is_a_deadlock() {
    let mut emu = windows_emulator();
    dispatch(&mut emu, "CreateEventA", &[0, 0, 0, 0], MAIN_RETURN);
    let event = ret(&mut emu);
    let result = dispatch(
        &mut emu,
        "WaitForSingleObject",
        &[event, INFINITE],
        MAIN_RETURN,
    );
    assert!(matches!(result, HleResult::Deadlock));
}

/// An auto-reset event wakes one waiter and resets; a second wait blocks.
#[test]
fn an_auto_reset_event_satisfies_one_wait() {
    let mut emu = windows_emulator();
    // Initially signalled, auto-reset.
    dispatch(&mut emu, "CreateEventA", &[0, 0, 1, 0], MAIN_RETURN);
    let event = ret(&mut emu);
    let result = dispatch(&mut emu, "WaitForSingleObject", &[event, 0], MAIN_RETURN);
    assert!(matches!(result, HleResult::Continue));
    assert_eq!(ret(&mut emu), 0, "the first wait takes it");
    dispatch(&mut emu, "WaitForSingleObject", &[event, 0], MAIN_RETURN);
    assert_eq!(ret(&mut emu), WAIT_TIMEOUT, "and it reset");
}
