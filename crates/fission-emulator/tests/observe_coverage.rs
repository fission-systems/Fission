//! An observer sees a real run, and an unobserved run is unchanged by it.
//!
//! Uses the emulator crate's own static ELF fixture -- built from source in
//! this repository, never a corpus binary. Dynamic analysis is exactly the
//! thing not to point at the benchmark corpus.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{BehaviorEvent, BehaviorLog, Coverage, ObserveMask};
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf")
}

fn build(max_inst: u64) -> Emulator {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info).expect("image");
    emu
}

#[test]
fn coverage_names_the_code_that_actually_ran() {
    let mut emu = build(20_000);
    emu.add_observer(Box::new(Coverage::new()));
    assert!(emu.observe_mask().block, "coverage asks for block entries");
    assert!(
        !emu.observe_mask().insn,
        "and must not force a per-instruction call"
    );

    let _ = emu.run();
    let executed = emu.inst_count;

    let observers = emu.take_observers();
    let cov = observers[0]
        .as_any()
        .downcast_ref::<Coverage>()
        .expect("coverage back");

    assert!(!cov.blocks.is_empty(), "no block was ever reported");
    let insns = cov.executed_instructions();
    assert!(!insns.is_empty(), "no instruction was covered");
    assert!(
        cov.bytes_covered() > 0,
        "covered instructions occupy no bytes"
    );

    // Every block reported must have been described at translation time, or
    // coverage cannot say which instructions it stands for.
    for pc in cov.blocks.keys() {
        assert!(
            cov.block_insns.contains_key(pc),
            "block 0x{pc:X} executed but was never translated in view of the observer"
        );
    }

    // Distinct instruction addresses cannot exceed instructions retired.
    assert!(
        insns.len() as u64 <= executed,
        "covered {} instructions but only {executed} ran",
        insns.len()
    );
}

#[test]
fn a_behavior_log_records_the_calls_a_run_makes_outward() {
    let mut emu = build(20_000);
    emu.add_observer(Box::new(BehaviorLog::new()));
    assert_eq!(
        emu.observe_mask(),
        ObserveMask::NONE,
        "syscall/HLE hooks are host-side and need no compiled-code instrumentation"
    );

    let _ = emu.run();

    let observers = emu.take_observers();
    let log = observers[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log back");

    assert!(
        !log.events.is_empty(),
        "a static CRT start-up makes no outward call at all?"
    );

    // The fixture is statically linked, so it reaches the kernel directly --
    // no PLT, no HLE stub. What start-up must do is fixed by the ABI: set the
    // TLS base, then claim memory, then leave.
    let syscalls: Vec<(u64, [u64; 6])> = log
        .events
        .iter()
        .filter_map(|e| match e {
            BehaviorEvent::Syscall { number, args, .. } => Some((*number, *args)),
            _ => None,
        })
        .collect();
    let numbers: Vec<u64> = syscalls.iter().map(|(n, _)| *n).collect();

    // arch_prctl(ARCH_SET_FS, ...) -- and the argument is the point: a report
    // that only counted syscalls could not tell this from any other prctl.
    let (_, arch_prctl_args) = syscalls
        .iter()
        .find(|(n, _)| *n == 158)
        .unwrap_or_else(|| panic!("no arch_prctl in {numbers:?}"));
    assert_eq!(
        arch_prctl_args[0], 0x1002,
        "arch_prctl's first argument should be ARCH_SET_FS"
    );
    assert_ne!(arch_prctl_args[1], 0, "ARCH_SET_FS with a null TLS base");

    assert!(numbers.contains(&12), "no brk in {numbers:?}");
    assert!(numbers.contains(&9), "no mmap in {numbers:?}");
    assert_eq!(
        numbers.last(),
        Some(&231),
        "a finished run should end at exit_group, got {numbers:?}"
    );

    // Every event renders to a line an analyst reads, not a row of numbers.
    for e in &log.events {
        eprintln!("  {}", e.render());
    }
    let rendered: Vec<String> = log.events.iter().map(|e| e.render()).collect();
    assert!(
        rendered
            .iter()
            .any(|l| l.starts_with("arch_prctl(ARCH_SET_FS,")),
        "arch_prctl's code should be named, got {rendered:?}"
    );

    // Ordering is the other half of a behaviour log: TLS is set up before the
    // allocator asks the kernel for anything.
    let first_prctl = numbers.iter().position(|n| *n == 158).expect("arch_prctl");
    let first_brk = numbers.iter().position(|n| *n == 12).expect("brk");
    assert!(first_prctl < first_brk, "start-up order lost: {numbers:?}");
}

#[test]
fn an_unobserved_run_matches_an_observed_one() {
    // The point of deciding instrumentation at translation time is that it
    // changes nothing else. If observing moved the guest's own behaviour, the
    // measurement would be reporting on itself.
    let mut plain = build(20_000);
    let _ = plain.run();

    let mut watched = build(20_000);
    watched.add_observer(Box::new(Coverage::new()));
    let _ = watched.run();

    assert_eq!(
        plain.inst_count, watched.inst_count,
        "observing changed how much guest code ran"
    );
    assert_eq!(
        plain.pc, watched.pc,
        "observing changed where the run ended up"
    );
    assert_eq!(
        plain.metrics.exit_reason, watched.metrics.exit_reason,
        "observing changed why the run stopped"
    );
}

/// Memory observation reports what the guest actually read and wrote.
///
/// `ObserveMask.mem` existed, `Observer::on_mem` existed, and nothing ever
/// called it: an observer that asked to watch memory was told, in silence,
/// that the program touched none. That is worse than an unimplemented
/// feature, because the answer looks like data.
#[test]
fn memory_observation_reports_the_accesses_a_run_makes() {
    /// Every RAM access, in order.
    #[derive(Default)]
    struct MemLog {
        events: Vec<(u64, u32, bool, u64)>,
    }

    impl fission_emulator::observe::Observer for MemLog {
        fn interest(&self) -> fission_emulator::observe::ObserveMask {
            fission_emulator::observe::ObserveMask {
                mem: true,
                ..fission_emulator::observe::ObserveMask::NONE
            }
        }
        fn on_mem(&mut self, addr: u64, size: u32, write: bool, value: u64) {
            self.events.push((addr, size, write, value));
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    for interpret in [false, true] {
        let mut emu = build(4096);
        emu.force_interpreter = interpret;
        emu.add_observer(Box::new(MemLog::default()));
        let _ = emu.run();

        let observers = emu.take_observers();
        let log = observers
            .iter()
            .find_map(|o| o.as_any().downcast_ref::<MemLog>())
            .expect("memory log");
        let engine = if interpret { "interpreter" } else { "jit" };

        assert!(
            !log.events.is_empty(),
            "{engine}: a run that pushes a stack frame reported no memory access"
        );
        assert!(
            log.events.iter().any(|(_, _, write, _)| *write),
            "{engine}: reads only -- a call pushes a return address"
        );
        assert!(
            log.events.iter().any(|(_, _, write, _)| !*write),
            "{engine}: writes only -- a return pops one back"
        );
        // Every access has to name a plausible guest address and a width a
        // varnode can have. A zero-size access is a report of nothing.
        for (addr, size, _, _) in &log.events {
            assert!(*addr != 0, "{engine}: an access at address zero");
            assert!(
                *size > 0 && *size <= 16,
                "{engine}: implausible access width {size}"
            );
        }
    }
}
