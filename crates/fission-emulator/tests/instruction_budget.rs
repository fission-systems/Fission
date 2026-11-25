//! `max_inst` is exact under the JIT, wherever a budget ends.
//!
//! `interp_differential::a_budget_of_n_runs_exactly_n_instructions_in_both_engines`
//! covers budgets of one to nine from a cold start. Those never reach the
//! JIT: with fewer than `MAX_TB_INSNS` left, the dispatcher runs the tail on
//! the interpreter. The overrun was in the warm case -- a compiled block
//! chaining into another compiled block that held more instructions than the
//! budget had left, which then ran whole. CSIC measured `n + 1` retired for
//! 16-26% of sampled budgets on x86-64 and AArch64.
//!
//! So each run here resumes one machine again and again with a budget a few
//! instructions further on, the step varied so the ends land at every offset
//! inside the blocks. Every stop must retire exactly the budget, and the JIT
//! must stop where the interpreter -- one instruction at a time, exact by
//! construction -- stops.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(fixture: &str, interpret: bool) -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
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
    let mut emu =
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator");
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// One stop: (budget, retired, pc, whether the guest exited instead).
type Stop = (u64, u64, u64, bool);

/// Resume to successive budgets, recording each stop.
fn stops(fixture: &str, interpret: bool, total: u64) -> Vec<Stop> {
    let mut emu = build(fixture, interpret);
    let mut out = Vec::new();
    let mut budget = 0u64;
    let mut step = 8u64;
    while budget < total && !emu.halt_requested {
        // 8..=29: never under `MAX_TB_INSNS` at the start of a leg, so each
        // leg starts on the JIT path, and the end moves through every offset.
        step = 8 + (step * 7 + 3) % 22;
        budget += step;
        emu.max_inst = Some(budget);
        if emu.run().is_err() {
            break;
        }
        out.push((budget, emu.inst_count, emu.pc, emu.halt_requested));
    }
    out
}

fn check(fixture: &str, total: u64) {
    let jitted = stops(fixture, false, total);
    let interpreted = stops(fixture, true, total);
    assert!(
        jitted.len() > 20,
        "{fixture}: too few legs ran: {}",
        jitted.len()
    );
    let overruns: Vec<_> = jitted
        .iter()
        // The leg the guest exits in stops short of its budget, as it should.
        .filter(|(budget, retired, _, exited)| !exited && retired != budget)
        .collect();
    assert!(
        overruns.is_empty(),
        "{fixture}: the JIT retired other than its budget at {} of {} stops, first {:?}",
        overruns.len(),
        jitted.len(),
        overruns.first()
    );
    assert_eq!(jitted.len(), interpreted.len(), "{fixture}: leg counts");
    for (j, i) in jitted.iter().zip(&interpreted) {
        assert_eq!(
            j, i,
            "{fixture}: (budget, retired, pc, exited) differ between JIT and interpreter"
        );
    }
}

#[test]
fn the_jit_stops_exactly_at_every_budget_on_x86_64() {
    check("x64_static_printf_malloc.elf", 3000);
}

#[test]
fn the_jit_stops_exactly_at_every_budget_on_aarch64() {
    check("aarch64_control_flow.elf", 3000);
}
