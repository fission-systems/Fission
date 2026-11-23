//! The recorded run: one record per retired instruction, with the register
//! and memory bytes it read and wrote, its control transfer, the syscalls it
//! made, and the input bytes it delivered.
//!
//! Recording single-steps the interpreter, which retires exactly one
//! instruction per step. Register reads and writes come from the
//! instruction's p-code; memory reads and writes are what the emulator
//! actually did (its memory trace), so addresses are concrete.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use fission_emulator::RunOutcome;
use fission_emulator::os::linux::syscall_conv::SyscallAbi;
use fission_pcode_ir::ir::PcodeOpcode;

use crate::cfg::{CodeIndex, Flow, flow_of};
use crate::contract::{EnvClass, MemPredicate, SinkObservation, SinkPoint, SinkSpec};
use crate::machine::{Engine, Program, Run, SyscallEvent};

/// Instruction fetches read this many bytes; data reads of exactly this
/// size from executable pages are treated as fetches and dropped.
const FETCH_BYTES: usize = 16;

/// One byte of machine state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ByteLoc {
    /// Register-space byte offset.
    Reg(u64),
    /// RAM byte address.
    Mem(u64),
}

/// A contiguous access: registers as varnodes, memory as observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    Reg { offset: u64, size: u32 },
    Mem { addr: u64, len: u32 },
}

impl Access {
    pub fn bytes(&self) -> impl Iterator<Item = ByteLoc> {
        let (base, len, reg) = match *self {
            Self::Reg { offset, size } => (offset, size, true),
            Self::Mem { addr, len } => (addr, len, false),
        };
        (0..u64::from(len)).map(move |i| {
            if reg {
                ByteLoc::Reg(base + i)
            } else {
                ByteLoc::Mem(base.wrapping_add(i))
            }
        })
    }
}

/// A delivered input byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputByte {
    pub addr: u64,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct StepRecord {
    /// Retired instructions before this one.
    pub step: u64,
    pub pc: u64,
    pub uses: Vec<Access>,
    pub defs: Vec<Access>,
    pub flow: Flow,
    /// The program counter after the step.
    pub next_pc: u64,
    pub syscalls: Vec<SyscallEvent>,
    /// Input bytes this step delivered (a read of the input stream).
    pub input: Vec<InputByte>,
    /// Constants this instruction compared against.
    pub compare_consts: Vec<u64>,
    /// Dynamic control parent: the step of the conditional branch (or call)
    /// this step's execution depends on.
    pub cd_parent: Option<u64>,
    /// Registers that fed a LOAD or STORE address in this instruction.
    pub address_regs: Vec<u64>,
    /// Constant address of a LOAD/STORE whose address p-code is a
    /// constant, by access order.
    pub direct_addresses: Vec<Option<u64>>,
}

/// How the recorded run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndState {
    Exited {
        code: Option<u32>,
    },
    Faulted {
        pc: u64,
        message: String,
    },
    /// The recording stopped at the sink event.
    StoppedAtSink,
    Budget,
}

pub struct Trace {
    pub steps: Vec<StepRecord>,
    /// The step at which Φ was decided: the sink instruction itself for
    /// call/branch/fault/pc-hit sinks, or one past the last step for exit.
    pub sink_step: u64,
    /// Where the sink criterion lives at `sink_step`.
    pub criterion: Vec<Access>,
    pub observation: SinkObservation,
    pub end: EndState,
    pub inputs: Vec<InputByte>,
    pub stdout: Vec<u8>,
}

impl Trace {
    pub fn step(&self, step: u64) -> Option<&StepRecord> {
        self.steps.get(usize::try_from(step).ok()?)
    }

    /// `(syscall name, step, class)` for every environment-derived value
    /// the run consumed.
    pub fn environment_reads(&self) -> Vec<(u64, String, EnvClass)> {
        let mut out = Vec::new();
        for s in &self.steps {
            for call in &s.syscalls {
                let name = call.name.unwrap_or("unknown");
                if let Some(class) = environment_class(name, call) {
                    out.push((s.step, name.to_string(), class));
                }
            }
        }
        out
    }
}

/// Which syscalls hand the guest a value from outside the recorded input
/// stream. Classified by name, so the table is the same on every ISA.
pub fn environment_class(name: &str, call: &SyscallEvent) -> Option<EnvClass> {
    match name {
        "time" | "gettimeofday" | "clock_gettime" | "clock_getres" | "times" | "getrandom"
        | "getpid" | "gettid" | "getppid" | "sysinfo" | "set_tid_address" => {
            Some(EnvClass::NonDeterministic)
        }
        "getuid" | "geteuid" | "getgid" | "getegid" | "uname" | "getcwd" => {
            Some(EnvClass::HostStable)
        }
        // A read of anything but the recorded standard input reads the
        // host, which the fingerprint does not pin.
        "read" | "pread64" | "readv" if call.args[0] != 0 => Some(EnvClass::NonDeterministic),
        "open" | "openat" => Some(EnvClass::NonDeterministic),
        _ => None,
    }
}

struct DynamicControl<'a, 'p> {
    code: &'a mut CodeIndex<'p>,
    /// One frame per activation: (base parent, stack of (branch step,
    /// reconvergence pc)). `None` reconvergence is "until return".
    frames: Vec<(Option<u64>, Vec<(u64, Option<u64>)>)>,
}

impl DynamicControl<'_, '_> {
    /// The control parent of the step about to run at `pc`: the innermost
    /// open branch region of this activation, else the call that opened
    /// the activation.
    fn parent(&mut self, pc: u64) -> Option<u64> {
        let frame = self.frames.last_mut()?;
        while let Some(&(_, join)) = frame.1.last() {
            if join == Some(pc) {
                frame.1.pop();
            } else {
                break;
            }
        }
        frame.1.last().map(|&(step, _)| step).or(frame.0)
    }

    fn after(&mut self, step: u64, pc: u64, flow: &Flow) {
        match flow {
            Flow::CBranch { .. } => {
                let join = self.code.ipdom(pc).flatten();
                if let Some(frame) = self.frames.last_mut() {
                    frame.1.push((step, join));
                }
            }
            Flow::Call { .. } => self.frames.push((Some(step), Vec::new())),
            Flow::Return => {
                if self.frames.len() > 1 {
                    self.frames.pop();
                }
            }
            _ => {}
        }
    }
}

/// Evaluates Φ while a run is single-stepped. Recording and every replay
/// use this one monitor, so Φ means the same thing in both.
pub struct SinkMonitor<'s> {
    sink: &'s SinkSpec,
    occurrences: BTreeMap<u64, u32>,
    /// `(step, criterion, observation)` once Φ is decided.
    pub decided: Option<(u64, Vec<Access>, SinkObservation)>,
}

impl<'s> SinkMonitor<'s> {
    pub fn new(sink: &'s SinkSpec) -> Self {
        Self {
            sink,
            occurrences: BTreeMap::new(),
            decided: None,
        }
    }

    /// A monitor that has already seen the recorded prefix `steps`, so
    /// occurrence counts continue from a checkpoint.
    pub fn resumed(sink: &'s SinkSpec, prefix: &[StepRecord]) -> Self {
        let mut monitor = Self::new(sink);
        for s in prefix {
            *monitor.occurrences.entry(s.pc).or_default() += 1;
        }
        monitor
    }

    /// Before the instruction at `pc` (step `step`) runs.
    pub fn before(&mut self, program: &Program, run: &mut Run, pc: u64, step: u64) {
        let occurrence = {
            let n = self.occurrences.entry(pc).or_default();
            *n += 1;
            *n
        };
        if self.decided.is_some() {
            return;
        }
        match self.sink {
            SinkSpec::CallTo { target, args } if *target == pc => {
                let actual = read_args(run, args.len());
                let matches = args
                    .iter()
                    .zip(&actual)
                    .all(|(want, got)| want.is_none_or(|w| w == *got));
                if matches {
                    let criterion = call_criterion(program, run, args);
                    self.decided = Some((
                        step,
                        criterion,
                        SinkObservation::Holds {
                            detail: format!("call to 0x{target:x} with args {actual:x?}"),
                        },
                    ));
                }
            }
            SinkSpec::MemoryValue {
                addr,
                len,
                predicate,
                at,
            } => {
                let due = match at {
                    SinkPoint::Step { step: s } => *s == step,
                    SinkPoint::PcHit {
                        pc: p,
                        occurrence: o,
                    } => *p == pc && *o == occurrence,
                    SinkPoint::Exit => false,
                };
                if due {
                    let obs = evaluate_memory(run, *addr, *len, predicate);
                    self.decided = Some((
                        step,
                        vec![Access::Mem {
                            addr: *addr,
                            len: *len,
                        }],
                        obs,
                    ));
                }
            }
            _ => {}
        }
    }

    /// After the instruction at `pc` ran and control went to `next_pc`.
    pub fn after(&mut self, pc: u64, step: u64, next_pc: u64, flow: &Flow, uses: &[Access]) {
        if self.decided.is_some() {
            return;
        }
        if let SinkSpec::BranchOutcome {
            pc: bpc,
            occurrence: want,
            taken,
        } = self.sink
            && *bpc == pc
            && self.occurrences.get(&pc).copied() == Some(*want)
        {
            let detail = format!("branch at step {step} went to 0x{next_pc:x}");
            let obs = match flow {
                Flow::CBranch { target } => {
                    if (next_pc == *target) == *taken {
                        SinkObservation::Holds { detail }
                    } else {
                        SinkObservation::DoesNotHold { detail }
                    }
                }
                other => SinkObservation::Undecided {
                    detail: format!(
                        "instruction at 0x{pc:x} is not a conditional branch ({other:?})"
                    ),
                },
            };
            self.decided = Some((step, uses.to_vec(), obs));
        }
    }

    /// The run is over; decide whatever is still open.
    pub fn finish(&mut self, run: &mut Run, end: &EndState, last: u64, last_uses: &[Access]) {
        if self.decided.is_some() {
            return;
        }
        if *end == EndState::Budget {
            self.decided = Some((
                last,
                Vec::new(),
                SinkObservation::Undecided {
                    detail: format!("instruction budget reached at step {last}"),
                },
            ));
            return;
        }
        self.decided = Some(match self.sink {
            SinkSpec::FaultAt { pc } => match end {
                EndState::Faulted { pc: at, message } if at == pc => (
                    last.saturating_sub(1),
                    last_uses.to_vec(),
                    SinkObservation::Holds {
                        detail: format!("fault at 0x{at:x}: {message}"),
                    },
                ),
                other => (
                    last,
                    Vec::new(),
                    SinkObservation::DoesNotHold {
                        detail: format!("run ended {other:?}"),
                    },
                ),
            },
            SinkSpec::MemoryValue {
                addr,
                len,
                predicate,
                at: SinkPoint::Exit,
            } => match end {
                EndState::Faulted { pc, .. } => (
                    last,
                    Vec::new(),
                    SinkObservation::DoesNotHold {
                        detail: format!("faulted at 0x{pc:x} before exit"),
                    },
                ),
                _ => {
                    let obs = evaluate_memory(run, *addr, *len, predicate);
                    (
                        last,
                        vec![Access::Mem {
                            addr: *addr,
                            len: *len,
                        }],
                        obs,
                    )
                }
            },
            _ => (
                last,
                Vec::new(),
                SinkObservation::DoesNotHold {
                    detail: format!("sink event never happened; run ended {end:?}"),
                },
            ),
        });
    }
}

/// Decode the instruction at the emulator's program counter.
fn decode_live(program: &Program, run: &Run, pc: u64) -> Vec<fission_pcode_ir::ir::PcodeOp> {
    let ram = run.emu.state.ram_space();
    let bytes = run
        .emu
        .state
        .read_space_readonly(ram, pc, FETCH_BYTES)
        .unwrap_or_default();
    program
        .sleigh_decode(&bytes, pc)
        .map(|(ops, _)| ops)
        .unwrap_or_default()
}

/// Map a step outcome to an end state, if the run is over.
fn end_of(run: &Run, pc: u64, outcome: Result<RunOutcome>) -> Option<EndState> {
    match outcome {
        Ok(RunOutcome::Stepped) if !run.emu.halt_requested => None,
        Ok(_) => Some(EndState::Exited {
            code: run.emu.exit_code,
        }),
        Err(e) => Some(EndState::Faulted {
            pc,
            message: e.to_string(),
        }),
    }
}

/// Record `program` from its initial state to the end of the run, applying
/// `interventions` at their steps (empty for the recorded incident).
pub fn record(
    program: &Program,
    sink: &SinkSpec,
    interventions: &[crate::contract::Intervention],
    max_steps: u64,
) -> Result<Trace> {
    let mut run = program.fresh(Engine::Interpreter)?;
    record_on(program, &mut run, sink, interventions, max_steps)
}

/// [`record`] on a run already at the initial state (for instance a pooled
/// emulator restored to its initial checkpoint).
pub fn record_on(
    program: &Program,
    run: &mut Run,
    sink: &SinkSpec,
    interventions: &[crate::contract::Intervention],
    max_steps: u64,
) -> Result<Trace> {
    run.emu.state.tracing_memory = true;
    let traced = record_inner(program, run, sink, interventions, max_steps);
    run.emu.state.tracing_memory = false;
    run.emu.state.trace_mem_reads.clear();
    run.emu.state.trace_mem_writes.clear();
    traced
}

fn record_inner(
    program: &Program,
    run: &mut Run,
    sink: &SinkSpec,
    interventions: &[crate::contract::Intervention],
    max_steps: u64,
) -> Result<Trace> {
    let abi = SyscallAbi::for_arch(&run.emu.arch);
    let syscall_uses: Vec<Access> = std::iter::once(abi.number)
        .chain(abi.args.iter().copied())
        .filter_map(|r| program.register(r))
        .map(|(_, offset, size)| Access::Reg { offset, size })
        .collect();
    let syscall_defs: Vec<Access> = program
        .register(abi.result)
        .map(|(_, offset, size)| vec![Access::Reg { offset, size }])
        .unwrap_or_default();

    let mut code = CodeIndex::new(program);
    let mut control = DynamicControl {
        code: &mut code,
        frames: vec![(None, Vec::new())],
    };
    let mut monitor = SinkMonitor::new(sink);
    let mut steps: Vec<StepRecord> = Vec::new();
    let mut inputs: Vec<InputByte> = Vec::new();
    let mut input_cursor = 0usize;
    let mut end = EndState::Budget;

    while (steps.len() as u64) < max_steps {
        let step = run.emu.inst_count;
        if step != steps.len() as u64 {
            bail!(
                "trace position {} but {} instructions retired: a step that does not retire \
                 exactly one instruction (an HLE stub) is outside the v1 contract",
                steps.len(),
                step
            );
        }
        for iv in interventions.iter().filter(|iv| iv.step == step) {
            crate::runner::apply(program, run, iv)?;
        }
        let pc = run.emu.pc;
        monitor.before(program, run, pc, step);

        let ops = decode_live(program, run, pc);
        let flow = flow_of(&ops, program.ram_space);
        let parent = control.parent(pc);

        let syscalls_before = run.syscall_count();
        run.emu.state.trace_mem_reads.clear();
        run.emu.state.trace_mem_writes.clear();
        let outcome = run.emu.step_instruction();

        let mut record = analyse_ops(program, step, pc, &ops, flow.clone(), parent);
        record.next_pc = run.emu.pc;
        for (addr, data) in run.emu.state.trace_mem_reads.drain(..) {
            let exec = run.emu.state.page_map.range_has_exec(addr, data.len());
            if data.len() == FETCH_BYTES && exec {
                continue;
            }
            record.uses.push(Access::Mem {
                addr,
                len: data.len() as u32,
            });
        }
        let writes: Vec<(u64, Vec<u8>, Vec<u8>)> =
            run.emu.state.trace_mem_writes.drain(..).collect();
        for (addr, _, new) in &writes {
            record.defs.push(Access::Mem {
                addr: *addr,
                len: new.len() as u32,
            });
        }
        record.syscalls = run.syscalls_since(syscalls_before);
        if !record.syscalls.is_empty() {
            record.uses.extend(syscall_uses.iter().copied());
            record.defs.extend(syscall_defs.iter().copied());
        }
        for call in &record.syscalls {
            if call.name == Some("read") && call.args[0] == 0 {
                let (buf, count) = (call.args[1], call.args[2]);
                let mut delivered: BTreeSet<u64> = BTreeSet::new();
                for (addr, _, new) in &writes {
                    for i in 0..new.len() as u64 {
                        let a = addr.wrapping_add(i);
                        if a >= buf && a < buf.wrapping_add(count) {
                            delivered.insert(a);
                        }
                    }
                }
                let n = delivered.len();
                for a in delivered {
                    let byte = InputByte {
                        addr: a,
                        offset: input_cursor + (a - buf) as usize,
                    };
                    record.input.push(byte);
                    inputs.push(byte);
                }
                input_cursor += n;
            }
        }
        record.uses.sort();
        record.uses.dedup();
        record.defs.sort();
        record.defs.dedup();
        control.after(step, pc, &flow);
        monitor.after(pc, step, record.next_pc, &flow, &record.uses);
        steps.push(record);

        if let Some(done) = end_of(run, pc, outcome) {
            end = done;
            break;
        }
    }

    let last = steps.len() as u64;
    let last_uses = steps.last().map(|s| s.uses.clone()).unwrap_or_default();
    monitor.finish(run, &end, last, &last_uses);
    let stdout = run.stdout();
    let (sink_step, criterion, observation) = monitor.decided.take().unwrap_or((
        last,
        Vec::new(),
        SinkObservation::Undecided {
            detail: "no decision".into(),
        },
    ));
    Ok(Trace {
        steps,
        sink_step,
        criterion,
        observation,
        end,
        inputs,
        stdout,
    })
}

/// Single-step `run` from where it is, evaluating Φ, until Φ is decided
/// (or, with `to_end`, until the run ends). Returns Φ and the end state.
pub fn observe(
    program: &Program,
    run: &mut Run,
    monitor: &mut SinkMonitor<'_>,
    max_steps: u64,
    to_end: bool,
) -> (SinkObservation, EndState) {
    let mut end = EndState::Budget;
    let branch_pc = match monitor.sink {
        SinkSpec::BranchOutcome { pc, .. } => Some(*pc),
        _ => None,
    };
    let mut last_uses: Vec<Access> = Vec::new();
    while run.emu.inst_count < max_steps {
        let step = run.emu.inst_count;
        let pc = run.emu.pc;
        monitor.before(program, run, pc, step);
        if monitor.decided.is_some() && !to_end {
            end = EndState::StoppedAtSink;
            break;
        }
        let flow = if branch_pc == Some(pc) || matches!(monitor.sink, SinkSpec::FaultAt { .. }) {
            let ops = decode_live(program, run, pc);
            let flow = flow_of(&ops, program.ram_space);
            if matches!(monitor.sink, SinkSpec::FaultAt { .. }) {
                last_uses = analyse_ops(program, step, pc, &ops, flow.clone(), None).uses;
            }
            flow
        } else {
            Flow::Fall
        };
        let outcome = run.emu.step_instruction();
        monitor.after(pc, step, run.emu.pc, &flow, &[]);
        if let Some(done) = end_of(run, pc, outcome) {
            end = done;
            break;
        }
        if monitor.decided.is_some() && !to_end {
            end = EndState::StoppedAtSink;
            break;
        }
    }
    let last = run.emu.inst_count + u64::from(matches!(end, EndState::Faulted { .. }));
    monitor.finish(run, &end, last, &last_uses);
    let obs = monitor
        .decided
        .as_ref()
        .map(|(_, _, o)| o.clone())
        .unwrap_or(SinkObservation::Undecided {
            detail: "no decision".into(),
        });
    (obs, end)
}

/// [`observe`] without single-stepping: the run goes block by block and
/// stops only at breakpoints on the sink's own addresses, where the same
/// [`SinkMonitor`] decides Φ. A fault sink needs the exact faulting
/// instruction, which only single-stepping reports, so it takes the slow
/// path.
pub fn observe_fast(
    program: &Program,
    run: &mut Run,
    monitor: &mut SinkMonitor<'_>,
    max_steps: u64,
    to_end: bool,
) -> (SinkObservation, EndState) {
    let stops: Vec<u64> = match monitor.sink {
        SinkSpec::FaultAt { .. } => return observe(program, run, monitor, max_steps, to_end),
        SinkSpec::CallTo { target, .. } => vec![*target],
        SinkSpec::BranchOutcome { pc, .. } => vec![*pc],
        SinkSpec::MemoryValue {
            at: SinkPoint::PcHit { pc, .. },
            ..
        } => vec![*pc],
        SinkSpec::MemoryValue { .. } => Vec::new(),
    };
    for pc in &stops {
        if !run.emu.is_breakpoint(*pc) {
            run.emu.set_breakpoint(*pc);
        }
    }
    let branch_pc = match monitor.sink {
        SinkSpec::BranchOutcome { pc, .. } => Some(*pc),
        _ => None,
    };
    let at_step = match monitor.sink {
        SinkSpec::MemoryValue {
            at: SinkPoint::Step { step },
            ..
        } => Some(*step),
        _ => None,
    };
    let end = loop {
        let budget = match at_step {
            Some(s) if monitor.decided.is_none() && run.emu.inst_count <= s => s,
            _ => max_steps,
        };
        run.emu.max_inst = Some(budget);
        let outcome = run.emu.resume();
        run.emu.max_inst = None;
        match outcome {
            Ok(RunOutcome::HitBreakpoint(pc)) => {
                let step = run.emu.inst_count;
                monitor.before(program, run, pc, step);
                if branch_pc == Some(pc) && monitor.decided.is_none() {
                    let ops = decode_live(program, run, pc);
                    let flow = flow_of(&ops, program.ram_space);
                    let stepped = run.emu.step_instruction();
                    monitor.after(pc, step, run.emu.pc, &flow, &[]);
                    if let Some(done) = end_of(run, pc, stepped) {
                        break done;
                    }
                }
                if monitor.decided.is_some() && !to_end {
                    break EndState::StoppedAtSink;
                }
            }
            Ok(RunOutcome::HitBudget) if Some(budget) == at_step && budget < max_steps => {
                let pc = run.emu.pc;
                monitor.before(program, run, pc, budget);
                if !to_end {
                    break EndState::StoppedAtSink;
                }
            }
            Ok(RunOutcome::HitBudget) => break EndState::Budget,
            Ok(_) => {
                break EndState::Exited {
                    code: run.emu.exit_code,
                };
            }
            Err(e) => {
                break EndState::Faulted {
                    pc: run.emu.pc,
                    message: e.to_string(),
                };
            }
        }
    };
    let last = run.emu.inst_count;
    monitor.finish(run, &end, last, &[]);
    let obs = monitor
        .decided
        .as_ref()
        .map(|(_, _, o)| o.clone())
        .unwrap_or(SinkObservation::Undecided {
            detail: "no decision".into(),
        });
    (obs, end)
}

fn read_args(run: &mut Run, n: usize) -> Vec<u64> {
    (0..n).map(|i| run.emu.read_arg(i).unwrap_or(0)).collect()
}

/// The argument registers a call sink constrains.
pub fn call_criterion(program: &Program, run: &Run, args: &[Option<u64>]) -> Vec<Access> {
    let regs = run.emu.arch.cc.arg_regs();
    args.iter()
        .enumerate()
        .filter(|(_, a)| a.is_some())
        .filter_map(|(i, _)| regs.get(i))
        .filter_map(|r| program.register(r))
        .map(|(_, offset, size)| Access::Reg { offset, size })
        .collect()
}

pub fn evaluate_memory(
    run: &mut Run,
    addr: u64,
    len: u32,
    predicate: &MemPredicate,
) -> SinkObservation {
    let ram = run.emu.state.ram_space();
    match run.emu.state.read_space_readonly(ram, addr, len as usize) {
        Ok(bytes) => {
            let holds = match predicate {
                MemPredicate::Equals(want) => &bytes == want,
                MemPredicate::NonZero => bytes.iter().any(|b| *b != 0),
            };
            let detail = format!("mem[0x{addr:x}..+{len}] = {bytes:02x?}");
            if holds {
                SinkObservation::Holds { detail }
            } else {
                SinkObservation::DoesNotHold { detail }
            }
        }
        Err(e) => SinkObservation::DoesNotHold {
            detail: format!("mem[0x{addr:x}] unreadable: {e}"),
        },
    }
}

/// Register uses and definitions of one instruction from its p-code, with
/// reads of a byte the same instruction already wrote not counted as uses.
fn analyse_ops(
    program: &Program,
    step: u64,
    pc: u64,
    ops: &[fission_pcode_ir::ir::PcodeOp],
    flow: Flow,
    cd_parent: Option<u64>,
) -> StepRecord {
    let reg = program.register_space;
    let mut defined: BTreeSet<u64> = BTreeSet::new();
    let mut uses: BTreeSet<(u64, u32)> = BTreeSet::new();
    let mut defs: BTreeSet<(u64, u32)> = BTreeSet::new();
    let mut compare_consts = Vec::new();
    // Which registers feed each temporary, so a LOAD/STORE address can be
    // traced back to the registers that computed it.
    let mut feeds: BTreeMap<(u64, u64), BTreeSet<u64>> = BTreeMap::new();
    let mut constant_temps: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    let mut address_regs: BTreeSet<u64> = BTreeSet::new();
    let mut direct_addresses = Vec::new();

    for op in ops {
        let memory_op = matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store);
        let mut op_feeds: BTreeSet<u64> = BTreeSet::new();
        let mut all_const = true;
        let mut const_value: Option<u64> = None;
        for (k, input) in op.inputs.iter().enumerate() {
            if memory_op && k == 0 {
                continue;
            }
            if input.is_constant {
                const_value = Some(input.offset);
                continue;
            }
            if input.space_id == reg {
                all_const = false;
                let first_unwritten =
                    (0..u64::from(input.size)).any(|i| !defined.contains(&(input.offset + i)));
                if first_unwritten {
                    uses.insert((input.offset, input.size));
                }
                op_feeds.insert(input.offset);
            } else if input.space_id != program.ram_space {
                let key = (input.space_id, input.offset);
                if let Some(f) = feeds.get(&key) {
                    op_feeds.extend(f.iter().copied());
                }
                if !constant_temps.contains_key(&key) {
                    all_const = false;
                }
            } else {
                all_const = false;
            }
        }
        if memory_op && let Some(addr) = op.inputs.get(1) {
            if addr.is_constant {
                direct_addresses.push(Some(addr.offset));
            } else if addr.space_id == reg {
                address_regs.insert(addr.offset);
                direct_addresses.push(None);
            } else {
                let key = (addr.space_id, addr.offset);
                address_regs.extend(feeds.get(&key).into_iter().flatten().copied());
                direct_addresses.push(constant_temps.get(&key).copied());
            }
        }
        if matches!(
            op.opcode,
            PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::IntSub
        ) {
            for input in &op.inputs {
                if input.is_constant && input.offset != 0 {
                    compare_consts.push(input.offset);
                }
            }
        }
        if let Some(out) = &op.output {
            if out.space_id == reg {
                for i in 0..u64::from(out.size) {
                    defined.insert(out.offset + i);
                }
                defs.insert((out.offset, out.size));
            } else if out.space_id != program.ram_space {
                let key = (out.space_id, out.offset);
                feeds.insert(key, op_feeds.clone());
                if all_const
                    && let Some(v) = const_value
                    && matches!(op.opcode, PcodeOpcode::Copy)
                {
                    constant_temps.insert(key, v);
                } else {
                    constant_temps.remove(&key);
                }
            }
        }
    }
    compare_consts.sort_unstable();
    compare_consts.dedup();
    StepRecord {
        step,
        pc,
        uses: uses
            .into_iter()
            .map(|(offset, size)| Access::Reg { offset, size })
            .collect(),
        defs: defs
            .into_iter()
            .map(|(offset, size)| Access::Reg { offset, size })
            .collect(),
        flow,
        next_pc: pc,
        syscalls: Vec::new(),
        input: Vec::new(),
        compare_consts,
        cd_parent,
        address_regs: address_regs.into_iter().collect(),
        direct_addresses,
    }
}
