//! Symbolic replays over the input stream, and the solver queries the
//! joint search asks of them.
//!
//! A symbolic replay is an ordinary emulator run on the interpreter with
//! `ShadowMode::Symbolic`: every byte the guest reads from standard input
//! is an 8-bit solver variable, and every conditional branch whose
//! condition depends on one is recorded with its condition expression. The
//! run's concrete behaviour is the emulator's; the expressions only
//! describe it. The shadow layer is coarse (a multi-byte store tags every
//! byte with the whole value's expression, unmodelled operations drop the
//! expression), so nothing a query answers is trusted until a replay
//! reproduces it.
//!
//! Variables are renamed to canonical ids derived from the input offset
//! before any query, so a model does not depend on the global variable
//! counter (and so on what else ran in the process): the same case asks the
//! same queries and gets the same models.

use std::collections::BTreeMap;

use anyhow::Result;
use fission_emulator::MachineState;
use fission_emulator::observe::ShadowMode;
use fission_solver::ast::{Sort, SymExpr, SymNodeId};
use fission_solver::solver::{SatResult, Solver, UnknownReason};

use crate::contract::{SinkObservation, SinkSpec};
use crate::machine::{Engine, Program};
use crate::trace::{self, EndState, SinkMonitor};

/// Canonical solver id of input byte `offset`.
const INPUT_VAR_BASE: SymNodeId = 0x4000_0000;
/// Canonical ids of symbolic values that are not input bytes.
const OTHER_VAR_BASE: SymNodeId = 0x6000_0000;

pub fn input_var(offset: usize) -> SymNodeId {
    INPUT_VAR_BASE + offset as SymNodeId
}

pub fn input_expr(offset: usize) -> SymExpr {
    SymExpr::Var {
        id: input_var(offset),
        name: format!("in{offset}"),
        sort: Sort::BitVector(8),
    }
}

/// A conditional branch the replay took on a symbolic condition.
#[derive(Debug, Clone)]
pub struct PathBranch {
    /// Retired instructions before the branch.
    pub step: u64,
    pub pc: u64,
    /// Whether the branch went to its target (the condition was true).
    pub taken: bool,
    /// The condition (true = branch to target), over canonical input
    /// variables.
    pub cond: SymExpr,
}

impl PathBranch {
    /// The constraint "this branch goes the way it went".
    pub fn as_taken(&self) -> SymExpr {
        if self.taken {
            self.cond.clone()
        } else {
            negate(&self.cond)
        }
    }
}

/// One symbolic replay.
pub struct SymPath {
    pub input: Vec<u8>,
    /// Symbolic branches before Φ was decided, in execution order.
    pub branches: Vec<PathBranch>,
    pub observation: SinkObservation,
    pub end: EndState,
    /// Φ itself as a boolean over the input, when the sink's own operand is
    /// symbolic (a branch-outcome sink on a symbolic condition, a call sink
    /// whose constrained arguments are symbolic).
    pub phi: Option<SymExpr>,
    /// The machine state at the end of the replay: the solver's memory
    /// oracle for symbolic loads.
    pub state: MachineState,
    /// Symbolic values that were not input bytes (another syscall's
    /// variables). Left free in queries.
    pub other_vars: usize,
}

impl SymPath {
    pub fn holds(&self) -> Option<bool> {
        self.observation.holds()
    }

    /// `(pc, taken)` of every symbolic branch, for comparing two replays.
    pub fn signature(&self) -> Vec<(u64, bool)> {
        self.branches.iter().map(|b| (b.pc, b.taken)).collect()
    }
}

fn negate(cond: &SymExpr) -> SymExpr {
    SymExpr::Eq(
        Box::new(cond.clone()),
        Box::new(SymExpr::Const {
            val: 0,
            size: cond.get_bit_width(),
        }),
    )
}

/// Run `program` on `input` with symbolic input bytes until Φ is decided
/// (or the run ends). Interpreter throughout: it is where the symbolic
/// shadow and the branch events live, and it retires exact step counts.
pub fn replay(program: &Program, input: &[u8], sink: &SinkSpec, max_steps: u64) -> Result<SymPath> {
    let mut run = program.fresh_with_input(Engine::Interpreter, input)?;
    run.emu.set_shadow_mode(ShadowMode::Symbolic);
    run.emu.sym_events.clear();
    let ram = run.emu.state.ram_space();
    // Emulator variable id -> input offset, from the shadow of each byte a
    // read of standard input delivered, in delivery order.
    let mut vars: BTreeMap<SymNodeId, usize> = BTreeMap::new();
    let mut cursor = 0usize;
    let mut monitor = SinkMonitor::new(sink);
    let mut end = EndState::Budget;
    let branch_pc = match sink {
        SinkSpec::BranchOutcome { pc, .. } => Some(*pc),
        _ => None,
    };
    let mut phi: Option<SymExpr> = None;
    let mut sink_branch_seen = 0u32;
    while run.emu.inst_count < max_steps {
        let step = run.emu.inst_count;
        let pc = run.emu.pc;
        monitor.before(program, &mut run, pc, step);
        if monitor.decided.is_some() {
            if let SinkSpec::CallTo { args, .. } = sink
                && phi.is_none()
            {
                phi = call_phi(program, &run, args);
            }
            end = EndState::StoppedAtSink;
            break;
        }
        let events_before = run.emu.sym_events.len();
        let syscalls_before = run.syscall_count();
        let faults_before = run.emu.metrics.memory_faults;
        let flow = if branch_pc == Some(pc) {
            let ops = program
                .sleigh_decode(
                    &run.emu
                        .state
                        .read_space_readonly(ram, pc, 16)
                        .unwrap_or_default(),
                    pc,
                )
                .map(|(ops, _)| ops)
                .unwrap_or_default();
            crate::cfg::flow_of(&ops, program.ram_space)
        } else {
            crate::cfg::Flow::Fall
        };
        let outcome = run.emu.step_instruction();
        for call in run.syscalls_since(syscalls_before) {
            if call.name == Some("read") && call.args[0] == 0 {
                let buf = call.args[1];
                for i in 0..call.args[2] {
                    let Some(node) = run.emu.state.get_shadow_memory(ram, buf + i) else {
                        break;
                    };
                    if let Some(SymExpr::Var { id, .. }) = run.emu.solver.nodes.get(&node)
                        && !vars.contains_key(id)
                    {
                        vars.insert(*id, cursor);
                        cursor += 1;
                    }
                }
            }
        }
        if branch_pc == Some(pc) {
            sink_branch_seen += 1;
            if let SinkSpec::BranchOutcome {
                occurrence, taken, ..
            } = sink
                && sink_branch_seen == *occurrence
                && let Some(event) = run.emu.sym_events[events_before..]
                    .iter()
                    .find(|e| e.pc == pc)
                && let Some(cond) = event
                    .condition_node
                    .and_then(|n| run.emu.solver.nodes.get(&n))
            {
                phi = Some(if *taken { cond.clone() } else { negate(cond) });
            }
        }
        if run.emu.metrics.memory_faults > faults_before
            && let Some(fault) = run.emu.last_fault
        {
            monitor.fault(fault.pc, step, &[]);
        }
        monitor.after(pc, step, run.emu.pc, &flow, &[]);
        let done = match outcome {
            Ok(fission_emulator::RunOutcome::Stepped) if !run.emu.halt_requested => None,
            Ok(_) => Some(trace::halted_end(&run)),
            Err(e) => {
                let message = e.to_string();
                Some(EndState::Faulted {
                    pc: trace::guest_fault_pc(&message).unwrap_or(pc),
                    message,
                })
            }
        };
        if let Some(done) = done {
            end = done;
            break;
        }
        if monitor.decided.is_some() {
            end = EndState::StoppedAtSink;
            break;
        }
    }
    let last = run.emu.inst_count + u64::from(matches!(end, EndState::Faulted { .. }));
    monitor.finish(&mut run, &end, last, &[]);
    let (sink_step, observation) = monitor
        .decided
        .as_ref()
        .map(|(s, _, o)| (*s, o.clone()))
        .unwrap_or((
            last,
            SinkObservation::Undecided {
                detail: "no decision".into(),
            },
        ));

    let mut renamer = Renamer {
        inputs: vars,
        others: BTreeMap::new(),
    };
    let mut branches = Vec::new();
    for event in &run.emu.sym_events {
        if event.step_index >= sink_step {
            break;
        }
        let Some(cond) = event
            .condition_node
            .and_then(|n| run.emu.solver.nodes.get(&n))
        else {
            continue;
        };
        branches.push(PathBranch {
            step: event.step_index,
            pc: event.pc,
            taken: event.condition_val_taken,
            cond: renamer.rename(cond),
        });
    }
    let phi = phi.map(|p| renamer.rename(&p));
    Ok(SymPath {
        input: input.to_vec(),
        branches,
        observation,
        end,
        phi,
        state: run.emu.state.clone(),
        other_vars: renamer.others.len(),
    })
}

/// Φ for a call sink: every constrained argument register equals its
/// value, when each one's shadow is symbolic.
fn call_phi(program: &Program, run: &crate::machine::Run, args: &[Option<u64>]) -> Option<SymExpr> {
    let regs = run.emu.arch.cc.arg_regs();
    let mut terms = Vec::new();
    for (i, want) in args.iter().enumerate() {
        let Some(want) = want else { continue };
        let (_, offset, size) = program.register(regs.get(i)?)?;
        let node = run
            .emu
            .state
            .get_shadow_memory(program.register_space, offset)?;
        let expr = run.emu.solver.nodes.get(&node)?.clone();
        let width = expr.get_bit_width();
        if width == 0 || width > size * 8 || width > 64 {
            return None;
        }
        let mask = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        if want & !mask != 0 {
            // The register's symbolic part cannot hold the value.
            return None;
        }
        terms.push(SymExpr::Eq(
            Box::new(expr),
            Box::new(SymExpr::Const {
                val: *want,
                size: width,
            }),
        ));
    }
    let mut it = terms.into_iter();
    let first = it.next()?;
    Some(it.fold(first, |a, b| SymExpr::And(Box::new(a), Box::new(b))))
}

/// Emulator variables -> canonical ones.
struct Renamer {
    inputs: BTreeMap<SymNodeId, usize>,
    others: BTreeMap<String, SymNodeId>,
}

impl Renamer {
    fn rename(&mut self, e: &SymExpr) -> SymExpr {
        use SymExpr as E;
        let b = |r: &mut Self, x: &SymExpr| Box::new(r.rename(x));
        match e {
            E::Const { .. } => e.clone(),
            E::Var { id, name, sort } => match self.inputs.get(id) {
                Some(offset) if *sort == Sort::BitVector(8) => input_expr(*offset),
                _ => {
                    let next = OTHER_VAR_BASE + self.others.len() as SymNodeId;
                    let key = format!("{name}#{id}");
                    let id = *self.others.entry(key).or_insert(next);
                    E::Var {
                        id,
                        name: format!("other{}", id - OTHER_VAR_BASE),
                        sort: sort.clone(),
                    }
                }
            },
            E::Add(x, y) => E::Add(b(self, x), b(self, y)),
            E::Sub(x, y) => E::Sub(b(self, x), b(self, y)),
            E::Mul(x, y) => E::Mul(b(self, x), b(self, y)),
            E::Udiv(x, y) => E::Udiv(b(self, x), b(self, y)),
            E::Urem(x, y) => E::Urem(b(self, x), b(self, y)),
            E::Sdiv(x, y) => E::Sdiv(b(self, x), b(self, y)),
            E::Srem(x, y) => E::Srem(b(self, x), b(self, y)),
            E::Smod(x, y) => E::Smod(b(self, x), b(self, y)),
            E::And(x, y) => E::And(b(self, x), b(self, y)),
            E::Or(x, y) => E::Or(b(self, x), b(self, y)),
            E::Xor(x, y) => E::Xor(b(self, x), b(self, y)),
            E::Shl(x, y) => E::Shl(b(self, x), b(self, y)),
            E::Lshr(x, y) => E::Lshr(b(self, x), b(self, y)),
            E::Ashr(x, y) => E::Ashr(b(self, x), b(self, y)),
            E::Eq(x, y) => E::Eq(b(self, x), b(self, y)),
            E::Neq(x, y) => E::Neq(b(self, x), b(self, y)),
            E::Ult(x, y) => E::Ult(b(self, x), b(self, y)),
            E::Ule(x, y) => E::Ule(b(self, x), b(self, y)),
            E::Slt(x, y) => E::Slt(b(self, x), b(self, y)),
            E::Sle(x, y) => E::Sle(b(self, x), b(self, y)),
            E::Sgt(x, y) => E::Sgt(b(self, x), b(self, y)),
            E::FAdd(x, y) => E::FAdd(b(self, x), b(self, y)),
            E::FSub(x, y) => E::FSub(b(self, x), b(self, y)),
            E::FMul(x, y) => E::FMul(b(self, x), b(self, y)),
            E::FDiv(x, y) => E::FDiv(b(self, x), b(self, y)),
            E::FNeg(x) => E::FNeg(b(self, x)),
            E::FAbs(x) => E::FAbs(b(self, x)),
            E::FSqrt(x) => E::FSqrt(b(self, x)),
            E::FEq(x, y) => E::FEq(b(self, x), b(self, y)),
            E::FNeq(x, y) => E::FNeq(b(self, x), b(self, y)),
            E::FLt(x, y) => E::FLt(b(self, x), b(self, y)),
            E::FLe(x, y) => E::FLe(b(self, x), b(self, y)),
            E::FIsNan(x) => E::FIsNan(b(self, x)),
            E::Ite { cond, t, f } => E::Ite {
                cond: b(self, cond),
                t: b(self, t),
                f: b(self, f),
            },
            E::Extract { expr, lsb, size } => E::Extract {
                expr: b(self, expr),
                lsb: *lsb,
                size: *size,
            },
            E::Concat(x, y) => E::Concat(b(self, x), b(self, y)),
            E::ArraySelect { array, index } => E::ArraySelect {
                array: b(self, array),
                index: b(self, index),
            },
            E::ArrayStore {
                array,
                index,
                value,
            } => E::ArrayStore {
                array: b(self, array),
                index: b(self, index),
                value: b(self, value),
            },
        }
    }
}

/// What one query concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResult {
    /// A model: the value of every input byte the query mentions.
    Sat(BTreeMap<usize, u8>),
    Unsat,
    Unknown(String),
}

/// `x_i != base_i` summed over `free` offsets is at most `k`.
fn cardinality(free: &[usize], base: &[u8], k: usize) -> SymExpr {
    let width = 16;
    let one = SymExpr::Const {
        val: 1,
        size: width,
    };
    let zero = SymExpr::Const {
        val: 0,
        size: width,
    };
    let mut sum = zero.clone();
    for &o in free {
        let differs = SymExpr::Neq(
            Box::new(input_expr(o)),
            Box::new(SymExpr::Const {
                val: u64::from(base[o]),
                size: 8,
            }),
        );
        let term = SymExpr::Ite {
            cond: Box::new(differs),
            t: Box::new(one.clone()),
            f: Box::new(zero.clone()),
        };
        sum = SymExpr::Add(Box::new(sum), Box::new(term));
    }
    SymExpr::Ule(
        Box::new(sum),
        Box::new(SymExpr::Const {
            val: k as u64,
            size: width,
        }),
    )
}

/// Pin every input byte outside `free` to its value in `base`.
fn pinned(base: &[u8], free: &[usize]) -> Vec<SymExpr> {
    (0..base.len())
        .filter(|o| !free.contains(o))
        .map(|o| {
            SymExpr::Eq(
                Box::new(input_expr(o)),
                Box::new(SymExpr::Const {
                    val: u64::from(base[o]),
                    size: 8,
                }),
            )
        })
        .collect()
}

/// Ask whether `constraints` hold with the input differing from `base` in
/// at most `k` of the `free` offsets (the others pinned to `base`). One
/// fresh solver per query, bounded by `conflicts`, so a query's answer
/// depends on nothing asked before it.
pub fn solve(
    constraints: &[SymExpr],
    base: &[u8],
    free: &[usize],
    k: usize,
    conflicts: u64,
    oracle: &MachineState,
) -> QueryResult {
    let mut all: Vec<SymExpr> = constraints.to_vec();
    all.push(cardinality(free, base, k));
    all.extend(pinned(base, free));
    let mut solver = Solver::new();
    solver.set_conflict_budget(Some(conflicts));
    // A model reports the variables the solver knows by node.
    for o in 0..base.len() {
        solver.nodes.insert(input_var(o), input_expr(o));
    }
    let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        solver.check_sat_with_oracle(Some(oracle), &all)
    }));
    match answer {
        Ok(Ok(SatResult::Sat)) => {
            let mut model = BTreeMap::new();
            for o in 0..base.len() {
                let value = solver.get_value(input_var(o)).map_or(base[o], |v| v as u8);
                model.insert(o, value);
            }
            QueryResult::Sat(model)
        }
        Ok(Ok(SatResult::Unsat)) => QueryResult::Unsat,
        Ok(Ok(SatResult::Unknown)) => QueryResult::Unknown(match solver.last_unknown() {
            Some(UnknownReason::ConflictBudget) => format!("conflict budget ({conflicts})"),
            Some(UnknownReason::Unsupported(what)) => format!("unsupported: {}", what.join(", ")),
            Some(UnknownReason::OracleRefinement) => "memory-oracle refinement".into(),
            None => "unknown".into(),
        }),
        Ok(Err(e)) => QueryResult::Unknown(format!("solver error: {e}")),
        Err(_) => QueryResult::Unknown("solver panicked".into()),
    }
}
