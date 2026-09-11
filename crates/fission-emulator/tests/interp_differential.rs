//! The interpreter and the JIT must agree, block for block.
//!
//! A fallback only ever reached by accident is a fallback nobody has tested,
//! so this drives the same fixture through both engines and compares what the
//! guest actually did -- not just that neither crashed.
//!
//! # What agrees
//!
//! All of it, on every fixture here: instruction path, outward calls,
//! instruction count, final PC. The tests below are the gate.
//!
//! Getting there fixed six real bugs, each found by this harness and none of
//! them findable any other way -- a benchmark score cannot see a wrong value,
//! only a wrong shape:
//!
//! 1. **The JIT flushed undefined variables over live registers.** `store_vn!`
//!    already writes `host_reg_file`, which *is* register space, so the exit
//!    writeback never needed values -- it needed to invalidate `reg_cache`.
//!    Reading the SSA variables instead meant a register written only on an
//!    untaken path flushed zero: `rep stosq` leaves via `CBRANCH` before the
//!    ops that touch RDI, so the exiting iteration zeroed a live pointer.
//! 2. **The interpreter read a branch target from the wrong field.**
//!    `remap_relative_branches` writes the resolved flat index to
//!    `constant_val` and leaves `offset` holding the signed delta; the JIT
//!    reads `constant_val` and the evaluator read `offset`.
//! 3. **The interpreter truncated wide varnodes to eight bytes.** An XMM
//!    register is sixteen, so a `COPY` left the top half stale.
//! 4. **Neither engine did 128-bit integer arithmetic.** Both took the low
//!    eight bytes, which makes `div r64` -- whose dividend is `RDX:RAX` --
//!    right only while `RDX` is zero. See `wide_integer_semantics`.
//! 5. **`Store` wrote eight bytes of a sixteen-byte value.** `movaps %xmm0,
//!    (%rax)` left half the destination holding its old contents, which
//!    corrupted a `va_copy` and sent `vfprintf` to the wrong stack slots three
//!    thousand instructions before anything looked wrong.
//! 6. **The JIT cached `RDX` and `EDX` as separate variables.** `var_map` is
//!    keyed by (space, offset, size), so the two views of one register are two
//!    entries, and writing the wide one left the narrow one holding what it
//!    had cached. Lazy seeding does not help -- it reads `host_reg_file` the
//!    first time a key is used, which may be before the write. `imulq
//!    %r12,%rdx` followed by `subl %edx,%esi`, inside one block, in musl's
//!    allocator.
//!
//! One difference here is convention, not correctness: on halt the JIT leaves
//! `pc` past the whole block and the interpreter now does the same, because
//! the two have to answer alike even where neither answer is obviously better.
//!
//! They all run by default: a gate that has to be asked for is not one.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{BehaviorEvent, BehaviorLog, Coverage};
use fission_emulator::os::{LinuxEnv, WindowsEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(max_inst: u64, interpret: bool) -> Emulator {
    build_from("x64_static_printf_malloc.elf", max_inst, interpret)
}

/// The fixture both engines can currently run to the end: no SSE, so neither
/// engine's 128-bit approximation is in play.
fn build_simple(max_inst: u64, interpret: bool) -> Emulator {
    let mut emu = build_from("x64_concolic_branch_sys.elf", max_inst, interpret);
    emu.seed_stdin(b"A");
    emu
}

fn build_from(fixture: &str, max_inst: u64, interpret: bool) -> Emulator {
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
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(max_inst));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// A 32-bit PE, from the dev corpus rather than `testdata` -- there is no
/// 32-bit fixture in the crate, and this shape needs a real CRT to exercise.
fn build_pe32(max_inst: u64, interpret: bool) -> Option<Emulator> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/control_flow_gcc-m32_O0.exe");
    if !path.is_file() {
        return None;
    }
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
        .with_max_inst(Some(max_inst));
    emu.force_interpreter = interpret;
    emu.apply_windows_image(info).expect("image");
    Some(emu)
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
    if std::env::var_os("DUMP_TRACES").is_some() {
        let write = |name: &str, pcs: &[u64]| {
            let body: String = pcs.iter().map(|pc| format!("{pc:X}\n")).collect();
            std::fs::write(name, body).expect("dump");
        };
        write("/tmp/trace_jit.txt", jit_pcs);
        write("/tmp/trace_int.txt", int_pcs);
    }

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

/// The gate: on a fixture inside both engines' reach, they must agree exactly.
///
/// The narrowest of the gates here: no SIMD at all, so it was the first to
/// pass and it is the one that keeps passing while the others are being made
/// to. A regression in the plain integer core breaks this before it breaks
/// anything else.
#[test]
fn the_engines_agree_exactly_on_a_binary_without_simd() {
    let mut jitted = build_simple(4096, false);
    jitted.add_observer(Box::new(PcTrace::default()));
    jitted.add_observer(Box::new(BehaviorLog::new()));
    let jit_run = jitted.run();

    let mut interpreted = build_simple(4096, true);
    interpreted.add_observer(Box::new(PcTrace::default()));
    interpreted.add_observer(Box::new(BehaviorLog::new()));
    let int_run = interpreted.run();

    assert!(
        interpreted.interpreted_blocks > 0 && interpreted.metrics.tbs_compiled == 0,
        "the interpreted run did not actually interpret"
    );
    assert_eq!(
        jit_run.is_ok(),
        int_run.is_ok(),
        "engines disagree on success: jit={jit_run:?} interp={int_run:?}"
    );

    let jit_obs = jitted.take_observers();
    let int_obs = interpreted.take_observers();
    let jit_pcs = &jit_obs
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<PcTrace>())
        .unwrap()
        .pcs;
    let int_pcs = &int_obs
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<PcTrace>())
        .unwrap()
        .pcs;
    assert!(!jit_pcs.is_empty(), "no trace recorded");
    if std::env::var_os("DUMP_TRACES").is_some() {
        let write = |name: &str, pcs: &[u64]| {
            let body: String = pcs.iter().map(|pc| format!("{pc:X}\n")).collect();
            std::fs::write(name, body).expect("dump");
        };
        write("/tmp/trace_jit.txt", jit_pcs);
        write("/tmp/trace_int.txt", int_pcs);
    }
    if let Some((i, a, b)) = first_divergence(jit_pcs, int_pcs) {
        panic!(
            "engines diverge at step {i}\n  after: {}\n  jit:    {}\n  interp: {}",
            jit_pcs[i.saturating_sub(6)..i]
                .iter()
                .map(|pc| format!("0x{pc:X}"))
                .collect::<Vec<_>>()
                .join(" "),
            a.map_or("<end>".into(), |p| format!("0x{p:X}")),
            b.map_or("<end>".into(), |p| format!("0x{p:X}")),
        );
    }

    let calls = |obs: &[Box<dyn fission_emulator::observe::Observer>]| -> Vec<String> {
        obs.iter()
            .find_map(|o| o.as_any().downcast_ref::<BehaviorLog>())
            .unwrap()
            .events
            .iter()
            .map(|e| e.render())
            .collect()
    };
    assert_eq!(
        calls(&jit_obs),
        calls(&int_obs),
        "engines made different calls outward"
    );
    assert_eq!(jitted.inst_count, interpreted.inst_count);
    assert_eq!(jitted.pc, interpreted.pc);
}

/// The engines agree on a 32-bit process too.
///
/// This is the fixture that found the fourth bug in the list at the top:
/// `store_vn!` put an untruncated 64-bit value into the SSA variable while
/// writing the correct narrow one to `host_reg_file`. `lea esp, [ebp-0xc]`
/// lifts to `ESP = EBP + 0xFFFFFFF4`, which overflows 32 bits, so the
/// following `pop` in the *same block* loaded from an address 4 GiB too high
/// and read zero -- while the interpreter, which masks, returned normally.
/// Every 32-bit PE in the dev corpus died in its first epilogue.
///
/// Nothing about the defect was 32-bit specific: any narrow arithmetic whose
/// result overflows and is read back inside one block was wrong. It took a
/// 32-bit process to *notice*, because there the stack pointer is narrow.
#[test]
fn the_engines_agree_on_a_32_bit_process() {
    let (Some(mut jitted), Some(mut interpreted)) =
        (build_pe32(500_000, false), build_pe32(500_000, true))
    else {
        eprintln!("skipping: dev corpus not present");
        return;
    };

    jitted.add_observer(Box::new(PcTrace::default()));
    let jit_run = jitted.run();
    interpreted.add_observer(Box::new(PcTrace::default()));
    let int_run = interpreted.run();

    assert_eq!(
        jit_run.is_ok(),
        int_run.is_ok(),
        "engines disagree on success: jit={jit_run:?} interp={int_run:?}"
    );
    assert!(
        jitted.halt_requested && interpreted.halt_requested,
        "the process should exit under both engines: jit={} interp={}",
        jitted.metrics.summary_line(),
        interpreted.metrics.summary_line()
    );

    let jit_obs = jitted.take_observers();
    let int_obs = interpreted.take_observers();
    let jit_pcs = &jit_obs
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<PcTrace>())
        .unwrap()
        .pcs;
    let int_pcs = &int_obs
        .iter()
        .find_map(|o| o.as_any().downcast_ref::<PcTrace>())
        .unwrap()
        .pcs;
    assert!(!jit_pcs.is_empty(), "no trace recorded");
    if let Some((i, a, b)) = first_divergence(jit_pcs, int_pcs) {
        panic!(
            "engines diverge at step {i}\n  after: {}\n  jit:    {a:X?}\n  interp: {b:X?}",
            jit_pcs[i.saturating_sub(6)..i]
                .iter()
                .map(|pc| format!("0x{pc:X}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}
