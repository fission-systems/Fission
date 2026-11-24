//! The one-variable intervention runner.
//!
//! 1. Record the incident: single-step the program on its pinned input and
//!    evaluate Φ ([`crate::trace::record`]).
//! 2. Check the experiment is reproducible: two fresh JIT replays and the
//!    interpreter recording must end identically, and Φ must not depend on
//!    a non-deterministic environment value.
//! 3. Generate candidates from the backward dynamic slice of Φ (data plus
//!    control dependence from static postdominators).
//! 4. For each candidate, restore the checkpoint at its step (or replay to
//!    it), set one alternative value, replay the same input and environment
//!    and evaluate Φ.
//! 5. Re-run every flip from the initial state in a fresh emulator, and
//!    once more with the JIT finishing the run, before calling it a
//!    witness.
//! 6. With no single witness, try bounded pairs of input-byte
//!    interventions, using the slice of each single counterfactual run to
//!    find partners the recorded run never read.
//!
//! Candidate order, values, and every verdict are deterministic; only the
//! `timing` block of a report depends on the machine.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use fission_emulator::MachineState;
use fission_emulator::os::linux::image_info::ImageInfo;
use fission_emulator::os::linux::signal::SignalState;
use fission_emulator::os::vfs::SimVFS;
use serde::{Deserialize, Serialize};

use crate::contract::{
    CONTRACT_VERSION, CandidateOrigin, CandidateResult, EnvClass, EnvSource, EnvSourceStatus,
    Intervention, InterventionWitness, InvalidReason, JointStatus, Location, ReplayFingerprint,
    SinkObservation, SinkSpec, Verdict,
};
use crate::machine::{Engine, Program, Run};
use crate::slice::{self, Candidate, Slice};
use crate::trace::{self, Access, EndState, SinkMonitor, Trace};

/// Budgets and switches for one case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Instructions a recording or replay may retire.
    pub max_steps: u64,
    /// Oracle executions (replays that evaluate Φ) per case, including
    /// checkpoint validation, environment perturbation and pair search.
    pub max_oracles: u64,
    pub values_per_candidate: usize,
    pub max_candidates: usize,
    pub max_pairs: u64,
    /// Follow dynamic control dependence when slicing.
    pub control_dependence: bool,
    /// Follow potential dependence (branches whose untaken arm could have
    /// redefined a value) when slicing.
    pub potential_dependence: bool,
    /// Restore checkpoints instead of replaying from the initial state.
    pub use_checkpoints: bool,
    /// Minimum distance in steps between two checkpoints.
    pub checkpoint_spacing: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_steps: 200_000,
            max_oracles: 4_000,
            values_per_candidate: 6,
            max_candidates: 600,
            max_pairs: 64,
            control_dependence: true,
            potential_dependence: true,
            use_checkpoints: true,
            checkpoint_spacing: 16,
        }
    }
}

/// One experiment: a program pinned to one input, a sink, and optionally
/// interventions the caller wants judged.
pub struct Case<'p> {
    pub name: String,
    pub program: &'p Program,
    pub sink: SinkSpec,
    pub requested: Vec<Intervention>,
}

/// Reproducibility evidence gathered before any candidate is tried.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Determinism {
    /// Two fresh JIT replays ended identically.
    pub replays_agree: bool,
    /// ...and identically to the interpreter recording.
    pub engines_agree: bool,
    pub recorded_end: String,
    pub replay_end: String,
}

/// Deterministic counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    pub recorded_steps: u64,
    pub sink_step: u64,
    pub slice_steps: usize,
    pub slice_static_instructions: usize,
    pub slice_input_bytes: usize,
    pub candidates_generated: usize,
    pub candidates_tested: usize,
    pub oracle_executions: u64,
    pub fresh_verifications: u64,
    pub checkpoints: usize,
    pub checkpoints_invalid: usize,
}

/// Machine-dependent measurements. Excluded from determinism checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timing {
    pub record_us: u128,
    pub checkpoint_capture_us: u128,
    /// Restoring a checkpoint into the pooled worker (the fork).
    pub checkpoint_restore_us_mean: f64,
    /// Replaying from the restored checkpoint to the exact step.
    pub replay_to_step_us_mean: f64,
    /// Building a new emulator, as fresh verification does.
    pub fresh_emulator_us_mean: f64,
    pub worker_construction_us: u128,
    pub oracle_us_mean: f64,
    pub total_us: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseReport {
    pub contract: String,
    pub case: String,
    pub fingerprint: ReplayFingerprint,
    pub sink: SinkSpec,
    pub original: SinkObservation,
    pub end: String,
    pub determinism: Determinism,
    /// Why the case was refused, if it was.
    pub refusal: Option<String>,
    pub candidates: Vec<CandidateResult>,
    pub requested: Vec<CandidateResult>,
    pub joint: JointStatus,
    /// Input offsets with a single-byte counterfactual witness.
    pub witness_inputs: Vec<usize>,
    pub counts: Counts,
    pub config: Config,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<Timing>,
}

impl CaseReport {
    /// The report without machine-dependent fields, for determinism checks
    /// and snapshots.
    pub fn deterministic_json(&self) -> String {
        let mut copy = self.clone();
        copy.timing = None;
        serde_json::to_string_pretty(&copy).unwrap_or_default()
    }
}

/// Emulator state outside `MachineState` that a checkpoint carries.
#[derive(Clone)]
struct Checkpoint {
    step: u64,
    pc: u64,
    state: MachineState,
    /// Shared between checkpoints whose file state is the same: the VFS
    /// carries the program image as seeded paths, which would otherwise be
    /// copied into every checkpoint.
    vfs: std::sync::Arc<SimVFS>,
    stdin_buffer: Option<Vec<u8>>,
    exit_code: Option<u32>,
    exit_signal: Option<i32>,
    last_fault: Option<fission_emulator::core::GuestFault>,
    heap_cursor: u64,
    fs_base: u64,
    gs_base: u64,
    clear_child_tid: u64,
    current_tid: u64,
    tick_count: u64,
    tsc_reads: u64,
    signals: SignalState,
    callother_result: u64,
    image_info: Option<ImageInfo>,
}

impl Checkpoint {
    fn capture(run: &mut Run) -> Self {
        let emu = &mut run.emu;
        let mut state = emu.state.clone();
        state.trace_mem_reads.clear();
        state.trace_mem_writes.clear();
        state.trace_shadow_writes.clear();
        Self {
            step: emu.inst_count,
            pc: emu.pc,
            state,
            vfs: std::sync::Arc::new(emu.vfs.clone()),
            stdin_buffer: emu.stdin_buffer.clone(),
            exit_code: emu.exit_code,
            exit_signal: emu.exit_signal,
            last_fault: emu.last_fault,
            heap_cursor: emu.heap_cursor,
            fs_base: emu.fs_base,
            gs_base: emu.gs_base,
            clear_child_tid: emu.clear_child_tid,
            current_tid: emu.current_tid,
            tick_count: emu.tick_count,
            tsc_reads: emu.tsc_reads,
            signals: emu.signals.clone(),
            callother_result: emu.callother_result,
            image_info: emu.image_info.clone(),
        }
    }

    /// Restore into a freshly built emulator. The OS layer's private state
    /// (task scheduler, HLE tables) is the fresh one's; every checkpoint is
    /// validated by a null-intervention replay before it is trusted.
    fn restore(&self, run: &mut Run, flush_blocks: bool) {
        let emu = &mut run.emu;
        let hooks = std::mem::take(&mut emu.state.hooks);
        emu.state = self.state.clone();
        emu.state.hooks = hooks;
        // `host_reg_file` is the register file itself (not a cache), so it
        // comes with the cloned state and must not be cleared.
        emu.state.normalize_host_reg_file();
        if flush_blocks {
            emu.jit_cache.flush_all();
        }
        emu.pc = self.pc;
        emu.inst_count = self.step;
        emu.vfs = (*self.vfs).clone();
        emu.stdin_buffer = self.stdin_buffer.clone();
        emu.exit_code = self.exit_code;
        emu.exit_signal = self.exit_signal;
        emu.last_fault = self.last_fault;
        emu.heap_cursor = self.heap_cursor;
        emu.fs_base = self.fs_base;
        emu.gs_base = self.gs_base;
        emu.clear_child_tid = self.clear_child_tid;
        emu.current_tid = self.current_tid;
        emu.tick_count = self.tick_count;
        emu.tsc_reads = self.tsc_reads;
        emu.signals = self.signals.clone();
        emu.callother_result = self.callother_result;
        emu.image_info = self.image_info.clone();
        emu.halt_requested = false;
    }
}

/// Read a location's current bytes.
pub fn read_location(program: &Program, run: &mut Run, location: &Location) -> Result<Vec<u8>> {
    let (space, offset, size) = program
        .storage(location)
        .ok_or_else(|| anyhow!("unknown location {location:?}"))?;
    run.emu.state.read_space(space, offset, size as usize)
}

/// Check an intervention against the contract on a run positioned at its
/// step. Returns the location's original bytes.
pub fn validate(
    program: &Program,
    run: &mut Run,
    trace: &Trace,
    iv: &Intervention,
) -> std::result::Result<Vec<u8>, InvalidReason> {
    if iv.step >= trace.sink_step || iv.step >= trace.steps.len() as u64 {
        return Err(InvalidReason::NotBeforeSink {
            step: iv.step,
            sink_step: trace.sink_step,
        });
    }
    let width = match &iv.location {
        Location::Register { name } => {
            let Some((_, offset, size)) = program.register(name) else {
                return Err(InvalidReason::UnknownRegister { name: name.clone() });
            };
            if let Some((_, pc_off, pc_size)) = program.register(&program.pc_register)
                && offset < pc_off + u64::from(pc_size)
                && pc_off < offset + u64::from(size)
            {
                return Err(InvalidReason::ProgramCounter);
            }
            size
        }
        Location::Memory { addr, len } => {
            for i in 0..u64::from(*len) {
                let a = addr.wrapping_add(i);
                if run.emu.state.page_map.page_flags(a).is_none() {
                    return Err(InvalidReason::Unmapped { addr: a });
                }
            }
            if run.emu.state.page_map.range_has_exec(*addr, *len as usize) {
                return Err(InvalidReason::CodePage { addr: *addr });
            }
            *len
        }
    };
    if iv.value.len() != width as usize {
        return Err(InvalidReason::WidthMismatch {
            expected: width,
            got: iv.value.len() as u32,
        });
    }
    let original = read_location(program, run, &iv.location).map_err(|_| match &iv.location {
        Location::Memory { addr, .. } => InvalidReason::Unmapped { addr: *addr },
        Location::Register { name } => InvalidReason::UnknownRegister { name: name.clone() },
    })?;
    if original == iv.value {
        return Err(InvalidReason::NoOp);
    }
    Ok(original)
}

/// Set a location. The caller has validated the intervention.
pub fn apply(program: &Program, run: &mut Run, iv: &Intervention) -> Result<()> {
    let (space, offset, _) = program
        .storage(&iv.location)
        .ok_or_else(|| anyhow!("unknown location {:?}", iv.location))?;
    // An intervention is the experimenter setting state, not a guest store:
    // page protections do not apply to it.
    let enforce = std::mem::replace(&mut run.emu.state.enforce_page_faults, false);
    let written = run.emu.state.write_space(space, offset, &iv.value);
    run.emu.state.enforce_page_faults = enforce;
    written
}

fn end_label(end: &EndState) -> String {
    match end {
        EndState::Exited { code } => format!("exited({})", code.map_or(-1, i64::from)),
        EndState::Faulted { pc, .. } => format!("faulted(0x{pc:x})"),
        EndState::Killed { signal } => format!("killed({signal})"),
        EndState::StoppedAtSink => "stopped_at_sink".into(),
        EndState::Budget => "budget".into(),
    }
}

/// What a run left behind that every engine must agree on.
fn engine_digest(run: &Run, end: &EndState) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(end_label(end).as_bytes());
    hasher.update(&run.stdout());
    hasher.finalize().to_hex().to_string()
}

/// What can change in the VFS while a program runs: per-descriptor content
/// length, cursor and EOF, and the next descriptor. Seeded paths do not.
fn vfs_signature(vfs: &SimVFS) -> Vec<(u64, usize, usize, bool)> {
    let mut sig: Vec<(u64, usize, usize, bool)> = vfs
        .files
        .iter()
        .map(|(fd, f)| (*fd, f.content.len(), f.cursor, f.eof))
        .collect();
    sig.sort_unstable();
    sig.push((u64::MAX, vfs.next_fd as usize, 0, false));
    sig
}

/// A pooled emulator and the checkpoints it can be restored to.
///
/// Building an emulator is not cheap (it re-reads the language's compiled
/// Sleigh tables for its register and userop maps), so experiments restore
/// one long-lived worker instead. The fork is the checkpoint: a clone of
/// `MachineState`, whose pages are persistent maps of shared buffers, so a
/// clone costs the register file and page-map bookkeeping and a page is
/// copied only when written.
pub struct Replayer<'p> {
    pub program: &'p Program,
    pub worker: Run,
    /// The engine that finishes a replay after positioning. Positioning
    /// itself always uses the interpreter (exact step counts).
    engine: Engine,
    /// Flush compiled blocks on restore. Needed only if the recorded run
    /// wrote executable memory; otherwise every checkpoint holds the same
    /// code and the worker's block cache stays valid across restores.
    flush_on_restore: bool,
    checkpoints: BTreeMap<u64, Checkpoint>,
    pub restore_us: Vec<u128>,
    pub replay_us: Vec<u128>,
    pub construction_us: u128,
}

impl<'p> Replayer<'p> {
    pub fn new(program: &'p Program, engine: Engine, flush_on_restore: bool) -> Result<Self> {
        let started = Instant::now();
        let mut worker = program.fresh(engine)?;
        let construction_us = started.elapsed().as_micros();
        let initial = Checkpoint::capture(&mut worker);
        Ok(Self {
            program,
            worker,
            engine,
            flush_on_restore,
            checkpoints: BTreeMap::from([(0, initial)]),
            restore_us: Vec::new(),
            replay_us: Vec::new(),
            construction_us,
        })
    }

    /// Capture a checkpoint at each of `steps` (ascending) in one pass.
    pub fn capture(&mut self, steps: &BTreeSet<u64>) -> Result<()> {
        self.position(0)?;
        for &step in steps {
            if step == 0 {
                continue;
            }
            if !self.worker.run_to_step(step)? {
                break;
            }
            let mut cp = Checkpoint::capture(&mut self.worker);
            if let Some((_, previous)) = self.checkpoints.range(..step).next_back()
                && vfs_signature(&previous.vfs) == vfs_signature(&cp.vfs)
            {
                cp.vfs = std::sync::Arc::clone(&previous.vfs);
            }
            self.checkpoints.insert(step, cp);
        }
        Ok(())
    }

    pub fn forget(&mut self, step: u64) {
        if step != 0 {
            self.checkpoints.remove(&step);
        }
    }

    pub fn checkpoint_steps(&self) -> Vec<u64> {
        self.checkpoints.keys().copied().collect()
    }

    /// Restore the closest checkpoint at or before `step` and replay the
    /// rest of the way.
    pub fn position(&mut self, step: u64) -> Result<&mut Run> {
        let started = Instant::now();
        let (_, cp) = self
            .checkpoints
            .range(..=step)
            .next_back()
            .ok_or_else(|| anyhow!("no checkpoint at or before {step}"))?;
        cp.restore(&mut self.worker, self.flush_on_restore);
        self.worker.emu.force_interpreter = self.engine == Engine::Interpreter;
        self.worker.emu.max_inst = None;
        self.restore_us.push(started.elapsed().as_micros());
        if self.worker.emu.inst_count < step {
            let started = Instant::now();
            let from = self.worker.emu.inst_count;
            if !self
                .worker
                .run_to_step(step)
                .with_context(|| format!("replaying from checkpoint {from} to step {step}"))?
            {
                anyhow::bail!("could not reach step {step}");
            }
            self.replay_us.push(started.elapsed().as_micros());
        }
        Ok(&mut self.worker)
    }
}

impl Replayer<'_> {
    /// Φ after applying `ivs` (one oracle execution) on the worker.
    pub fn observe(
        &mut self,
        trace: &Trace,
        sink: &SinkSpec,
        ivs: &[Intervention],
        max_steps: u64,
    ) -> Result<SinkObservation> {
        let step = ivs.iter().map(|iv| iv.step).min().unwrap_or(0);
        let prefix = &trace.steps[..(step as usize).min(trace.steps.len())];
        let program = self.program;
        let run = self.position(step)?;
        Ok(drive(program, run, prefix, sink, ivs, max_steps, false)?.observation)
    }
}

/// Apply `ivs` (all at or after the run's step) and evaluate Φ.
fn drive(
    program: &Program,
    run: &mut Run,
    prefix: &[trace::StepRecord],
    sink: &SinkSpec,
    ivs: &[Intervention],
    max_steps: u64,
    to_end: bool,
) -> Result<Oracle> {
    let mut ordered: Vec<&Intervention> = ivs.iter().collect();
    ordered.sort();
    for iv in ordered {
        if iv.step > run.emu.inst_count {
            run.run_to_step(iv.step)?;
        }
        apply(program, run, iv)?;
    }
    let mut monitor = SinkMonitor::resumed(sink, prefix);
    let (observation, end) = trace::observe_fast(program, run, &mut monitor, max_steps, to_end);
    Ok(Oracle {
        digest: engine_digest(run, &end),
        observation,
    })
}

struct Runner<'c, 'p> {
    case: &'c Case<'p>,
    config: Config,
    trace: Trace,
    replay: Replayer<'p>,
    /// Holds only the initial checkpoint: verifications replay from step 0.
    verifier: Replayer<'p>,
    new_emulator_checks: u32,
    counts: Counts,
    timing: Timing,
    oracle_us: Vec<u128>,
    fresh_us: Vec<u128>,
    original_digest: String,
}

/// Outcome of one oracle execution.
struct Oracle {
    observation: SinkObservation,
    digest: String,
}

impl Runner<'_, '_> {
    fn budget_left(&self) -> bool {
        self.counts.oracle_executions < self.config.max_oracles
    }

    fn prefix_len(&self, step: u64) -> usize {
        (step as usize).min(self.trace.steps.len())
    }

    /// Replay with `ivs` from `step` and evaluate Φ. `fresh` replays from
    /// the initial state in a newly built emulator instead of restoring a
    /// checkpoint into the worker.
    fn oracle(
        &mut self,
        step: u64,
        ivs: &[Intervention],
        fresh: bool,
        to_end: bool,
    ) -> Result<Oracle> {
        let started = Instant::now();
        self.counts.oracle_executions += 1;
        let program = self.case.program;
        let prefix_len = self.prefix_len(step);
        let result = if fresh && self.new_emulator_checks == 0 {
            // The first verification of a case builds a brand-new emulator,
            // so nothing pooled can stand behind it.
            self.new_emulator_checks += 1;
            let mut run = program.fresh(Engine::Interpreter)?;
            self.fresh_us.push(started.elapsed().as_micros());
            if !run.run_to_step(step)? {
                anyhow::bail!("fresh replay could not reach step {step}");
            }
            drive(
                program,
                &mut run,
                &self.trace.steps[..prefix_len],
                &self.case.sink,
                ivs,
                self.config.max_steps,
                to_end,
            )?
        } else if fresh {
            // Later ones replay from the initial state on a second pooled
            // emulator that holds no checkpoint but the initial one, so the
            // fork checkpoints never stand behind a verification.
            let run = self.verifier.position(step)?;
            drive(
                program,
                run,
                &self.trace.steps[..prefix_len],
                &self.case.sink,
                ivs,
                self.config.max_steps,
                to_end,
            )?
        } else {
            let run = self.replay.position(step)?;
            drive(
                program,
                run,
                &self.trace.steps[..prefix_len],
                &self.case.sink,
                ivs,
                self.config.max_steps,
                to_end,
            )?
        };
        self.oracle_us.push(started.elapsed().as_micros());
        Ok(result)
    }

    /// The same interventions with the JIT finishing the run from the
    /// intervention point, for an engine-diverse end state.
    fn jit_digest(&mut self, step: u64, ivs: &[Intervention]) -> Result<String> {
        let program = self.case.program;
        let max_steps = self.config.max_steps;
        let run = self.replay.position(step)?;
        let mut ordered: Vec<&Intervention> = ivs.iter().collect();
        ordered.sort();
        for iv in ordered {
            if iv.step > run.emu.inst_count {
                run.run_to_step(iv.step)?;
            }
            apply(program, run, iv)?;
        }
        run.emu.force_interpreter = false;
        run.emu.max_inst = Some(max_steps);
        let end = finish_on_jit(run);
        let digest = engine_digest(run, &end);
        run.emu.max_inst = None;
        Ok(digest)
    }

    /// Confirm a flip from the initial state in a fresh emulator, then once
    /// more with the JIT finishing the run.
    fn verify(
        &mut self,
        step: u64,
        ivs: &[Intervention],
        expected: &SinkObservation,
    ) -> Result<(bool, String, SinkObservation)> {
        self.counts.fresh_verifications += 1;
        let fresh = self.oracle(step, ivs, true, true)?;
        let jit = self.jit_digest(step, ivs)?;
        let agrees = fresh.observation.holds() == expected.holds() && fresh.digest == jit;
        Ok((agrees, fresh.digest, fresh.observation))
    }

    fn capture_checkpoints(&mut self, steps: &BTreeSet<u64>) -> Result<()> {
        let started = Instant::now();
        self.replay.capture(steps)?;
        self.timing.checkpoint_capture_us = started.elapsed().as_micros();
        // A checkpoint is trusted only if restoring it and running on
        // unchanged reproduces the recorded run's Φ and end state.
        let original = self.trace.observation.holds();
        for step in self.replay.checkpoint_steps() {
            let null = self.oracle(step, &[], false, true)?;
            if null.observation.holds() != original || null.digest != self.original_digest {
                self.replay.forget(step);
                self.counts.checkpoints_invalid += 1;
            }
        }
        self.counts.checkpoints = self.replay.checkpoint_steps().len();
        Ok(())
    }

    /// Test one candidate location at one step with deterministic values.
    fn test_candidate(&mut self, cand: &Candidate, pool: &[u64]) -> Result<Verdict> {
        if !self.budget_left() {
            return Ok(Verdict::BudgetBounded {
                detail: format!("oracle budget of {} spent", self.config.max_oracles),
            });
        }
        let program = self.case.program;
        let probe = self.replay.position(cand.step)?;
        let original = match read_location(program, probe, &cand.location) {
            Ok(v) => v,
            Err(e) => {
                return Ok(Verdict::Unsupported {
                    reason: format!("cannot read {:?}: {e}", cand.location),
                });
            }
        };
        let values = slice::alternatives(&original, pool, self.config.values_per_candidate);
        let mut checked = Vec::new();
        for value in values {
            let iv = Intervention {
                step: cand.step,
                location: cand.location.clone(),
                value,
            };
            if let Err(reason) = validate(program, probe, &self.trace, &iv) {
                return Ok(Verdict::InvalidIntervention { reason });
            }
            checked.push(iv);
        }
        let mut tried = Vec::new();
        for iv in checked {
            if !self.budget_left() {
                return Ok(Verdict::BudgetBounded {
                    detail: format!("oracle budget spent after {} values", tried.len()),
                });
            }
            let result = self.oracle(cand.step, std::slice::from_ref(&iv), false, false)?;
            tried.push(iv.value.clone());
            match result.observation.holds() {
                None => {
                    return Ok(Verdict::BudgetBounded {
                        detail: format!("replay undecided: {:?}", result.observation),
                    });
                }
                Some(h) if Some(h) != self.trace.observation.holds() => {
                    return self.witness(&iv, original, result.observation);
                }
                Some(_) => {}
            }
        }
        Ok(Verdict::NotACause { tried })
    }

    fn witness(
        &mut self,
        iv: &Intervention,
        original: Vec<u8>,
        counterfactual: SinkObservation,
    ) -> Result<Verdict> {
        let (agrees, digest, fresh_obs) =
            self.verify(iv.step, std::slice::from_ref(iv), &counterfactual)?;
        if !agrees {
            return Ok(Verdict::NonDeterministic {
                reason: format!(
                    "fork replay said {counterfactual:?}, fresh replay said {fresh_obs:?} \
                     (or the JIT finished the run differently)"
                ),
            });
        }
        let pc = self.trace.step(iv.step).map_or(0, |s| s.pc);
        Ok(Verdict::CounterfactualWitness {
            witness: InterventionWitness {
                step: iv.step,
                pc,
                location: iv.location.clone(),
                original_bytes: original,
                intervened_bytes: iv.value.clone(),
                original_sink: self.trace.observation.clone(),
                counterfactual_sink: fresh_obs,
                counterfactual_trace_blake3: digest,
                fresh_replay_agrees: agrees,
            },
        })
    }

    fn judge_requested(&mut self, iv: &Intervention) -> Result<CandidateResult> {
        let pc = self.trace.step(iv.step).map_or(0, |s| s.pc);
        let input_offset = match iv.location {
            Location::Memory { addr, len: 1 } => slice::input_offset_at(&self.trace, addr, iv.step),
            _ => None,
        };
        let verdict = if iv.step >= self.trace.steps.len() as u64 {
            Verdict::InvalidIntervention {
                reason: InvalidReason::NotBeforeSink {
                    step: iv.step,
                    sink_step: self.trace.sink_step,
                },
            }
        } else {
            let program = self.case.program;
            let probe = self.replay.position(iv.step)?;
            match validate(program, probe, &self.trace, iv) {
                Err(reason) => Verdict::InvalidIntervention { reason },
                Ok(original) => {
                    let result = self.oracle(iv.step, std::slice::from_ref(iv), false, false)?;
                    match result.observation.holds() {
                        None => Verdict::BudgetBounded {
                            detail: format!("{:?}", result.observation),
                        },
                        Some(h) if Some(h) != self.trace.observation.holds() => {
                            self.witness(iv, original, result.observation)?
                        }
                        Some(_) => Verdict::NotACause {
                            tried: vec![iv.value.clone()],
                        },
                    }
                }
            }
        };
        Ok(CandidateResult {
            step: iv.step,
            pc,
            location: iv.location.clone(),
            origin: CandidateOrigin::Requested,
            input_offset,
            verdict,
        })
    }

    /// Environment sources: refuse if Φ reaches or depends on a
    /// non-deterministic one.
    fn environment(&mut self, slice: &Slice) -> Result<(Vec<EnvSource>, Option<String>)> {
        let mut sources = Vec::new();
        let mut refusal = None;
        for (step, name, class) in self.trace.environment_reads() {
            if class == EnvClass::HostStable {
                sources.push(EnvSource {
                    step,
                    syscall: name,
                    class,
                    status: EnvSourceStatus::Recorded,
                });
                continue;
            }
            let status = if slice.syscall_steps.contains(&step) {
                EnvSourceStatus::InSinkSlice
            } else if self.perturbation_changes_sink(step)? {
                EnvSourceStatus::PerturbationChangesSink
            } else {
                EnvSourceStatus::OutsideSliceStable
            };
            if status != EnvSourceStatus::OutsideSliceStable && refusal.is_none() {
                refusal = Some(format!(
                    "Φ depends on `{name}` at step {step} ({status:?}); its value differs \
                     between native runs and the replay only reproduces the emulator's stub"
                ));
            }
            sources.push(EnvSource {
                step,
                syscall: name,
                class,
                status,
            });
        }
        Ok((sources, refusal))
    }

    /// Does any tried value of the syscall's outputs, set right after it,
    /// change Φ?
    fn perturbation_changes_sink(&mut self, step: u64) -> Result<bool> {
        let Some(record) = self.trace.step(step) else {
            return Ok(false);
        };
        let after = step + 1;
        if after >= self.trace.sink_step {
            return Ok(false);
        }
        let mut locations = Vec::new();
        for def in &record.defs {
            match *def {
                Access::Reg { offset, size } => {
                    if let Some(l) = slice::register_location(self.case.program, offset, size) {
                        locations.push(l);
                    }
                }
                Access::Mem { addr, len } => {
                    for i in 0..u64::from(len) {
                        locations.push(Location::Memory {
                            addr: addr + i,
                            len: 1,
                        });
                    }
                }
            }
        }
        for location in locations {
            let cand = Candidate {
                step: after,
                location,
                origin: CandidateOrigin::EnvironmentSource,
                input_offset: None,
            };
            if let Verdict::CounterfactualWitness { .. } = self.test_candidate(&cand, &[])? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Bounded pair search over input bytes. Partners come from the
    /// recorded slice and from the slice of each single counterfactual
    /// run (dual slicing), so a byte the recorded run never read can pair.
    fn joint_search(&mut self, singles: &[CandidateResult], slice: &Slice) -> Result<JointStatus> {
        let inputs: Vec<&CandidateResult> = singles
            .iter()
            .filter(|c| c.origin == CandidateOrigin::InputByte)
            .collect();
        let program = self.case.program;
        let mut pairs_tried = 0u64;
        for first in inputs {
            let Verdict::NotACause { tried } = &first.verdict else {
                continue;
            };
            let Some(first_value) = tried.first().cloned() else {
                continue;
            };
            let first_iv = Intervention {
                step: first.step,
                location: first.location.clone(),
                value: first_value,
            };
            // The counterfactual run's own slice.
            self.counts.oracle_executions += 1;
            let run = self.replay.position(0)?;
            let cf = trace::record_on(
                program,
                run,
                &self.case.sink,
                std::slice::from_ref(&first_iv),
                self.config.max_steps,
            )?;
            let cf_potential = self
                .config
                .potential_dependence
                .then(|| slice::Potential::of(program, &cf));
            let cf_slice = slice::backward_with(
                &cf,
                cf.sink_step,
                &cf.criterion,
                self.config.control_dependence,
                cf_potential.as_ref(),
            );
            let mut partners: BTreeSet<usize> = cf_slice.input_offsets.clone();
            partners.extend(slice.input_offsets.iter().copied());
            partners.remove(&first.input_offset.unwrap_or(usize::MAX));
            for offset in partners {
                let Some(byte) = self
                    .trace
                    .inputs
                    .iter()
                    .find(|b| b.offset == offset)
                    .copied()
                else {
                    continue;
                };
                let step = first.step.max(
                    self.trace
                        .steps
                        .iter()
                        .find(|s| s.input.iter().any(|b| b.offset == offset))
                        .map_or(0, |s| s.step + 1),
                );
                let location = Location::Memory {
                    addr: byte.addr,
                    len: 1,
                };
                let probe = self.replay.position(step)?;
                let original = read_location(program, probe, &location)?;
                for value in slice::alternatives(&original, &[], 2) {
                    if pairs_tried >= self.config.max_pairs || !self.budget_left() {
                        return Ok(JointStatus::NoWitnessWithinBudget { pairs_tried });
                    }
                    pairs_tried += 1;
                    let second_iv = Intervention {
                        step,
                        location: location.clone(),
                        value,
                    };
                    let ivs = vec![first_iv.clone(), second_iv.clone()];
                    let start = first_iv.step.min(step);
                    let result = self.oracle(start, &ivs, false, false)?;
                    if result.observation.holds().is_some()
                        && result.observation.holds() != self.trace.observation.holds()
                    {
                        let (agrees, _, _) = self.verify(start, &ivs, &result.observation)?;
                        if agrees {
                            return Ok(JointStatus::JointWitness {
                                interventions: ivs,
                                input_offsets: vec![first.input_offset, Some(offset)],
                            });
                        }
                    }
                }
            }
        }
        Ok(JointStatus::NoWitnessWithinBudget { pairs_tried })
    }
}

/// Run to the end on the JIT, through any sink breakpoints the run carries.
fn finish_on_jit(run: &mut Run) -> EndState {
    run.emu.force_interpreter = false;
    loop {
        match run.emu.resume() {
            Ok(fission_emulator::RunOutcome::HitBreakpoint(_)) => {}
            Ok(_) if run.emu.halt_requested => return trace::halted_end(run),
            Ok(_) => return EndState::Budget,
            Err(e) => {
                return EndState::Faulted {
                    pc: fault_pc(&e.to_string()).unwrap_or(run.emu.pc),
                    message: e.to_string(),
                };
            }
        }
    }
}

/// Did the recorded run write any executable page?
fn runner_trace_writes_code(program: &Program, trace: &Trace) -> bool {
    let Ok(probe) = program.fresh(Engine::Interpreter) else {
        return true;
    };
    trace.steps.iter().any(|s| {
        s.defs.iter().any(|d| match *d {
            Access::Mem { addr, len } => {
                probe.emu.state.page_map.range_has_exec(addr, len as usize)
            }
            Access::Reg { .. } => false,
        })
    })
}

fn fault_pc(message: &str) -> Option<u64> {
    trace::guest_fault_pc(message)
}

/// Run one case end to end.
pub fn run_case(case: &Case<'_>, config: &Config) -> Result<CaseReport> {
    let total = Instant::now();
    let program = case.program;
    let started = Instant::now();
    let recorded = trace::record(program, &case.sink, &[], config.max_steps)?;
    let record_us = started.elapsed().as_micros();

    // Reproducibility: the recording and two fresh JIT replays.
    let recorded_digest = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(end_label(&recorded.end).as_bytes());
        hasher.update(&recorded.stdout);
        hasher.finalize().to_hex().to_string()
    };
    let mut jit_digests = Vec::new();
    for _ in 0..2 {
        let mut run = program.fresh(Engine::Jit)?;
        run.emu.max_inst = Some(config.max_steps);
        let end = finish_on_jit(&mut run);
        jit_digests.push(engine_digest(&run, &end));
    }
    let determinism = Determinism {
        replays_agree: jit_digests[0] == jit_digests[1],
        engines_agree: jit_digests[0] == recorded_digest,
        recorded_end: end_label(&recorded.end),
        replay_end: jit_digests[0].clone(),
    };

    let potential = config
        .potential_dependence
        .then(|| slice::Potential::of(program, &recorded));
    let slice = slice::backward_with(
        &recorded,
        recorded.sink_step,
        &recorded.criterion,
        config.control_dependence,
        potential.as_ref(),
    );
    let mut counts = Counts {
        recorded_steps: recorded.steps.len() as u64,
        sink_step: recorded.sink_step,
        slice_steps: slice.steps.len(),
        slice_static_instructions: slice
            .steps
            .iter()
            .filter_map(|s| recorded.step(*s).map(|r| r.pc))
            .collect::<BTreeSet<_>>()
            .len(),
        slice_input_bytes: slice.input_offsets.len(),
        ..Counts::default()
    };
    let mut generated = slice::candidates(program, &recorded, &slice);
    counts.candidates_generated = generated.len();
    generated.truncate(config.max_candidates);
    let mut pool: Vec<u64> = slice
        .steps
        .iter()
        .filter_map(|s| recorded.step(*s))
        .flat_map(|s| s.compare_consts.iter().copied())
        .collect();
    pool.sort_unstable();
    pool.dedup();
    pool.truncate(8);

    // Forks finish on the JIT and verifications on the interpreter, so a
    // verified witness has been seen by both engines.
    let smc = runner_trace_writes_code(program, &recorded);
    let replay = Replayer::new(program, Engine::Jit, smc)?;
    let verifier = Replayer::new(program, Engine::Interpreter, smc)?;
    let mut runner = Runner {
        case,
        config: config.clone(),
        trace: recorded,
        replay,
        verifier,
        new_emulator_checks: 0,
        counts,
        timing: Timing {
            record_us,
            ..Timing::default()
        },
        oracle_us: Vec::new(),
        fresh_us: Vec::new(),
        original_digest: recorded_digest.clone(),
    };

    let mut refusal = None;
    if !determinism.replays_agree || !determinism.engines_agree {
        refusal = Some(format!(
            "replay is not reproducible: recording ended {}, JIT replays {} / {}",
            determinism.recorded_end, jit_digests[0], jit_digests[1]
        ));
    }
    if runner.trace.observation.holds().is_none() {
        refusal = Some(format!(
            "Φ undecided on the recorded run: {:?}",
            runner.trace.observation
        ));
    }

    if config.use_checkpoints && refusal.is_none() {
        let mut wanted: BTreeSet<u64> = generated.iter().map(|c| c.step).collect();
        // Thin to at most one checkpoint per `checkpoint_spacing` steps;
        // the rest of the way is replayed from the closest one.
        let mut steps: BTreeSet<u64> = BTreeSet::new();
        let mut last: Option<u64> = None;
        for s in std::mem::take(&mut wanted) {
            if last.is_none_or(|l| s >= l + config.checkpoint_spacing) {
                steps.insert(s);
                last = Some(s);
            }
        }
        steps.extend(case.requested.iter().map(|iv| iv.step));
        steps.retain(|s| *s < runner.trace.steps.len() as u64);
        runner
            .capture_checkpoints(&steps)
            .context("capturing checkpoints")?;
    }

    let (sources, env_refusal) = if refusal.is_none() {
        runner.environment(&slice).context("environment sources")?
    } else {
        (Vec::new(), None)
    };
    if refusal.is_none() {
        refusal = env_refusal;
    }

    let mut candidates = Vec::new();
    let mut requested = Vec::new();
    let joint;
    if let Some(reason) = &refusal {
        for iv in &case.requested {
            requested.push(CandidateResult {
                step: iv.step,
                pc: runner.trace.step(iv.step).map_or(0, |s| s.pc),
                location: iv.location.clone(),
                origin: CandidateOrigin::Requested,
                input_offset: None,
                verdict: Verdict::NonDeterministic {
                    reason: reason.clone(),
                },
            });
        }
        joint = JointStatus::Refused;
    } else {
        for cand in &generated {
            let verdict = runner
                .test_candidate(cand, &pool)
                .with_context(|| format!("candidate {cand:?}"))?;
            runner.counts.candidates_tested += 1;
            candidates.push(CandidateResult {
                step: cand.step,
                pc: runner.trace.step(cand.step).map_or(0, |s| s.pc),
                location: cand.location.clone(),
                origin: cand.origin,
                input_offset: cand.input_offset,
                verdict,
            });
        }
        for iv in &case.requested {
            requested.push(
                runner
                    .judge_requested(iv)
                    .with_context(|| format!("requested {iv:?}"))?,
            );
        }
        // Explanations are scored at input-byte granularity, so the pair
        // search runs whenever no single input byte is a witness, even if
        // some internal location (a read length, a saved register) is.
        let any_witness = candidates.iter().any(|c| {
            c.input_offset.is_some() && matches!(c.verdict, Verdict::CounterfactualWitness { .. })
        });
        joint = if any_witness {
            JointStatus::NotNeeded
        } else {
            runner
                .joint_search(&candidates, &slice)
                .context("joint search")?
        };
    }

    let witness_inputs: Vec<usize> = candidates
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::CounterfactualWitness { .. }))
        .filter_map(|c| c.input_offset)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mean = |v: &[u128]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<u128>() as f64 / v.len() as f64
        }
    };
    runner.timing.checkpoint_restore_us_mean = mean(&runner.replay.restore_us);
    runner.timing.replay_to_step_us_mean = mean(&runner.replay.replay_us);
    runner.timing.fresh_emulator_us_mean = mean(&runner.fresh_us);
    runner.timing.worker_construction_us = runner.replay.construction_us;
    runner.timing.oracle_us_mean = mean(&runner.oracle_us);
    runner.timing.total_us = total.elapsed().as_micros();

    Ok(CaseReport {
        contract: CONTRACT_VERSION.to_string(),
        case: case.name.clone(),
        fingerprint: program.fingerprint(sources),
        sink: case.sink.clone(),
        original: runner.trace.observation.clone(),
        end: end_label(&runner.trace.end),
        determinism,
        refusal,
        candidates,
        requested,
        joint,
        witness_inputs,
        counts: runner.counts.clone(),
        config: config.clone(),
        timing: Some(runner.timing.clone()),
    })
}

/// The recorded trace and its slice, for baselines that want them.
pub fn record_and_slice(
    program: &Program,
    sink: &SinkSpec,
    config: &Config,
) -> Result<(Trace, Slice)> {
    let recorded = trace::record(program, sink, &[], config.max_steps)?;
    let potential = config
        .potential_dependence
        .then(|| slice::Potential::of(program, &recorded));
    let slice = slice::backward_with(
        &recorded,
        recorded.sink_step,
        &recorded.criterion,
        config.control_dependence,
        potential.as_ref(),
    );
    Ok((recorded, slice))
}
