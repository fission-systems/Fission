//! ARMv7-M's special registers answer, and answer consistently.
//!
//! # What this covers, and what it does not
//!
//! The system-register semantics are architecture-independent in the way they
//! are implemented -- the state is a struct, and the only register it touches
//! is whatever `arch.sp_reg` names -- so they are exercised here through an
//! ordinary emulator rather than a Cortex-M image. There is no Cortex-M
//! fixture in this repository and there will not be one built from a corpus
//! binary: the benchmark corpus's firmware images are malware-adjacent and are
//! never executed.
//!
//! What that leaves uncovered is the *bridge*: that these names arrive from
//! real Thumb code. The translation-coverage benchmark is what shows that --
//! the list of names here came from it, and re-running it is what shows them
//! move from "nothing answers yet" to "answered".
//!
//! The thing that can actually be wrong is the state machine, and that is what
//! is pinned below: what a core reports out of reset, that handler mode is
//! privileged whatever `CONTROL` says, and that the banked stack pointers stay
//! coherent with `sp` across a stack switch.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{BareMetalEnv, env::answer_processor_userop};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// An emulator to hang the state on. The image is irrelevant -- nothing here
/// executes a guest instruction.
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

/// Call a userop the way the dispatch does, and read back what it answered.
fn ask(emu: &mut Emulator, name: &str, args: &[u64]) -> u64 {
    assert!(
        answer_processor_userop(emu, name, args),
        "{name} was not answered at all"
    );
    emu.callother_result
}

#[test]
fn a_core_out_of_reset_reports_a_machine_nothing_has_interrupted() {
    let mut emu = emulator();

    // This is the one that mattered most. Every `MRS` in the SLEIGH spec is
    // guarded by it, and a zero here means "unprivileged" -- so the guarded
    // read is skipped and the destination keeps the zero the spec wrote first.
    // Unanswered, every special-register read in every Cortex-M image came
    // back as zero.
    assert_eq!(ask(&mut emu, "isCurrentModePrivileged", &[]), 1);
    assert_eq!(ask(&mut emu, "isThreadMode", &[]), 1);
    assert_eq!(ask(&mut emu, "isThreadModePrivileged", &[]), 1);
    assert_eq!(ask(&mut emu, "isUsingMainStack", &[]), 1);
    assert_eq!(ask(&mut emu, "isIRQinterruptsEnabled", &[]), 1);
    assert_eq!(ask(&mut emu, "getBasePriority", &[]), 0);
    assert_eq!(ask(&mut emu, "getCurrentExceptionNumber", &[]), 0);
}

#[test]
fn what_was_written_reads_back() {
    let mut emu = emulator();

    assert!(answer_processor_userop(
        &mut emu,
        "setBasePriority",
        &[0x20]
    ));
    assert_eq!(ask(&mut emu, "getBasePriority", &[]), 0x20);

    assert!(answer_processor_userop(
        &mut emu,
        "disableIRQinterrupts",
        &[]
    ));
    assert_eq!(ask(&mut emu, "isIRQinterruptsEnabled", &[]), 0);
    assert!(answer_processor_userop(
        &mut emu,
        "enableIRQinterrupts",
        &[]
    ));
    assert_eq!(ask(&mut emu, "isIRQinterruptsEnabled", &[]), 1);

    assert!(answer_processor_userop(
        &mut emu,
        "setThreadModePrivileged",
        &[0]
    ));
    assert_eq!(ask(&mut emu, "isThreadModePrivileged", &[]), 0);
    assert_eq!(
        ask(&mut emu, "isCurrentModePrivileged", &[]),
        0,
        "thread mode, and CONTROL says unprivileged"
    );

    // An exception handler is privileged whatever CONTROL says.
    emu.cortex_m.exception_number = 3;
    assert_eq!(ask(&mut emu, "isCurrentModePrivileged", &[]), 1);
    assert_eq!(ask(&mut emu, "isThreadMode", &[]), 0);
    assert_eq!(ask(&mut emu, "getCurrentExceptionNumber", &[]), 3);
}

#[test]
fn the_active_stack_pointer_is_sp_and_the_other_one_is_banked() {
    let mut emu = emulator();
    let sp_reg = emu.arch.sp_reg;
    emu.write_register_u64(sp_reg, 0x2000_1000).expect("sp");

    // Main stack selected at reset, so MSP *is* `sp`. Reading a saved copy
    // instead would report a stale value the moment the guest pushes.
    assert_eq!(ask(&mut emu, "getMainStackPointer", &[]), 0x2000_1000);
    assert_eq!(ask(&mut emu, "getProcessStackPointer", &[]), 0);

    // An RTOS sets PSP while still on MSP, then switches.
    assert!(answer_processor_userop(
        &mut emu,
        "setProcessStackPointer",
        &[0x2000_8000]
    ));
    assert_eq!(ask(&mut emu, "getProcessStackPointer", &[]), 0x2000_8000);
    assert_eq!(
        emu.read_register_u64(sp_reg).unwrap(),
        0x2000_1000,
        "writing the inactive bank must not move sp"
    );

    // `setStackMode(0)` selects the process stack.
    assert!(answer_processor_userop(&mut emu, "setStackMode", &[0]));
    assert_eq!(ask(&mut emu, "isUsingMainStack", &[]), 0);
    assert_eq!(
        emu.read_register_u64(sp_reg).unwrap(),
        0x2000_8000,
        "the switch has to move the selected pointer into sp"
    );
    assert_eq!(
        ask(&mut emu, "getMainStackPointer", &[]),
        0x2000_1000,
        "and bank the one it replaced"
    );

    // Back again, with the stacks having moved in the meantime.
    emu.write_register_u64(sp_reg, 0x2000_7F00).expect("sp");
    assert!(answer_processor_userop(&mut emu, "setStackMode", &[1]));
    assert_eq!(emu.read_register_u64(sp_reg).unwrap(), 0x2000_1000);
    assert_eq!(ask(&mut emu, "getProcessStackPointer", &[]), 0x2000_7F00);
}

#[test]
fn selecting_the_stack_already_selected_changes_nothing() {
    let mut emu = emulator();
    let sp_reg = emu.arch.sp_reg;
    emu.write_register_u64(sp_reg, 0x2000_1000).expect("sp");
    emu.cortex_m.banked_sp = 0x2000_8000;

    // The spec's `msr control` path reads `isUsingMainStack()` and hands the
    // same value straight back to `setStackMode`, so this is the common case,
    // not a corner one. Swapping the banks on it would corrupt `sp`.
    assert!(answer_processor_userop(&mut emu, "setStackMode", &[1]));
    assert_eq!(emu.read_register_u64(sp_reg).unwrap(), 0x2000_1000);
    assert_eq!(emu.cortex_m.banked_sp, 0x2000_8000);
}

#[test]
fn an_architecture_with_no_isa_mode_is_left_alone() {
    // `setISAMode` flushes every compiled block when the mode changes, so it
    // must be inert where there is no mode to change -- x86 has no
    // `ISAModeSwitch` register and no `TMode` context field.
    let mut emu = emulator();
    let before = emu.metrics.tbs_compiled;
    assert!(answer_processor_userop(&mut emu, "setISAMode", &[]));
    assert_eq!(emu.metrics.tbs_compiled, before);
}
