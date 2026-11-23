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

use anyhow::{Result, anyhow};
use fission_emulator::MachineState;
use fission_emulator::os::linux::image_info::ImageInfo;
use fission_emulator::os::linux::signal::SignalState;
use fission_emulator::os::vfs::SimVFS;
use serde::Serialize;

use crate::contract::{
    CONTRACT_VERSION, CandidateOrigin, CandidateResult, EnvClass, EnvSource, EnvSourceStatus,
    Intervention, InterventionWitness, InvalidReason, JointStatus, Location, ReplayFingerprint,
    SinkObservation, SinkSpec, Verdict,
};
use crate::machine::{Engine, Program, Run};
use crate::slice::{self, Candidate, Slice};
use crate::trace::{self, Access, EndState, SinkMonitor, Trace};

/// Budgets and switches for one case.
#[derive(Debug, Clone, Serialize)]
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
    /// Restore checkpoints instead of replaying from the initial state.
    pub use_checkpoints: bool,
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
            use_checkpoints: true,
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
#[derive(Debug, Clone, Serialize)]
pub struct Determinism {
    /// Two fresh JIT replays ended identically.
    pub replays_agree: bool,
    /// ...and identically to the interpreter recording.
    pub engines_agree: bool,
    pub recorded_end: String,
    pub replay_end: String,
}

/// Deterministic counts.
#[derive(Debug, Clone, Default, Serialize)]
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
#[derive(Debug, Clone, Default, Serialize)]
pub struct Timing {
    pub record_us: u128,
    pub checkpoint_capture_us: u128,
    pub checkpoint_restore_us_mean: f64,
    pub fresh_position_us_mean: f64,
    pub oracle_us_mean: f64,
    pub total_us: u128,
}

#[derive(Debug, Clone, Serialize)]
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
    vfs: SimVFS,
    stdin_buffer: Option<Vec<u8>>,
    exit_code: Option<u32>,
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
            vfs: emu.vfs.clone(),
            stdin_buffer: emu.stdin_buffer.clone(),
            exit_code: emu.exit_code,
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
    fn restore(&self, run: &mut Run) {
        let emu = &mut run.emu;
        let hooks = std::mem::take(&mut emu.state.hooks);
        emu.state = self.state.clone();
        emu.state.hooks = hooks;
        emu.state.normalize_host_reg_file();
        emu.state.invalidate_reg_cache();
        emu.jit_cache.flush_all();
        emu.pc = self.pc;
        emu.inst_count = self.step;
        emu.vfs = self.vfs.clone();
        emu.stdin_buffer = self.stdin_buffer.clone();
        emu.exit_code = self.exit_code;
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

struct Runner<'c, 'p> {
    case: &'c Case<'p>,
    config: Config,
    trace: Trace,
    checkpoints: BTreeMap<u64, Checkpoint>,
    counts: Counts,
    timing: Timing,
    restore_us: Vec<u128>,
    position_us: Vec<u128>,
    oracle_us: Vec<u128>,
    original_digest: String,
}

/// Outcome of one oracle execution.
struct Oracle {
    observation: SinkObservation,
    digest: String,
}

impl Runner<'_, '_> {
    fn program(&self) -> &Program {
        self.case.program
    }

    fn budget_left(&self) -> bool {
        self.counts.oracle_executions < self.config.max_oracles
    }

    /// A run positioned at `step`: restored from a checkpoint, or replayed
    /// from the initial state.
    fn position(&mut self, step: u64, fresh: bool) -> Result<Run> {
        let started = Instant::now();
        let mut run = self.program().fresh(Engine::Interpreter)?;
        if !fresh && let Some(cp) = self.checkpoints.get(&step) {
            cp.restore(&mut run);
            self.restore_us.push(started.elapsed().as_micros());
        } else {
            if !run.run_to_step(step)? {
                anyhow::bail!("could not reach step {step}");
            }
            self.position_us.push(started.elapsed().as_micros());
        }
        Ok(run)
    }

    /// Replay with `ivs` (all at or after `step`) and evaluate Φ.
    fn oracle(
        &mut self,
        step: u64,
        ivs: &[Intervention],
        fresh: bool,
        to_end: bool,
    ) -> Result<Oracle> {
        let started = Instant::now();
        self.counts.oracle_executions += 1;
        let mut run = self.position(step, fresh)?;
        let mut ordered: Vec<&Intervention> = ivs.iter().collect();
        ordered.sort();
        for iv in ordered {
            if iv.step > run.emu.inst_count {
                run.run_to_step(iv.step)?;
            }
            apply(self.case.program, &mut run, iv)?;
        }
        let prefix = &self.trace.steps[..(step as usize).min(self.trace.steps.len())];
        let mut monitor = SinkMonitor::resumed(&self.case.sink, prefix);
        let (observation, end) = trace::observe(
            self.case.program,
            &mut run,
            &mut monitor,
            self.config.max_steps,
            to_end,
        );
        let digest = engine_digest(&run, &end);
        self.oracle_us.push(started.elapsed().as_micros());
        Ok(Oracle {
            observation,
            digest,
        })
    }

    /// The same interventions finished by the JIT from the intervention
    /// point, for an engine-diverse end state.
    fn jit_digest(&mut self, step: u64, ivs: &[Intervention]) -> Result<String> {
        let mut run = self.program().fresh(Engine::Interpreter)?;
        let mut ordered: Vec<&Intervention> = ivs.iter().collect();
        ordered.sort();
        if !run.run_to_step(step)? {
            anyhow::bail!("could not reach step {step}");
        }
        for iv in ordered {
            if iv.step > run.emu.inst_count {
                run.run_to_step(iv.step)?;
            }
            apply(self.case.program, &mut run, iv)?;
        }
        run.emu.force_interpreter = false;
        run.emu.max_inst = Some(self.config.max_steps);
        let end = match run.emu.resume() {
            Ok(_) if run.emu.halt_requested => EndState::Exited {
                code: run.emu.exit_code,
            },
            Ok(_) => EndState::Budget,
            Err(e) => EndState::Faulted {
                pc: fault_pc(&e.to_string()).unwrap_or(run.emu.pc),
                message: e.to_string(),
            },
        };
        Ok(engine_digest(&run, &end))
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
        let mut run = self.program().fresh(Engine::Interpreter)?;
        for &step in steps {
            if !run.run_to_step(step)? {
                break;
            }
            self.checkpoints.insert(step, Checkpoint::capture(&mut run));
        }
        self.timing.checkpoint_capture_us = started.elapsed().as_micros();
        self.counts.checkpoints = self.checkpoints.len();
        // A checkpoint is trusted only if restoring it and running on
        // unchanged reproduces the recorded run's Φ and end state.
        let original = self.trace.observation.holds();
        let steps: Vec<u64> = self.checkpoints.keys().copied().collect();
        for step in steps {
            let null = self.oracle(step, &[], false, true)?;
            if null.observation.holds() != original || null.digest != self.original_digest {
                self.checkpoints.remove(&step);
                self.counts.checkpoints_invalid += 1;
            }
        }
        Ok(())
    }

    /// Test one candidate location at one step with deterministic values.
    fn test_candidate(&mut self, cand: &Candidate, pool: &[u64]) -> Result<Verdict> {
        if !self.budget_left() {
            return Ok(Verdict::BudgetBounded {
                detail: format!("oracle budget of {} spent", self.config.max_oracles),
            });
        }
        let mut probe = self.position(cand.step, false)?;
        let original = match read_location(self.case.program, &mut probe, &cand.location) {
            Ok(v) => v,
            Err(e) => {
                return Ok(Verdict::Unsupported {
                    reason: format!("cannot read {:?}: {e}", cand.location),
                });
            }
        };
        let values = slice::alternatives(&original, pool, self.config.values_per_candidate);
        let mut tried = Vec::new();
        for value in values {
            let iv = Intervention {
                step: cand.step,
                location: cand.location.clone(),
                value: value.clone(),
            };
            if let Err(reason) = validate(self.case.program, &mut probe, &self.trace, &iv) {
                return Ok(Verdict::InvalidIntervention { reason });
            }
            if !self.budget_left() {
                return Ok(Verdict::BudgetBounded {
                    detail: format!("oracle budget spent after {} values", tried.len()),
                });
            }
            let result = self.oracle(cand.step, std::slice::from_ref(&iv), false, false)?;
            tried.push(value.clone());
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
        let verdict = if iv.step > self.trace.steps.len() as u64 {
            Verdict::InvalidIntervention {
                reason: InvalidReason::NotBeforeSink {
                    step: iv.step,
                    sink_step: self.trace.sink_step,
                },
            }
        } else {
            let mut probe = self.position(iv.step, false)?;
            match validate(self.case.program, &mut probe, &self.trace, iv) {
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
            let cf = trace::record(
                self.case.program,
                &self.case.sink,
                std::slice::from_ref(&first_iv),
                self.config.max_steps,
            )?;
            self.counts.oracle_executions += 1;
            let cf_slice = slice::backward(
                &cf,
                cf.sink_step,
                &cf.criterion,
                self.config.control_dependence,
            );
            let mut partners: BTreeSet<usize> = cf_slice.input_offsets.clone();
            partners.extend(slice.input_offsets.iter().copied());
            partners.remove(&first.input_offset.unwrap_or(usize::MAX));
            for offset in partners {
                let Some(byte) = self.trace.inputs.iter().find(|b| b.offset == offset).copied()
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
                let mut probe = self.position(step, false)?;
                let original = read_location(self.case.program, &mut probe, &location)?;
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

fn fault_pc(message: &str) -> Option<u64> {
    let at = message.find("guest PC 0x")? + "guest PC 0x".len();
    let hex: String = message[at..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    u64::from_str_radix(&hex, 16).ok()
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
        let end = match run.emu.resume() {
            Ok(_) if run.emu.halt_requested => EndState::Exited {
                code: run.emu.exit_code,
            },
            Ok(_) => EndState::Budget,
            Err(e) => EndState::Faulted {
                pc: fault_pc(&e.to_string()).unwrap_or(run.emu.pc),
                message: e.to_string(),
            },
        };
        jit_digests.push(engine_digest(&run, &end));
    }
    let determinism = Determinism {
        replays_agree: jit_digests[0] == jit_digests[1],
        engines_agree: jit_digests[0] == recorded_digest,
        recorded_end: end_label(&recorded.end),
        replay_end: jit_digests[0].clone(),
    };

    let slice = slice::backward(
        &recorded,
        recorded.sink_step,
        &recorded.criterion,
        config.control_dependence,
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

    let mut runner = Runner {
        case,
        config: config.clone(),
        trace: recorded,
        checkpoints: BTreeMap::new(),
        counts,
        timing: Timing {
            record_us,
            ..Timing::default()
        },
        restore_us: Vec::new(),
        position_us: Vec::new(),
        oracle_us: Vec::new(),
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
        let mut steps: BTreeSet<u64> = generated.iter().map(|c| c.step).collect();
        steps.extend(case.requested.iter().map(|iv| iv.step));
        steps.retain(|s| *s < runner.trace.steps.len() as u64);
        runner.capture_checkpoints(&steps)?;
    }

    let (sources, env_refusal) = if refusal.is_none() {
        runner.environment(&slice)?
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
            let verdict = runner.test_candidate(cand, &pool)?;
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
            requested.push(runner.judge_requested(iv)?);
        }
        let any_witness = candidates
            .iter()
            .any(|c| matches!(c.verdict, Verdict::CounterfactualWitness { .. }));
        joint = if any_witness {
            JointStatus::NotNeeded
        } else {
            runner.joint_search(&candidates, &slice)?
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
    runner.timing.checkpoint_restore_us_mean = mean(&runner.restore_us);
    runner.timing.fresh_position_us_mean = mean(&runner.position_us);
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
pub fn record_and_slice(program: &Program, sink: &SinkSpec, config: &Config) -> Result<(Trace, Slice)> {
    let recorded = trace::record(program, sink, &[], config.max_steps)?;
    let slice = slice::backward(
        &recorded,
        recorded.sink_step,
        &recorded.criterion,
        config.control_dependence,
    );
    Ok((recorded, slice))
}
