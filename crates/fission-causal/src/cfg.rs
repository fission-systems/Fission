//! Static instruction-level control flow, postdominators and control
//! dependence, per function, from Sleigh p-code.
//!
//! Every fact here comes from the lifted p-code of the binary's own bytes,
//! so it is the same code on every ISA (ADR 0009): a conditional branch is a
//! `CBRANCH` to a RAM address, a call is a `CALL`, a return is a `RETURN`.
//! Function bounds come from the loader's symbol table. A function with no
//! known size is not given a CFG; its branches get no postdominator, which
//! callers treat as "control scope lasts until the activation returns".

use std::collections::{BTreeMap, BTreeSet};

use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode};

use crate::machine::Program;

/// Longest instruction any supported ISA decodes.
const MAX_INSN_BYTES: usize = 16;
/// Upper bound on instructions per function CFG.
const MAX_FUNCTION_INSNS: usize = 20_000;

/// How control leaves an instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Fall,
    Branch(u64),
    CBranch { target: u64 },
    Call { target: Option<u64> },
    Return,
    /// Indirect jump: target unknown statically.
    Indirect,
}

#[derive(Debug, Clone)]
pub struct Insn {
    pub pc: u64,
    pub len: u32,
    pub ops: Vec<PcodeOp>,
    pub flow: Flow,
}

impl Insn {
    pub fn fall_through(&self) -> u64 {
        self.pc.wrapping_add(u64::from(self.len))
    }
}

/// Decode one instruction from the binary's file bytes.
pub fn decode(program: &Program, pc: u64) -> Option<Insn> {
    let available = program.binary.available_execution_bytes(pc)?;
    let bytes = program
        .binary
        .view_executable_bytes(pc, available.min(MAX_INSN_BYTES))?;
    let (ops, len) = program.sleigh_decode(bytes, pc)?;
    if len == 0 {
        return None;
    }
    let len = u32::try_from(len).ok()?;
    let flow = flow_of(&ops, program.ram_space);
    Some(Insn { pc, len, ops, flow })
}

/// Classify an instruction's control transfer from its p-code.
pub fn flow_of(ops: &[PcodeOp], ram_space: u64) -> Flow {
    let ram_target = |op: &PcodeOp| {
        op.inputs
            .first()
            .filter(|v| !v.is_constant && v.space_id == ram_space)
            .map(|v| v.offset)
    };
    let mut flow = Flow::Fall;
    for op in ops {
        match op.opcode {
            PcodeOpcode::Return => return Flow::Return,
            PcodeOpcode::BranchInd => return Flow::Indirect,
            PcodeOpcode::Call => flow = Flow::Call { target: ram_target(op) },
            PcodeOpcode::CallInd => flow = Flow::Call { target: None },
            PcodeOpcode::Branch => {
                // A constant-space target is a p-code-relative branch inside
                // this instruction, not a transfer out of it.
                if let Some(target) = ram_target(op) {
                    return Flow::Branch(target);
                }
            }
            PcodeOpcode::CBranch => {
                if let Some(target) = ram_target(op) {
                    flow = Flow::CBranch { target };
                }
            }
            _ => {}
        }
    }
    flow
}

/// One function's instructions, immediate postdominators and control
/// dependence.
#[derive(Debug, Clone, Default)]
pub struct FunctionCfg {
    pub start: u64,
    pub end: u64,
    pub insns: BTreeMap<u64, Insn>,
    /// Immediate postdominator of every instruction; `None` is the
    /// function's exit (or no postdominator was found).
    pub ipdom: BTreeMap<u64, Option<u64>>,
    /// Instruction -> conditional branches it is control dependent on.
    pub control_parents: BTreeMap<u64, BTreeSet<u64>>,
}

impl FunctionCfg {
    pub fn successors(&self, insn: &Insn) -> Vec<Option<u64>> {
        let inside = |pc: u64| (pc >= self.start && pc < self.end).then_some(pc);
        match insn.flow {
            Flow::Fall | Flow::Call { .. } => vec![inside(insn.fall_through())],
            Flow::Branch(target) => vec![inside(target)],
            Flow::CBranch { target } => vec![inside(target), inside(insn.fall_through())],
            Flow::Return | Flow::Indirect => vec![None],
        }
    }
}

/// Lazily built function CFGs for a program.
pub struct CodeIndex<'p> {
    program: &'p Program,
    functions: BTreeMap<u64, Option<FunctionCfg>>,
}

impl<'p> CodeIndex<'p> {
    pub fn new(program: &'p Program) -> Self {
        Self {
            program,
            functions: BTreeMap::new(),
        }
    }

    /// The `(start, end)` of the function containing `pc`, by symbol.
    pub fn function_bounds(&self, pc: u64) -> Option<(u64, u64)> {
        let f = self.program.binary.function_at(pc)?;
        if f.is_import || f.size == 0 {
            return None;
        }
        let end = f.address.checked_add(f.size)?;
        (pc >= f.address && pc < end).then_some((f.address, end))
    }

    pub fn function(&mut self, pc: u64) -> Option<&FunctionCfg> {
        let (start, end) = self.function_bounds(pc)?;
        if !self.functions.contains_key(&start) {
            let built = build_function(self.program, start, end);
            self.functions.insert(start, built);
        }
        self.functions.get(&start).and_then(Option::as_ref)
    }

    /// Immediate postdominator of the instruction at `pc` within its
    /// function. `Some(None)` is "the function's exit"; `None` is "no CFG".
    pub fn ipdom(&mut self, pc: u64) -> Option<Option<u64>> {
        self.function(pc)?.ipdom.get(&pc).copied()
    }
}

fn build_function(program: &Program, start: u64, end: u64) -> Option<FunctionCfg> {
    let mut cfg = FunctionCfg {
        start,
        end,
        ..FunctionCfg::default()
    };
    // Linear sweep: compiler output for the fixtures this crate targets has
    // no data in .text, and a decode failure ends the sweep rather than
    // inventing instructions.
    let mut pc = start;
    while pc < end && cfg.insns.len() < MAX_FUNCTION_INSNS {
        let insn = decode(program, pc)?;
        pc = insn.fall_through();
        cfg.insns.insert(insn.pc, insn);
    }
    compute_postdominators(&mut cfg);
    compute_control_dependence(&mut cfg);
    Some(cfg)
}

/// Iterative postdominator sets over the reverse CFG with a virtual exit.
/// Functions in the fixtures are small; the quadratic set form is clear and
/// deterministic.
fn compute_postdominators(cfg: &mut FunctionCfg) {
    let nodes: Vec<u64> = cfg.insns.keys().copied().collect();
    let succs: BTreeMap<u64, Vec<Option<u64>>> = cfg
        .insns
        .values()
        .map(|i| {
            let s = cfg
                .successors(i)
                .into_iter()
                .map(|s| s.filter(|pc| cfg.insns.contains_key(pc)))
                .collect();
            (i.pc, s)
        })
        .collect();
    let all: BTreeSet<u64> = nodes.iter().copied().collect();
    let mut pdom: BTreeMap<u64, BTreeSet<u64>> =
        nodes.iter().map(|&n| (n, all.clone())).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for &n in nodes.iter().rev() {
            let mut meet: Option<BTreeSet<u64>> = None;
            for s in &succs[&n] {
                let set = match s {
                    // The virtual exit postdominates only itself.
                    None => BTreeSet::new(),
                    Some(s) => pdom[s].clone(),
                };
                meet = Some(match meet {
                    None => set,
                    Some(m) => m.intersection(&set).copied().collect(),
                });
            }
            let mut next = meet.unwrap_or_default();
            next.insert(n);
            if next != pdom[&n] {
                pdom.insert(n, next);
                changed = true;
            }
        }
    }
    // A node that cannot reach the exit keeps the full set; it gets no
    // postdominator rather than an invented one.
    for &n in &nodes {
        if !reaches_exit(&succs, n) {
            cfg.ipdom.insert(n, None);
            continue;
        }
        let strict: Vec<u64> = pdom[&n].iter().copied().filter(|&p| p != n).collect();
        // The immediate postdominator is the strict postdominator that every
        // other strict postdominator also postdominates.
        let ipdom = strict
            .iter()
            .copied()
            .find(|&p| strict.iter().all(|&q| q == p || pdom[&p].contains(&q)));
        cfg.ipdom.insert(n, ipdom);
    }
}

fn reaches_exit(succs: &BTreeMap<u64, Vec<Option<u64>>>, from: u64) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            continue;
        }
        for s in &succs[&n] {
            match s {
                None => return true,
                Some(s) => stack.push(*s),
            }
        }
    }
    false
}

/// Ferrante-Ottenstein-Warren: for every edge `b -> s` where `s` does not
/// postdominate `b`, the nodes on the postdominator-tree path from `s` up
/// to (not including) `ipdom(b)` are control dependent on `b`.
fn compute_control_dependence(cfg: &mut FunctionCfg) {
    let branches: Vec<(u64, Vec<u64>)> = cfg
        .insns
        .values()
        .filter(|i| matches!(i.flow, Flow::CBranch { .. }))
        .map(|i| {
            let succ = cfg
                .successors(i)
                .into_iter()
                .flatten()
                .filter(|pc| cfg.insns.contains_key(pc))
                .collect();
            (i.pc, succ)
        })
        .collect();
    for (b, succs) in branches {
        let stop = cfg.ipdom.get(&b).copied().flatten();
        for s in succs {
            let mut node = Some(s);
            let mut guard = 0usize;
            while let Some(n) = node {
                if Some(n) == stop || guard > cfg.insns.len() {
                    break;
                }
                cfg.control_parents.entry(n).or_default().insert(b);
                node = cfg.ipdom.get(&n).copied().flatten();
                guard += 1;
            }
        }
    }
}
