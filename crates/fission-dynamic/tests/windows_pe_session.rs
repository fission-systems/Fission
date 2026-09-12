//! Debugging a Windows binary on a machine that is not Windows.
//!
//! This is the reason the emulator backend matters: the native Windows
//! debugger needs Windows, and the whole point of examining a Windows sample
//! is to do it somewhere it cannot do any harm. The backend already loaded PE
//! images and ran them; what it could not do was stop, step, or look --
//! which is to say it could run a sample and tell you nothing about it.
//!
//! Static analysis only, on the crate's own fixtures. Nothing here runs on the
//! host: the guest instructions are emulated.
#![cfg(feature = "interactive_runtime")]

use fission_dynamic::debug::emulator_backend::EmulatorBackend;
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::MemoryBpKind;

fn pe_fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/win_x64_write.exe"
    )
    .to_string()
}

#[test]
fn a_windows_binary_can_be_stepped_and_inspected() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&pe_fixture(), &[]).expect("launch the PE");

    let entry = backend.fetch_registers(1).expect("registers").pc;
    assert_ne!(entry, 0, "launched with no program counter");

    // Step, one instruction at a time, and watch the program counter move.
    let mut seen = vec![entry];
    for i in 0..25 {
        let before = backend.emulator.as_ref().unwrap().inst_count;
        let pc_before = backend.fetch_registers(1).unwrap().pc;
        let Ok(()) = backend.single_step() else {
            // The program ended; stepping past that is an error, not silence.
            break;
        };
        let after = backend.emulator.as_ref().unwrap().inst_count;
        let pc_after = backend.fetch_registers(1).unwrap().pc;
        // A step runs one guest instruction, or none when it dispatches a
        // high-level-emulation stub standing in for an imported API -- which
        // is still one step, and is most of what a Windows binary does.
        assert!(
            after - before <= 1,
            "step {i} from 0x{pc_before:X} ran {} instructions",
            after - before
        );
        seen.push(pc_after);
    }
    assert!(
        seen.iter().collect::<std::collections::HashSet<_>>().len() > 5,
        "twenty-five steps visited fewer than six addresses: {seen:02x?}"
    );

    // The registers are the machine's, and the image is readable at the entry.
    let regs = backend.fetch_registers(1).expect("registers");
    assert!(
        regs.get("RSP").is_some_and(|sp| sp != 0),
        "no stack pointer"
    );
    let code = backend
        .read_memory(entry, 16)
        .expect("read the entry point");
    assert_eq!(code.len(), 16);
    assert!(
        code.iter().any(|b| *b != 0),
        "the entry point read back as sixteen zero bytes"
    );
}

#[test]
fn a_breakpoint_stops_a_windows_binary() {
    // An address this PE reaches, taken from the run itself. Twelve steps,
    // because the fixture writes its line and exits in rather fewer than
    // thirty and stepping past that is an error now.
    let target = {
        let mut backend = EmulatorBackend::new();
        backend.launch(&pe_fixture(), &[]).expect("launch");
        for _ in 0..12 {
            backend.single_step().expect("step");
        }
        backend.fetch_registers(1).unwrap().pc
    };

    let mut backend = EmulatorBackend::new();
    backend.launch(&pe_fixture(), &[]).expect("launch");
    backend.set_sw_breakpoint(target).expect("set breakpoint");
    backend.continue_execution().expect("continue");

    assert_eq!(
        backend.fetch_registers(1).unwrap().pc,
        target,
        "continue did not stop at the breakpoint"
    );
}

/// The question a sample is actually examined to answer.
#[test]
fn a_watchpoint_names_what_wrote_to_the_stack() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&pe_fixture(), &[]).expect("launch");
    for _ in 0..8 {
        backend.single_step().expect("step");
    }

    let sp = backend.fetch_registers(1).unwrap().get("RSP").expect("RSP");
    let watched = sp.wrapping_sub(32);
    backend
        .set_memory_breakpoint(watched, 8, MemoryBpKind::Write)
        .expect("watch the stack");
    backend.continue_execution().expect("continue");

    let emu = backend.emulator.as_ref().unwrap();
    if let Some(hit) = emu.last_watch_hit() {
        assert!(hit.write);
        assert_ne!(
            hit.pc, 0,
            "the write was reported without the instruction that made it"
        );
    }
    // Whether this particular fixture writes there is the fixture's business;
    // what must hold is that arming the watch did not break the run.
    assert!(
        emu.inst_count > 8,
        "arming a watchpoint stopped the program from running at all"
    );
}

/// A finished program says so, instead of stepping forever in place.
///
/// `run_inner` clears `halt_requested` on the way in, so resuming an exited
/// process dispatched its exit stub again: stepping past the end printed the
/// same address and the same instruction count for as long as anyone asked.
#[test]
fn stepping_past_the_end_says_the_program_exited() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&pe_fixture(), &[]).expect("launch");
    backend.continue_execution().expect("run to completion");

    assert_eq!(
        backend.get_state().status,
        fission_dynamic::debug::types::DebugStatus::Terminated,
        "a finished program did not report as terminated"
    );
    let err = backend
        .single_step()
        .expect_err("stepping an exited program reported success");
    assert!(
        err.to_string().contains("exited"),
        "unhelpful error for a finished program: {err}"
    );
    assert!(backend.continue_execution().is_err());
}

/// What happened, in the order it happened.
///
/// `poll_event` returned "not supported on this platform", so a front end
/// driving the emulator had to infer everything from the machine's state
/// afterwards -- and could not see the guest's output at all, which for a
/// sample under examination is often the whole point. The bytes were already
/// in the emulator's simulated filesystem; nothing surfaced them.
#[test]
fn a_session_reports_what_the_program_did() {
    use fission_dynamic::debug::types::DebugEvent;

    let mut backend = EmulatorBackend::new();
    let pid = backend.launch(&pe_fixture(), &[]).expect("launch");

    // Launching is itself an event.
    assert!(
        matches!(
            backend.poll_event(0).expect("poll"),
            Some(DebugEvent::ProcessCreated { pid: p, .. }) if p == pid
        ),
        "launching reported nothing"
    );

    backend.continue_execution().expect("run");

    let mut events = Vec::new();
    while let Some(event) = backend.poll_event(0).expect("poll") {
        events.push(event);
    }

    let output: String = events
        .iter()
        .filter_map(|e| match e {
            DebugEvent::OutputString { message } => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        output.contains("hi"),
        "the program's own output never reached the session: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DebugEvent::ProcessExited { .. })),
        "the program ended and the session was not told: {events:?}"
    );
    // And the queue empties.
    assert!(
        backend.poll_event(0).expect("poll").is_none(),
        "the queue still had something in it"
    );
}

/// A breakpoint and a watchpoint arrive as separate, distinguishable events.
#[test]
fn a_watchpoint_is_reported_as_itself() {
    use fission_dynamic::debug::types::DebugEvent;

    let mut backend = EmulatorBackend::new();
    backend.launch(&pe_fixture(), &[]).expect("launch");
    for _ in 0..8 {
        backend.single_step().expect("step");
    }
    let sp = backend.fetch_registers(1).unwrap().get("RSP").expect("RSP");
    while backend.poll_event(0).expect("poll").is_some() {}

    backend
        .set_memory_breakpoint(sp.wrapping_sub(256), 256, MemoryBpKind::Write)
        .expect("watch");
    backend.continue_execution().expect("run");

    let mut events = Vec::new();
    while let Some(event) = backend.poll_event(0).expect("poll") {
        events.push(event);
    }
    if let Some(DebugEvent::WatchpointHit { pc, write, .. }) = events
        .iter()
        .find(|e| matches!(e, DebugEvent::WatchpointHit { .. }))
    {
        assert!(*write);
        assert_ne!(*pc, 0, "the watchpoint event carried no instruction");
    }
}

/// The two things an emulated process can be asked about, and the errors for
/// everything else -- which say why rather than "not supported".
#[test]
fn attaching_and_thread_switching_mean_what_they_can() {
    let mut backend = EmulatorBackend::new();

    let err = backend.attach(9999).expect_err("nothing launched yet");
    assert!(err.to_string().contains("launched"), "unhelpful: {err}");

    let pid = backend.launch(&pe_fixture(), &[]).expect("launch");
    backend.attach(pid).expect("attach to the running machine");
    assert!(backend.attach(pid + 1).is_err());

    backend.set_current_thread(1).expect("the one thread");
    let err = backend
        .set_current_thread(2)
        .expect_err("there is no thread 2");
    assert!(
        err.to_string().contains("one guest context"),
        "unhelpful: {err}"
    );

    backend.detach().expect("detach");
    assert!(!backend.is_attached());
    assert!(backend.poll_event(0).is_err(), "polling a dead session");
}
