//! The interpreter and the JIT must agree, block for block.
//!
//! A fallback only ever reached by accident is a fallback nobody has tested,
//! so this drives the same fixture through both engines and compares what the
//! guest actually did -- not just that neither crashed.
//!
//! # Ignored, because they do not agree yet
//!
//! These ran the moment the interpreter was wired, and found two things.
//!
//! The first is fixed: an instruction that lifts to *no* p-code -- x86
//! `nop dword ptr [rax]` is one -- shares its start index with whatever
//! follows it, and both engines keyed their per-instruction accounting by that
//! index alone. One of the pair vanished, so `jit_count_insn` fired once for
//! two instructions and `inst_count` had been under-reporting every
//! p-code-less instruction in every run.
//!
//! The second is open. With that fixed the traces agree for 1,060 steps and
//! then split on a flag:
//!
//! ```text
//! after: 0x10034B0 0x10034B2 0x10034B5 0x10034A0 0x10034A2 0x10034A5
//! jit:    0x10034A7   (fell through)
//! interp: 0x10034B7   (took the branch)
//! ```
//!
//! `0x10034A5` is `jnz`, `0x10034A2` is `test CL, 0x1`, and the block is a
//! loop whose first iteration both engines agreed on. So the disagreement is
//! over `CL` or over the `EDX` that `0x10034A0` copies into it -- one engine's
//! sub-register read or its `movzx` from `[RCX + 0x1000280]` is wrong. Not yet
//! diagnosed, and until it is, the interpreter is a fallback for blocks the
//! JIT declines, not an engine to trust on its own.
//!
//! Run them with `cargo test -p fission-emulator --test interp_differential
//! -- --ignored`.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{BehaviorEvent, BehaviorLog, Coverage};
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(max_inst: u64, interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
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
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// Ordered PC trace. A sorted coverage set can only say *that* two runs
/// differ; the first place they differ is what names the bug.
#[derive(Default)]
struct PcTrace {
    pcs: Vec<u64>,
}

impl fission_emulator::observe::Observer for PcTrace {
    fn interest(&self) -> fission_emulator::observe::ObserveMask {
        fission_emulator::observe::ObserveMask {
            insn: true,
            ..fission_emulator::observe::ObserveMask::NONE
        }
    }
    fn on_insn(&mut self, pc: u64) {
        self.pcs.push(pc);
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn first_divergence(a: &[u64], b: &[u64]) -> Option<(usize, Option<u64>, Option<u64>)> {
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied(), b.get(i).copied());
        if x != y {
            return Some((i, x, y));
        }
    }
    None
}

fn syscall_trace(emu: &mut Emulator) -> Vec<(u64, [u64; 6])> {
    let observers = emu.take_observers();
    let log = observers
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<BehaviorLog>())
        .expect("behaviour log");
    log.events
        .iter()
        .filter_map(|e| match e {
            BehaviorEvent::Syscall { number, args, .. } => Some((*number, *args)),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "interpreter diverges from the JIT; see the module doc for where"]
fn the_interpreter_reaches_the_same_place_as_the_jit() {
    let mut jitted = build(20_000, false);
    jitted.add_observer(Box::new(BehaviorLog::new()));
    let jit_result = jitted.run();
    let jit_calls = syscall_trace(&mut jitted);

    let mut interpreted = build(20_000, true);
    interpreted.add_observer(Box::new(BehaviorLog::new()));
    let interp_result = interpreted.run();
    let interp_calls = syscall_trace(&mut interpreted);

    assert_eq!(
        jit_result.is_ok(),
        interp_result.is_ok(),
        "engines disagree on whether the run succeeded: jit={jit_result:?} interp={interp_result:?}"
    );
    assert!(
        interpreted.interpreted_blocks > 0,
        "force_interpreter did not actually interpret anything"
    );
    assert_eq!(
        interpreted.metrics.tbs_compiled, 0,
        "force_interpreter still compiled blocks"
    );

    // The syscall trace is the guest's observable behaviour: same calls, same
    // arguments, same order, or the two engines are not running the same
    // program.
    assert_eq!(
        jit_calls, interp_calls,
        "syscall traces differ\n  jit:    {jit_calls:?}\n  interp: {interp_calls:?}"
    );

    assert_eq!(
        jitted.inst_count, interpreted.inst_count,
        "engines retired different instruction counts"
    );
    assert_eq!(jitted.pc, interpreted.pc, "engines ended at different PCs");
    assert_eq!(
        jitted.metrics.exit_reason, interpreted.metrics.exit_reason,
        "engines stopped for different reasons"
    );
}

#[test]
#[ignore = "interpreter diverges from the JIT; see the module doc for where"]
fn the_interpreter_covers_the_same_code() {
    let mut jitted = build(20_000, false);
    jitted.add_observer(Box::new(Coverage::new()));
    let _ = jitted.run();
    let jit_obs = jitted.take_observers();
    let jit_cov = jit_obs[0].as_any().downcast_ref::<Coverage>().unwrap();

    let mut interpreted = build(20_000, true);
    interpreted.add_observer(Box::new(Coverage::new()));
    let _ = interpreted.run();
    let int_obs = interpreted.take_observers();
    let int_cov = int_obs[0].as_any().downcast_ref::<Coverage>().unwrap();

    let (jit_insns, int_insns) = (
        jit_cov.executed_instructions(),
        int_cov.executed_instructions(),
    );
    if let Some((i, a, b)) = first_divergence(&jit_insns, &int_insns) {
        panic!(
            "coverage differs at sorted index {i}: jit={} interp={} \
             ({} vs {} instructions, {} vs {} bytes)",
            a.map_or("<none>".into(), |p| format!("0x{p:X}")),
            b.map_or("<none>".into(), |p| format!("0x{p:X}")),
            jit_insns.len(),
            int_insns.len(),
            jit_cov.bytes_covered(),
            int_cov.bytes_covered(),
        );
    }
}

#[test]
#[ignore = "interpreter diverges from the JIT; see the module doc for where"]
fn the_engines_take_the_same_path_instruction_by_instruction() {
    let mut jitted = build(20_000, false);
    jitted.add_observer(Box::new(PcTrace::default()));
    let _ = jitted.run();
    let jit_obs = jitted.take_observers();
    let jit_pcs = &jit_obs[0].as_any().downcast_ref::<PcTrace>().unwrap().pcs;

    let mut interpreted = build(20_000, true);
    interpreted.add_observer(Box::new(PcTrace::default()));
    let _ = interpreted.run();
    let int_obs = interpreted.take_observers();
    let int_pcs = &int_obs[0].as_any().downcast_ref::<PcTrace>().unwrap().pcs;

    assert!(
        !jit_pcs.is_empty() && !int_pcs.is_empty(),
        "no trace recorded"
    );

    if let Some((i, jit_pc, int_pc)) = first_divergence(jit_pcs, int_pcs) {
        let context: Vec<String> = jit_pcs[i.saturating_sub(6)..i]
            .iter()
            .map(|pc| format!("0x{pc:X}"))
            .collect();
        panic!(
            "engines diverge at step {i}\n  after: {}\n  jit:    {}\n  interp: {}",
            context.join(" "),
            jit_pc.map_or("<end>".into(), |p| format!("0x{p:X}")),
            int_pc.map_or("<end>".into(), |p| format!("0x{p:X}")),
        );
    }
}
