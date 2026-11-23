//! A pinned, conservative static backward slice: the static-slicing
//! baseline CSIC is compared with.
//!
//! Definition (fixed for P1 so results are comparable over time):
//!
//! - **Scope**: every function (by symbol bounds) the recorded run entered,
//!   decoded completely, so paths the run did not take are included.
//! - **Graph**: interprocedural, context-insensitive. A call goes to the
//!   callee's entry and every return of the callee goes back to every call
//!   site's fall-through. Indirect calls and jumps use the targets the
//!   recorded run took (more than a purely static tool would know).
//! - **Locations**: register bytes; memory bytes at addresses that block-local
//!   constant propagation resolves to a constant (globals) or to a stack or
//!   frame register plus a constant (frame slots); every other access is
//!   `MemAny`.
//! - **Aliasing**: no points-to analysis. A `MemAny` store may define, and a
//!   `MemAny` load may read, every memory location. Only a resolved store
//!   kills.
//! - **Syscalls**: use their number and argument registers and `MemAny`,
//!   define their result register and may define `MemAny`.
//! - **Control**: Ferrante-Ottenstein-Warren control dependence within each
//!   function; a callee in the slice pulls in its call sites.
//! - **Explanation**: input bytes of every read syscall in the slice whose
//!   (dynamically observed) addresses are relevant there; all of that
//!   read's bytes if `MemAny` or a frame slot is relevant there.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode};
use fission_emulator::os::linux::syscall_conv::SyscallAbi;
use serde::Serialize;

use crate::cfg::{CodeIndex, Flow, Insn};
use crate::machine::{Engine, Program};
use crate::trace::{Access, Trace};

/// An abstract location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Abs {
    Reg(u64),
    Glob(u64),
    /// (function start, base register offset, byte offset from base)
    Frame(u64, u64, i64),
    MemAny,
}

/// May a definition of `def` change `rel`?
fn may_alias(def: Abs, rel: Abs) -> bool {
    match (def, rel) {
        (Abs::Reg(a), Abs::Reg(b)) => a == b,
        (Abs::Reg(_), _) | (_, Abs::Reg(_)) => false,
        (Abs::MemAny, _) | (_, Abs::MemAny) => true,
        (Abs::Glob(a), Abs::Glob(b)) => a == b,
        (Abs::Frame(f, b, k), Abs::Frame(g, c, j)) => f == g && (b != c || k == j),
        _ => false,
    }
}

#[derive(Debug, Default, Clone)]
struct Node {
    uses: BTreeSet<Abs>,
    /// Definitions that kill (registers, resolved memory).
    kills: BTreeSet<Abs>,
    /// Every definition, including weak ones.
    defs: BTreeSet<Abs>,
    succs: Vec<u64>,
    control_parents: BTreeSet<u64>,
    function: u64,
    is_read_syscall: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StaticSliceResult {
    /// Static instructions in the slice.
    pub instructions: usize,
    /// Instructions in scope.
    pub scope_instructions: usize,
    pub functions: usize,
    pub input_offsets: BTreeSet<usize>,
    /// Whether `MemAny` reached a read in the slice.
    pub mem_any_at_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Val {
    Const(u64),
    Base(u64, i64),
}

fn mask(v: u64, size: u32) -> u64 {
    if size >= 8 { v } else { v & ((1u64 << (8 * size)) - 1) }
}

/// Block-local abstract evaluation of one instruction's p-code: returns
/// the resolved memory accesses (`None` for unresolved), in op order.
fn eval_insn(
    ops: &[PcodeOp],
    env: &mut BTreeMap<(u64, u64), Val>,
    program: &Program,
) -> Vec<(bool, Option<Val>, u32)> {
    let mut accesses = Vec::new();
    let value = |env: &BTreeMap<(u64, u64), Val>, v: &fission_pcode_ir::ir::Varnode| {
        if v.is_constant {
            Some(Val::Const(v.offset))
        } else {
            env.get(&(v.space_id, v.offset)).copied()
        }
    };
    for op in ops {
        let out_val = match op.opcode {
            PcodeOpcode::Load | PcodeOpcode::Store => {
                let addr = op.inputs.get(1).and_then(|a| value(env, a));
                let size = if op.opcode == PcodeOpcode::Load {
                    op.output.as_ref().map_or(0, |o| o.size)
                } else {
                    op.inputs.get(2).map_or(0, |v| v.size)
                };
                accesses.push((op.opcode == PcodeOpcode::Store, addr, size));
                None
            }
            PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                op.inputs.first().and_then(|a| value(env, a))
            }
            PcodeOpcode::IntAdd | PcodeOpcode::IntSub => {
                let a = op.inputs.first().and_then(|v| value(env, v));
                let b = op.inputs.get(1).and_then(|v| value(env, v));
                let sub = op.opcode == PcodeOpcode::IntSub;
                match (a, b) {
                    (Some(Val::Const(x)), Some(Val::Const(y))) => Some(Val::Const(if sub {
                        x.wrapping_sub(y)
                    } else {
                        x.wrapping_add(y)
                    })),
                    (Some(Val::Base(r, k)), Some(Val::Const(c))) => {
                        let size = op.output.as_ref().map_or(8, |o| o.size);
                        let c = mask(c, size);
                        let signed = if size >= 8 {
                            c as i64
                        } else {
                            ((c << (64 - 8 * size)) as i64) >> (64 - 8 * size)
                        };
                        Some(Val::Base(r, if sub { k - signed } else { k + signed }))
                    }
                    (Some(Val::Const(c)), Some(Val::Base(r, k))) if !sub => {
                        Some(Val::Base(r, k + c as i64))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(out) = &op.output {
            if out.space_id == program.ram_space {
                continue;
            }
            match out_val {
                Some(Val::Const(c)) => {
                    env.insert((out.space_id, out.offset), Val::Const(mask(c, out.size)));
                }
                Some(v) => {
                    env.insert((out.space_id, out.offset), v);
                }
                None => {
                    env.remove(&(out.space_id, out.offset));
                }
            }
        }
    }
    accesses
}

fn reg_bytes(offset: u64, size: u32) -> impl Iterator<Item = Abs> {
    (0..u64::from(size)).map(move |i| Abs::Reg(offset + i))
}

fn mem_abs(function: u64, val: Option<Val>, size: u32) -> Vec<Abs> {
    match val {
        Some(Val::Const(a)) => (0..u64::from(size))
            .map(|i| Abs::Glob(a.wrapping_add(i)))
            .collect(),
        Some(Val::Base(b, k)) => (0..i64::from(size))
            .map(|i| Abs::Frame(function, b, k + i))
            .collect(),
        None => vec![Abs::MemAny],
    }
}

/// Build the node for one instruction.
fn node_for(
    program: &Program,
    insn: &Insn,
    function: u64,
    env: &mut BTreeMap<(u64, u64), Val>,
    syscall_regs: &[(u64, u32)],
    result_reg: Option<(u64, u32)>,
) -> Node {
    let reg = program.register_space;
    let mut node = Node {
        function,
        ..Node::default()
    };
    let mut written: BTreeSet<u64> = BTreeSet::new();
    for op in &insn.ops {
        let memory_op = matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store);
        for (k, input) in op.inputs.iter().enumerate() {
            if (memory_op && k == 0) || input.is_constant || input.space_id != reg {
                continue;
            }
            for b in input.offset..input.offset + u64::from(input.size) {
                if !written.contains(&b) {
                    node.uses.insert(Abs::Reg(b));
                }
            }
        }
        if let Some(out) = &op.output
            && out.space_id == reg
        {
            for b in reg_bytes(out.offset, out.size) {
                node.kills.insert(b);
                node.defs.insert(b);
                if let Abs::Reg(o) = b {
                    written.insert(o);
                }
            }
        }
        if op.opcode == PcodeOpcode::CallOther {
            node.is_read_syscall = true;
        }
    }
    for (store, val, size) in eval_insn(&insn.ops, env, program) {
        let abs = mem_abs(function, val, size);
        if store {
            for a in abs {
                node.defs.insert(a);
                if a != Abs::MemAny {
                    node.kills.insert(a);
                }
            }
        } else {
            node.uses.extend(abs);
        }
    }
    // A CALLOTHER is treated as a possible syscall: it reads its argument
    // registers and any memory, writes its result register, and may write
    // any memory.
    if node.is_read_syscall {
        for &(offset, size) in syscall_regs {
            node.uses.extend(reg_bytes(offset, size));
        }
        node.uses.insert(Abs::MemAny);
        node.defs.insert(Abs::MemAny);
        if let Some((offset, size)) = result_reg {
            for b in reg_bytes(offset, size) {
                node.defs.insert(b);
            }
        }
    }
    node
}

/// Compute the pinned static slice for the recorded incident.
pub fn slice(program: &Program, trace: &Trace) -> Result<StaticSliceResult> {
    let mut code = CodeIndex::new(program);
    // Scope: functions the run entered.
    let mut functions: BTreeSet<u64> = BTreeSet::new();
    for s in &trace.steps {
        if let Some((start, _)) = code.function_bounds(s.pc) {
            functions.insert(start);
        }
    }
    let probe = program.fresh(Engine::Interpreter)?;
    let abi = SyscallAbi::for_arch(&probe.emu.arch);
    let syscall_regs: Vec<(u64, u32)> = std::iter::once(abi.number)
        .chain(abi.args.iter().copied())
        .filter_map(|r| program.register(r))
        .map(|(_, o, s)| (o, s))
        .collect();
    let result_reg = program.register(abi.result).map(|(_, o, s)| (o, s));
    let base_regs: Vec<u64> = [program.sp_register.as_str()]
        .into_iter()
        .chain(probe.emu.arch.gp_regs.iter().copied().filter(|r| {
            matches!(r.to_ascii_uppercase().as_str(), "RBP" | "EBP" | "X29" | "FP" | "R11")
        }))
        .filter_map(|r| program.register(r))
        .map(|(_, o, _)| o)
        .collect();

    // Observed indirect targets.
    let mut indirect: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for s in &trace.steps {
        if matches!(s.flow, Flow::Indirect | Flow::Call { target: None }) {
            indirect.entry(s.pc).or_default().insert(s.next_pc);
        }
    }

    let mut nodes: BTreeMap<u64, Node> = BTreeMap::new();
    let mut call_sites: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new(); // callee -> call pcs
    let mut returns: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new(); // function -> return pcs
    let mut cfgs = Vec::new();
    for &f in &functions {
        if let Some(cfg) = code.function(f) {
            cfgs.push(cfg.clone());
        }
    }
    let all_pcs: BTreeSet<u64> = cfgs
        .iter()
        .flat_map(|c| c.insns.keys().copied())
        .collect();
    for cfg in &cfgs {
        let mut leaders: BTreeSet<u64> = BTreeSet::from([cfg.start]);
        for insn in cfg.insns.values() {
            match insn.flow {
                Flow::Branch(t) => {
                    leaders.insert(t);
                    leaders.insert(insn.fall_through());
                }
                Flow::CBranch { target } => {
                    leaders.insert(target);
                    leaders.insert(insn.fall_through());
                }
                Flow::Call { .. } | Flow::Return | Flow::Indirect => {
                    leaders.insert(insn.fall_through());
                }
                Flow::Fall => {}
            }
        }
        let mut env: BTreeMap<(u64, u64), Val> = BTreeMap::new();
        for insn in cfg.insns.values() {
            if leaders.contains(&insn.pc) {
                env.clear();
                for &b in &base_regs {
                    env.insert((program.register_space, b), Val::Base(b, 0));
                }
            }
            let mut node = node_for(program, insn, cfg.start, &mut env, &syscall_regs, result_reg);
            node.control_parents = cfg
                .control_parents
                .get(&insn.pc)
                .cloned()
                .unwrap_or_default();
            node.succs = match insn.flow {
                Flow::Fall => vec![insn.fall_through()],
                Flow::Branch(t) => vec![t],
                Flow::CBranch { target } => vec![target, insn.fall_through()],
                Flow::Call { target } => {
                    let targets: Vec<u64> = match target {
                        Some(t) => vec![t],
                        None => indirect
                            .get(&insn.pc)
                            .map(|s| s.iter().copied().collect())
                            .unwrap_or_default(),
                    };
                    let mut succ = Vec::new();
                    for t in targets {
                        if functions.contains(&t) {
                            call_sites.entry(t).or_default().insert(insn.pc);
                            succ.push(t);
                        }
                    }
                    if succ.is_empty() {
                        succ.push(insn.fall_through());
                    }
                    succ
                }
                Flow::Return => {
                    returns.entry(cfg.start).or_default().insert(insn.pc);
                    Vec::new()
                }
                Flow::Indirect => indirect
                    .get(&insn.pc)
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default(),
            };
            node.succs.retain(|s| all_pcs.contains(s));
            nodes.insert(insn.pc, node);
        }
    }
    // Return edges: every return of a callee to every call site's
    // fall-through.
    let mut fallthrough: BTreeMap<u64, u64> = BTreeMap::new();
    for cfg in &cfgs {
        for insn in cfg.insns.values() {
            if matches!(insn.flow, Flow::Call { .. }) {
                fallthrough.insert(insn.pc, insn.fall_through());
            }
        }
    }
    for (callee, rets) in &returns {
        let sites: Vec<u64> = call_sites
            .get(callee)
            .map(|s| s.iter().filter_map(|c| fallthrough.get(c).copied()).collect())
            .unwrap_or_default();
        for r in rets {
            if let Some(n) = nodes.get_mut(r) {
                n.succs.extend(sites.iter().copied());
            }
        }
    }
    let mut preds: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (&pc, n) in &nodes {
        for &s in &n.succs {
            preds.entry(s).or_default().push(pc);
        }
    }

    // Criterion.
    let (criterion_node, criterion): (Option<u64>, BTreeSet<Abs>) = {
        let node = trace
            .step(trace.sink_step)
            .or_else(|| trace.steps.last())
            .map(|s| s.pc);
        let abs = trace
            .criterion
            .iter()
            .flat_map(|a| match *a {
                Access::Reg { offset, size } => reg_bytes(offset, size).collect::<Vec<_>>(),
                Access::Mem { addr, len } => (0..u64::from(len))
                    .map(|i| Abs::Glob(addr.wrapping_add(i)))
                    .collect(),
            })
            .collect();
        (node, abs)
    };
    let Some(criterion_node) = criterion_node.filter(|n| nodes.contains_key(n)) else {
        return Ok(StaticSliceResult {
            functions: functions.len(),
            scope_instructions: nodes.len(),
            ..StaticSliceResult::default()
        });
    };

    // Relevant sets at node entry, iterated to a fixpoint together with
    // the set of nodes forced in by control dependence.
    let mut rel_in: BTreeMap<u64, BTreeSet<Abs>> = BTreeMap::new();
    let mut in_slice: BTreeSet<u64> = BTreeSet::new();
    let mut forced: BTreeSet<u64> = BTreeSet::from([criterion_node]);
    loop {
        let mut work: Vec<u64> = nodes.keys().copied().collect();
        let mut queued: BTreeSet<u64> = work.iter().copied().collect();
        while let Some(pc) = work.pop() {
            queued.remove(&pc);
            let node = &nodes[&pc];
            let mut out: BTreeSet<Abs> = BTreeSet::new();
            if pc == criterion_node {
                out.extend(criterion.iter().copied());
            }
            for s in &node.succs {
                if let Some(r) = rel_in.get(s) {
                    out.extend(r.iter().copied());
                }
            }
            let hits = node
                .defs
                .iter()
                .any(|d| out.iter().any(|r| may_alias(*d, *r)));
            let member = hits || forced.contains(&pc);
            let mut new_in: BTreeSet<Abs> =
                out.iter().copied().filter(|r| !node.kills.contains(r)).collect();
            if member {
                new_in.extend(node.uses.iter().copied());
                in_slice.insert(pc);
            }
            if rel_in.get(&pc) != Some(&new_in) {
                rel_in.insert(pc, new_in);
                for p in preds.get(&pc).into_iter().flatten() {
                    if queued.insert(*p) {
                        work.push(*p);
                    }
                }
            }
        }
        // Control dependence, and call sites of callees in the slice.
        let mut grew = false;
        let members: Vec<u64> = in_slice.iter().copied().collect();
        for pc in members {
            let node = &nodes[&pc];
            for &b in &node.control_parents {
                grew |= forced.insert(b);
            }
            for &site in call_sites.get(&node.function).into_iter().flatten() {
                grew |= forced.insert(site);
            }
        }
        if !grew {
            break;
        }
    }

    // Explanation at the reads.
    let mut result = StaticSliceResult {
        instructions: in_slice.len(),
        scope_instructions: nodes.len(),
        functions: functions.len(),
        ..StaticSliceResult::default()
    };
    for s in &trace.steps {
        if s.input.is_empty() || !in_slice.contains(&s.pc) {
            continue;
        }
        let node = &nodes[&s.pc];
        let mut out: BTreeSet<Abs> = BTreeSet::new();
        for succ in &node.succs {
            if let Some(r) = rel_in.get(succ) {
                out.extend(r.iter().copied());
            }
        }
        let any = out
            .iter()
            .any(|a| matches!(a, Abs::MemAny | Abs::Frame(..)));
        if any {
            result.mem_any_at_input = true;
        }
        for b in &s.input {
            if any || out.contains(&Abs::Glob(b.addr)) {
                result.input_offsets.insert(b.offset);
            }
        }
    }
    Ok(result)
}
