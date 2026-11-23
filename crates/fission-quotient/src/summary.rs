//! Symbolic path summaries of one step of a P-code slice.
//!
//! One step is one call of the slice: its inputs are fresh, its context is
//! the state the previous step left. Executing the slice once over symbolic
//! inputs and a symbolic context yields every feasible path as a triple
//! `(guard, result, context')` -- the white-box transition relation the
//! quotient is computed from. A branch whose one side the solver refutes is
//! not a fork: that side is dead for every input and every state, which is
//! exactly what an opaque predicate is, and it is recorded rather than kept.
//!
//! The memory model is separated and explicit: the stack frame, the context
//! region and the register file are distinct regions addressed by offset, so
//! nothing aliases across them. Anything outside that model -- an absolute
//! address, a pointer the slice computes from data, a call, a user-defined
//! operation, a floating-point value -- stops the summary with the reason
//! instead of being approximated.

use std::collections::{BTreeMap, BTreeSet};

use fission_pcode_ir::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_solver::{SatResult, Solver, SymExpr, SymNodeId};
use serde::Serialize;

use crate::boundary::{AbiModel, QuotientBoundary, QuotientBudget, ReturnAddress};
use crate::expr::{
    Bin, and_all, as_const, bin, concat, extract, free_vars, ite, konst, mask, not, resize, sext,
    truthy, widen_bool, width, zext,
};

/// Why a summary could not be computed. Each is a statement about the slice,
/// not a crash: the caller reports it as `UnsupportedSemantics`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SummaryError {
    #[error("at {address:#x}: {reason}")]
    Unsupported { address: u64, reason: String },
    #[error("budget: {0}")]
    Budget(String),
}

/// A contiguous piece of the context the slice reads or writes as a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub offset: u32,
    pub size: u32,
    /// The slot's value at the start of a step.
    pub var: SymExpr,
}

impl Slot {
    #[must_use]
    pub fn id(&self) -> SymNodeId {
        var_id(&self.var)
    }

    #[must_use]
    pub fn name(&self) -> String {
        format!("ctx[{:#x}:{}]", self.offset, self.size)
    }
}

/// One feasible path through one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSummary {
    /// Conjunction of the branch conditions this path took, over the input
    /// and slot variables.
    pub guard: SymExpr,
    /// The observed result: the low `output_bits` of the return register.
    pub output: SymExpr,
    /// Each slot's value when the step returns, in slot order.
    pub post: Vec<SymExpr>,
    /// P-code blocks the path ran, in order.
    pub blocks: Vec<u32>,
}

/// A branch side no input and no state can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PrunedBranch {
    pub address: u64,
    /// Whether the refuted side is the taken one.
    pub taken: bool,
}

#[derive(Debug, Clone)]
pub struct SliceSummary {
    /// One variable per input port.
    pub inputs: Vec<SymExpr>,
    pub slots: Vec<Slot>,
    pub paths: Vec<PathSummary>,
    pub pruned: Vec<PrunedBranch>,
    /// Feasibility questions the solver could not decide; both sides were
    /// kept, which over-approximates the path set but never loses one.
    pub solver_unknowns: u32,
}

impl SliceSummary {
    /// Human-readable names for every variable the summary introduced.
    #[must_use]
    pub fn names(&self, boundary: &QuotientBoundary) -> BTreeMap<SymNodeId, String> {
        let mut names = BTreeMap::new();
        for (port, var) in boundary.inputs.iter().zip(&self.inputs) {
            names.insert(var_id(var), port.name.clone());
        }
        for slot in &self.slots {
            names.insert(slot.id(), slot.name());
        }
        names
    }
}

pub(crate) fn var_id(var: &SymExpr) -> SymNodeId {
    match var {
        SymExpr::Var { id, .. } => *id,
        other => panic!("not a variable: {other:?}"),
    }
}

/// Summarize one step of `function` over `boundary`.
pub fn summarize(
    function: &PcodeFunction,
    abi: &AbiModel,
    boundary: &QuotientBoundary,
    budget: &QuotientBudget,
) -> Result<SliceSummary, SummaryError> {
    validate_boundary(abi, boundary)?;
    let inputs = boundary
        .inputs
        .iter()
        .map(|port| SymExpr::new_var(&port.name, port.bits))
        .collect::<Vec<_>>();

    // First pass: byte-granular context, to learn which pieces the slice
    // reads and writes as units. Second pass: one variable per piece, so
    // summaries name `ctx[0x4:4]` rather than four bytes.
    let size = boundary.context.initial.len() as u32;
    let bytes = (0..size)
        .map(|offset| Slot {
            offset,
            size: 1,
            var: SymExpr::new_var(&format!("ctx_byte_{offset:x}"), 8),
        })
        .collect::<Vec<_>>();
    let probe = Executor::new(function, abi, boundary, budget, &inputs, bytes).run()?;
    let slots = merge_ranges(&probe.context_accesses)
        .into_iter()
        .map(|(offset, size)| Slot {
            offset,
            size,
            var: SymExpr::new_var(&format!("ctx_{offset:x}"), size * 8),
        })
        .collect::<Vec<_>>();
    let run = Executor::new(function, abi, boundary, budget, &inputs, slots.clone()).run()?;
    Ok(SliceSummary {
        inputs,
        slots,
        paths: run.paths,
        pruned: run.pruned.into_iter().collect(),
        solver_unknowns: run.solver_unknowns,
    })
}

fn validate_boundary(abi: &AbiModel, boundary: &QuotientBoundary) -> Result<(), SummaryError> {
    let unsupported = |reason: String| SummaryError::Unsupported {
        address: boundary.entry,
        reason,
    };
    let mut used = BTreeSet::new();
    let slots = boundary
        .inputs
        .iter()
        .map(|port| port.arg_index)
        .chain([boundary.context.arg_index]);
    for index in slots {
        if index >= abi.arguments.len() {
            return Err(unsupported(format!(
                "argument {index} is passed on the stack, which the boundary does not model"
            )));
        }
        if !used.insert(index) {
            return Err(unsupported(format!("argument {index} is named twice")));
        }
    }
    for port in &boundary.inputs {
        if port.bits == 0 || port.bits > 64 || port.bits % 8 != 0 {
            return Err(unsupported(format!(
                "input {} has {} bits; ports are whole bytes up to 64 bits",
                port.name, port.bits
            )));
        }
        if port.bits > abi.arguments[port.arg_index].size * 8 {
            return Err(unsupported(format!(
                "input {} is wider than its argument register",
                port.name
            )));
        }
    }
    if boundary.output_bits == 0
        || boundary.output_bits % 8 != 0
        || boundary.output_bits > abi.return_value.size * 8
    {
        return Err(unsupported(format!(
            "a {}-bit result does not fit the return register",
            boundary.output_bits
        )));
    }
    if boundary.context.initial.is_empty() {
        return Err(unsupported("the context region is empty".to_string()));
    }
    Ok(())
}

/// Union of overlapping `(offset, size)` accesses, in offset order.
fn merge_ranges(accesses: &BTreeSet<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for &(offset, size) in accesses {
        let end = offset + size;
        match merged.last_mut() {
            Some((start, len)) if offset < *start + *len => *len = (*len).max(end - *start),
            _ => merged.push((offset, size)),
        }
    }
    merged
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Region {
    Stack,
    Context,
}

/// A value in the separated memory model.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Val {
    Bits(SymExpr),
    /// An address `region + offset`, `bits` wide. Its absolute value is
    /// never known; only offsets within one region are compared.
    Ptr {
        region: Region,
        offset: i64,
        bits: u32,
    },
    /// A value the model cannot give meaning to (a pointer's flags, a read
    /// of an undefined byte). Harmless until something observes it.
    Poison(&'static str),
}

#[derive(Debug, Clone)]
struct Cell {
    size: u32,
    val: Val,
}

/// Byte-addressed storage holding values in the pieces they were written.
#[derive(Debug, Clone, Default)]
struct Cells(BTreeMap<i64, Cell>);

impl Cells {
    fn covering(&self, byte: i64) -> Option<(i64, &Cell)> {
        self.0
            .range(..=byte)
            .next_back()
            .filter(|(start, cell)| byte < **start + i64::from(cell.size))
            .map(|(start, cell)| (*start, cell))
    }

    /// The bytes `[first, first + count)` of a cell, as a value.
    fn piece(cell: &Cell, first: u32, count: u32, big_endian: bool) -> Val {
        match &cell.val {
            Val::Bits(value) => {
                let lsb = if big_endian {
                    cell.size - first - count
                } else {
                    first
                };
                Val::Bits(extract(value.clone(), lsb * 8, count * 8))
            }
            Val::Ptr { .. } => Val::Poison("part of a pointer"),
            Val::Poison(reason) => Val::Poison(reason),
        }
    }

    /// Read `size` bytes at `start`; `init` gives the value of a byte
    /// nothing has written.
    fn read(
        &self,
        start: i64,
        size: u32,
        big_endian: bool,
        init: &mut dyn FnMut(i64) -> Val,
    ) -> Val {
        if let Some(cell) = self.0.get(&start)
            && cell.size == size
        {
            return cell.val.clone();
        }
        // One value covering the whole read is narrowed in one piece, not
        // taken apart into bytes and put back together.
        if let Some((cell_start, cell)) = self.covering(start)
            && start + i64::from(size) <= cell_start + i64::from(cell.size)
        {
            return Self::piece(cell, (start - cell_start) as u32, size, big_endian);
        }
        let mut bytes = Vec::with_capacity(size as usize);
        for byte in start..start + i64::from(size) {
            let value = match self.covering(byte) {
                Some((cell_start, cell)) => {
                    Self::piece(cell, (byte - cell_start) as u32, 1, big_endian)
                }
                None => init(byte),
            };
            match value {
                Val::Bits(value) => bytes.push(value),
                other => return other,
            }
        }
        if big_endian {
            bytes.reverse();
        }
        // `bytes` is least significant first.
        let mut value = bytes.remove(0);
        for byte in bytes {
            value = concat(byte, value);
        }
        Val::Bits(value)
    }

    fn write(&mut self, start: i64, size: u32, val: Val, big_endian: bool) {
        let end = start + i64::from(size);
        let overlapping = self
            .0
            .range(..end)
            .filter(|(cell_start, cell)| **cell_start + i64::from(cell.size) > start)
            .map(|(cell_start, _)| *cell_start)
            .collect::<Vec<_>>();
        for cell_start in overlapping {
            let cell = self.0.remove(&cell_start).expect("listed above");
            let cell_end = cell_start + i64::from(cell.size);
            // Keep whatever part of the old cell the write does not cover.
            for (from, to) in [
                (cell_start, start.min(cell_end)),
                (end.max(cell_start), cell_end),
            ] {
                if from >= to {
                    continue;
                }
                let piece = Self::piece(
                    &cell,
                    (from - cell_start) as u32,
                    (to - from) as u32,
                    big_endian,
                );
                self.0.insert(
                    from,
                    Cell {
                        size: (to - from) as u32,
                        val: piece,
                    },
                );
            }
        }
        self.0.insert(start, Cell { size, val });
    }
}

#[derive(Debug, Clone)]
struct PathState {
    spaces: BTreeMap<u64, Cells>,
    stack: Cells,
    context: Cells,
    conjuncts: Vec<SymExpr>,
    blocks: Vec<u32>,
    ops: usize,
}

struct RunResult {
    paths: Vec<PathSummary>,
    pruned: BTreeSet<PrunedBranch>,
    solver_unknowns: u32,
    context_accesses: BTreeSet<(u32, u32)>,
}

struct Executor<'a> {
    function: &'a PcodeFunction,
    abi: &'a AbiModel,
    boundary: &'a QuotientBoundary,
    budget: &'a QuotientBudget,
    inputs: &'a [SymExpr],
    slots: Vec<Slot>,
    block_at: BTreeMap<u64, usize>,
    /// Caller values of registers the slice reads without writing, one
    /// variable per pointer-sized word, shared by every path.
    live_ins: BTreeMap<u64, SymExpr>,
    return_address: SymExpr,
    allowed: BTreeSet<SymNodeId>,
    result: RunResult,
}

type Step<T> = Result<T, SummaryError>;

impl<'a> Executor<'a> {
    fn new(
        function: &'a PcodeFunction,
        abi: &'a AbiModel,
        boundary: &'a QuotientBoundary,
        budget: &'a QuotientBudget,
        inputs: &'a [SymExpr],
        slots: Vec<Slot>,
    ) -> Self {
        let block_at = function
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.start_address, index))
            .collect();
        let allowed = inputs
            .iter()
            .map(var_id)
            .chain(slots.iter().map(Slot::id))
            .collect();
        Self {
            function,
            abi,
            boundary,
            budget,
            inputs,
            slots,
            block_at,
            live_ins: BTreeMap::new(),
            return_address: SymExpr::new_var("return_address", abi.pointer_bits),
            allowed,
            result: RunResult {
                paths: Vec::new(),
                pruned: BTreeSet::new(),
                solver_unknowns: 0,
                context_accesses: BTreeSet::new(),
            },
        }
    }

    fn unsupported<T>(&self, op: &PcodeOp, reason: impl Into<String>) -> Step<T> {
        Err(SummaryError::Unsupported {
            address: op.address,
            reason: reason.into(),
        })
    }

    fn initial_state(&mut self) -> PathState {
        let mut registers = Cells::default();
        let abi = self.abi;
        let big = abi.big_endian;
        let sp = &abi.stack_pointer;
        registers.write(
            sp.offset as i64,
            sp.size,
            Val::Ptr {
                region: Region::Stack,
                offset: 0,
                bits: sp.size * 8,
            },
            big,
        );
        let context = &abi.arguments[self.boundary.context.arg_index];
        registers.write(
            context.offset as i64,
            context.size,
            Val::Ptr {
                region: Region::Context,
                offset: 0,
                bits: context.size * 8,
            },
            big,
        );
        for (port, var) in self.boundary.inputs.iter().zip(self.inputs) {
            let slot = &abi.arguments[port.arg_index];
            let value = if port.bits == slot.size * 8 {
                var.clone()
            } else {
                let garbage = SymExpr::new_var(
                    &format!("{}_upper_bits", slot.name),
                    slot.size * 8 - port.bits,
                );
                concat(garbage, var.clone())
            };
            registers.write(slot.offset as i64, slot.size, Val::Bits(value), big);
        }
        if let ReturnAddress::Register(link) = &abi.return_address {
            let value = resize(self.return_address.clone(), link.size * 8);
            registers.write(link.offset as i64, link.size, Val::Bits(value), big);
        }
        PathState {
            spaces: BTreeMap::from([(abi.register_space, registers)]),
            stack: Cells::default(),
            context: Cells::default(),
            conjuncts: Vec::new(),
            blocks: Vec::new(),
            ops: 0,
        }
    }

    fn run(mut self) -> Step<RunResult> {
        if self.function.blocks.is_empty() {
            return Err(SummaryError::Unsupported {
                address: self.boundary.entry,
                reason: "the slice has no P-code".to_string(),
            });
        }
        let entry = self
            .block_at
            .get(&self.boundary.entry)
            .copied()
            .unwrap_or(0);
        let initial = self.initial_state();
        let mut work = vec![(initial, entry, 0usize)];
        while let Some((state, block, op_index)) = work.pop() {
            if self.result.paths.len() + work.len() + 1 > self.budget.max_paths {
                return Err(SummaryError::Budget(format!(
                    "more than {} feasible paths in one step",
                    self.budget.max_paths
                )));
            }
            self.run_path(state, block, op_index, &mut work)?;
        }
        Ok(self.result)
    }

    fn run_path(
        &mut self,
        mut state: PathState,
        mut block: usize,
        mut op_index: usize,
        work: &mut Vec<(PathState, usize, usize)>,
    ) -> Step<()> {
        let function = self.function;
        if op_index == 0 {
            state.blocks.push(function.blocks[block].index);
        }
        loop {
            let ops = &function.blocks[block].ops;
            if op_index >= ops.len() {
                block = self.fallthrough(block, None)?;
                op_index = 0;
                state.blocks.push(function.blocks[block].index);
                continue;
            }
            let op = &ops[op_index];
            state.ops += 1;
            if state.ops > self.budget.max_path_ops {
                return Err(SummaryError::Budget(format!(
                    "a path ran more than {} P-code operations (a loop the inputs do not bound?)",
                    self.budget.max_path_ops
                )));
            }
            match op.opcode {
                PcodeOpcode::Branch => {
                    (block, op_index) = self.branch_target(op, block, op_index)?;
                    if op_index == 0 {
                        state.blocks.push(function.blocks[block].index);
                    }
                }
                PcodeOpcode::CBranch => {
                    let condition = match self.read(&mut state, op, &op.inputs[1])? {
                        Val::Bits(value) => truthy(value),
                        Val::Ptr { .. } => {
                            return self.unsupported(op, "branch on a pointer value");
                        }
                        Val::Poison(reason) => {
                            return self.unsupported(op, format!("branch on {reason}"));
                        }
                    };
                    let taken = self.branch_target(op, block, op_index)?;
                    let fall = if op_index + 1 < ops.len() {
                        (block, op_index + 1)
                    } else {
                        (self.fallthrough(block, Some(taken.0))?, 0)
                    };
                    let (go, fork) = match as_const(&condition) {
                        Some(1) => (taken, None),
                        Some(_) => (fall, None),
                        None => {
                            self.check_vars(op, &condition, "a branch condition")?;
                            let can_take = self.feasible(&state.conjuncts, &condition);
                            let can_fall = self.feasible(&state.conjuncts, &not(condition.clone()));
                            match (can_take, can_fall) {
                                (SatResult::Unsat, SatResult::Unsat) => return Ok(()),
                                (SatResult::Unsat, _) => {
                                    self.result.pruned.insert(PrunedBranch {
                                        address: op.address,
                                        taken: true,
                                    });
                                    (fall, None)
                                }
                                (_, SatResult::Unsat) => {
                                    self.result.pruned.insert(PrunedBranch {
                                        address: op.address,
                                        taken: false,
                                    });
                                    (taken, None)
                                }
                                _ => (taken, Some((fall, condition))),
                            }
                        }
                    };
                    if let Some((other, condition)) = fork {
                        let mut alternative = state.clone();
                        alternative.conjuncts.push(not(condition.clone()));
                        work.push((alternative, other.0, other.1));
                        state.conjuncts.push(condition);
                    }
                    (block, op_index) = go;
                    if op_index == 0 {
                        state.blocks.push(function.blocks[block].index);
                    }
                }
                PcodeOpcode::Return => return self.finish(state, op),
                PcodeOpcode::Call | PcodeOpcode::CallInd => {
                    return self
                        .unsupported(op, "a call: the callee's effects are outside the slice");
                }
                PcodeOpcode::CallOther => {
                    return self.unsupported(
                        op,
                        "a user-defined operation (an effect the P-code does not define)",
                    );
                }
                PcodeOpcode::BranchInd => {
                    return self.unsupported(op, "an indirect branch");
                }
                _ => {
                    self.step(&mut state, op)?;
                    op_index += 1;
                }
            }
        }
    }

    /// Ask whether `conjuncts && condition` has a solution. `Unknown` counts
    /// as possible -- keeping a path is safe, dropping one is not.
    fn feasible(&mut self, conjuncts: &[SymExpr], condition: &SymExpr) -> SatResult {
        let mut solver = Solver::new();
        solver.set_conflict_budget(self.budget.conflict_budget);
        for conjunct in conjuncts {
            solver.assert(conjunct.clone());
        }
        let result = solver
            .check_sat_with_oracle(None, std::slice::from_ref(condition))
            .unwrap_or(SatResult::Unknown);
        if result == SatResult::Unknown {
            self.result.solver_unknowns += 1;
        }
        result
    }

    fn branch_target(&self, op: &PcodeOp, block: usize, op_index: usize) -> Step<(usize, usize)> {
        let target = &op.inputs[0];
        if target.is_constant || target.space_id == 0 {
            // A relative branch inside one instruction's P-code.
            let index = op_index as i64 + target.constant_val;
            let ops = &self.function.blocks[block].ops;
            if index < 0 || index as usize > ops.len() {
                return self.unsupported(op, "a relative branch leaves its block");
            }
            if index as usize == ops.len() {
                return Ok((self.fallthrough(block, None)?, 0));
            }
            return Ok((block, index as usize));
        }
        if target.space_id != self.abi.ram_space {
            return self.unsupported(op, "a branch into a non-code space");
        }
        match self.block_at.get(&target.offset) {
            Some(index) => Ok((*index, 0)),
            None => self.unsupported(
                op,
                format!("a branch to {:#x}, outside the slice", target.offset),
            ),
        }
    }

    /// The block control reaches by falling off the end of `block`, or off
    /// a conditional branch whose taken side is `taken`.
    fn fallthrough(&self, block: usize, taken: Option<usize>) -> Step<usize> {
        let here = &self.function.blocks[block];
        let index_of = |successor: u32| {
            self.function
                .blocks
                .iter()
                .position(|candidate| candidate.index == successor)
        };
        let successors = here
            .successors
            .iter()
            .filter_map(|successor| index_of(*successor))
            .collect::<Vec<_>>();
        let candidates = match taken {
            Some(taken) => {
                let others = successors
                    .iter()
                    .copied()
                    .filter(|successor| *successor != taken)
                    .collect::<Vec<_>>();
                if others.is_empty() && successors.contains(&taken) {
                    vec![taken]
                } else {
                    others
                }
            }
            None => successors,
        };
        match candidates.as_slice() {
            [only] => Ok(*only),
            _ => Err(SummaryError::Unsupported {
                address: here.start_address,
                reason: format!(
                    "block {} has no single fall-through successor ({:?})",
                    here.index, here.successors
                ),
            }),
        }
    }

    fn finish(&mut self, mut state: PathState, op: &PcodeOp) -> Step<()> {
        let abi = self.abi;
        let ret = &abi.return_value;
        let bytes = self.boundary.output_bits / 8;
        let start = if abi.big_endian {
            ret.offset + u64::from(ret.size - bytes)
        } else {
            ret.offset
        };
        let output = match self.read_register(&mut state, abi.register_space, start as i64, bytes) {
            Val::Bits(value) => value,
            Val::Ptr { .. } => return self.unsupported(op, "the result is an address"),
            Val::Poison(reason) => {
                return self.unsupported(op, format!("the result is {reason}"));
            }
        };
        self.check_vars(op, &output, "the result")?;
        let mut post = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            match self.context_value(&state, i64::from(slot.offset), slot.size) {
                Val::Bits(value) => {
                    self.check_vars(op, &value, "the context")?;
                    post.push(value);
                }
                Val::Ptr { .. } => {
                    return self.unsupported(op, "an address is left in the context");
                }
                Val::Poison(reason) => {
                    return self.unsupported(op, format!("the context is left holding {reason}"));
                }
            }
        }
        for conjunct in &state.conjuncts {
            self.check_vars(op, conjunct, "a branch condition")?;
        }
        self.result.paths.push(PathSummary {
            guard: and_all(state.conjuncts),
            output,
            post,
            blocks: state.blocks,
        });
        Ok(())
    }

    /// Only the declared inputs and the context may reach what is observed.
    fn check_vars(&self, op: &PcodeOp, value: &SymExpr, what: &str) -> Step<()> {
        let stray = free_vars(value)
            .into_iter()
            .filter(|id| !self.allowed.contains(id))
            .collect::<Vec<_>>();
        if stray.is_empty() {
            return Ok(());
        }
        let mut names = BTreeSet::new();
        value.visit_preorder(&mut |node| {
            if let SymExpr::Var { id, name, .. } = node
                && stray.contains(id)
            {
                names.insert(name.clone());
            }
        });
        self.unsupported(
            op,
            format!(
                "{what} depends on values outside the boundary: {}",
                names.into_iter().collect::<Vec<_>>().join(", ")
            ),
        )
    }

    fn read_register(&mut self, state: &mut PathState, space: u64, start: i64, size: u32) -> Val {
        let big = self.abi.big_endian;
        let word = i64::from(self.abi.pointer_bits / 8);
        let register_space = self.abi.register_space;
        let live_ins = &mut self.live_ins;
        let cells = state.spaces.entry(space).or_default();
        cells.read(start, size, big, &mut |byte| {
            if space != register_space {
                return Val::Poison("a temporary read before it is written");
            }
            let base = byte - byte.rem_euclid(word);
            let value = live_ins
                .entry(base as u64)
                .or_insert_with(|| {
                    SymExpr::new_var(&format!("live_in_reg_{base:x}"), word as u32 * 8)
                })
                .clone();
            let index = (byte - base) as u32;
            let significance = if big { word as u32 - 1 - index } else { index };
            Val::Bits(extract(value, significance * 8, 8))
        })
    }

    /// Read the context, recording the access so the second pass makes it a
    /// slot.
    fn read_context(
        &mut self,
        state: &PathState,
        op: &PcodeOp,
        start: i64,
        size: u32,
    ) -> Step<Val> {
        let limit = self.boundary.context.initial.len() as i64;
        if start < 0 || start + i64::from(size) > limit {
            return self.unsupported(
                op,
                format!("context access [{start:#x}, +{size}) outside its {limit} bytes"),
            );
        }
        self.result.context_accesses.insert((start as u32, size));
        Ok(self.context_value(state, start, size))
    }

    /// The context's current value at `[start, +size)`, without recording
    /// an access.
    fn context_value(&self, state: &PathState, start: i64, size: u32) -> Val {
        let big = self.abi.big_endian;
        let slots = &self.slots;
        state.context.read(start, size, big, &mut |byte| {
            let Some(slot) = slots.iter().find(|slot| {
                i64::from(slot.offset) <= byte && byte < i64::from(slot.offset + slot.size)
            }) else {
                return Val::Poison("a context byte the first pass never saw");
            };
            let index = (byte - i64::from(slot.offset)) as u32;
            let significance = if big { slot.size - 1 - index } else { index };
            Val::Bits(extract(slot.var.clone(), significance * 8, 8))
        })
    }

    fn read(&mut self, state: &mut PathState, op: &PcodeOp, varnode: &Varnode) -> Step<Val> {
        if varnode.size == 0 || varnode.size > 8 {
            return self.unsupported(op, format!("a {}-byte value", varnode.size));
        }
        let bits = varnode.size * 8;
        if varnode.is_constant || varnode.space_id == 0 {
            return Ok(Val::Bits(konst(
                varnode.constant_val as u64 & mask(bits),
                bits,
            )));
        }
        if varnode.space_id == self.abi.ram_space {
            return self.unsupported(
                op,
                format!(
                    "absolute memory at {:#x} (globals are outside the model)",
                    varnode.offset
                ),
            );
        }
        Ok(self.read_register(state, varnode.space_id, varnode.offset as i64, varnode.size))
    }

    fn write(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        varnode: &Varnode,
        val: Val,
    ) -> Step<()> {
        if varnode.size == 0 || varnode.size > 8 {
            return self.unsupported(op, format!("a {}-byte value", varnode.size));
        }
        if varnode.space_id == self.abi.ram_space {
            return self.unsupported(
                op,
                format!(
                    "absolute memory at {:#x} (globals are outside the model)",
                    varnode.offset
                ),
            );
        }
        state.spaces.entry(varnode.space_id).or_default().write(
            varnode.offset as i64,
            varnode.size,
            val,
            self.abi.big_endian,
        );
        Ok(())
    }

    fn step(&mut self, state: &mut PathState, op: &PcodeOp) -> Step<()> {
        match op.opcode {
            PcodeOpcode::Load => {
                let output = op.output.as_ref().expect("validated: LOAD has an output");
                let value = match self.read(state, op, &op.inputs[1])? {
                    Val::Ptr {
                        region: Region::Stack,
                        offset,
                        ..
                    } => self.read_stack(state, offset, output.size),
                    Val::Ptr {
                        region: Region::Context,
                        offset,
                        ..
                    } => self.read_context(state, op, offset, output.size)?,
                    Val::Bits(address) => {
                        return self.unsupported(
                            op,
                            match as_const(&address) {
                                Some(at) => format!("a load from absolute address {at:#x}"),
                                None => "a load through an address computed from data".to_string(),
                            },
                        );
                    }
                    Val::Poison(reason) => {
                        return self.unsupported(op, format!("a load through {reason}"));
                    }
                };
                self.write(state, op, output, value)
            }
            PcodeOpcode::Store => {
                let value = self.read(state, op, &op.inputs[2])?;
                let size = op.inputs[2].size;
                match self.read(state, op, &op.inputs[1])? {
                    Val::Ptr {
                        region: Region::Stack,
                        offset,
                        ..
                    } => {
                        state.stack.write(offset, size, value, self.abi.big_endian);
                        Ok(())
                    }
                    Val::Ptr {
                        region: Region::Context,
                        offset,
                        ..
                    } => {
                        match &value {
                            Val::Bits(_) => {}
                            Val::Ptr { .. } => {
                                return self.unsupported(op, "an address is stored in the context");
                            }
                            Val::Poison(reason) => {
                                return self
                                    .unsupported(op, format!("the context is given {reason}"));
                            }
                        }
                        // Reading first checks the bounds and records the
                        // access, so a write-only piece still becomes a slot.
                        self.read_context(state, op, offset, size)?;
                        state
                            .context
                            .write(offset, size, value, self.abi.big_endian);
                        Ok(())
                    }
                    Val::Bits(address) => self.unsupported(
                        op,
                        match as_const(&address) {
                            Some(at) => format!("a store to absolute address {at:#x}"),
                            None => "a store through an address computed from data".to_string(),
                        },
                    ),
                    Val::Poison(reason) => {
                        self.unsupported(op, format!("a store through {reason}"))
                    }
                }
            }
            _ => {
                let Some(output) = op.output.as_ref() else {
                    return self.unsupported(op, format!("{:?} without an output", op.opcode));
                };
                let inputs = op
                    .inputs
                    .iter()
                    .map(|input| self.read(state, op, input))
                    .collect::<Step<Vec<_>>>()?;
                let value = self.apply(op, output.size * 8, inputs)?;
                self.write(state, op, output, value)
            }
        }
    }

    fn read_stack(&mut self, state: &mut PathState, offset: i64, size: u32) -> Val {
        let pointer_bytes = i64::from(self.abi.pointer_bits / 8);
        let big = self.abi.big_endian;
        let on_stack = matches!(self.abi.return_address, ReturnAddress::Stack);
        let return_address = self.return_address.clone();
        state.stack.read(offset, size, big, &mut |byte| {
            if on_stack && (0..pointer_bytes).contains(&byte) {
                let index = byte as u32;
                let significance = if big {
                    pointer_bytes as u32 - 1 - index
                } else {
                    index
                };
                Val::Bits(extract(return_address.clone(), significance * 8, 8))
            } else if byte < 0 {
                Val::Poison("a stack byte read before it is written")
            } else {
                Val::Poison("the caller's frame")
            }
        })
    }

    fn apply(&self, op: &PcodeOp, out_bits: u32, inputs: Vec<Val>) -> Step<Val> {
        use PcodeOpcode as O;
        if let Some(Val::Poison(reason)) = inputs.iter().find(|v| matches!(v, Val::Poison(_))) {
            return Ok(Val::Poison(reason));
        }
        if inputs.iter().any(|v| matches!(v, Val::Ptr { .. })) {
            return Ok(pointer_op(op.opcode, out_bits, &inputs));
        }
        let bits: Vec<SymExpr> = inputs
            .into_iter()
            .map(|v| match v {
                Val::Bits(value) => value,
                _ => unreachable!("pointers and poison handled above"),
            })
            .collect();
        let a = || bits[0].clone();
        let b = || bits[1].clone();
        let binary = |kind: Bin| Ok(Val::Bits(bin(kind, a(), b())));
        let compare = |kind: Bin| Ok(Val::Bits(widen_bool(bin(kind, a(), b()), out_bits)));
        match op.opcode {
            O::Copy => Ok(Val::Bits(resize(a(), out_bits))),
            O::IntAdd => binary(Bin::Add),
            O::IntSub => binary(Bin::Sub),
            O::IntMult => binary(Bin::Mul),
            O::IntAnd => binary(Bin::And),
            O::IntOr => binary(Bin::Or),
            O::IntXor => binary(Bin::Xor),
            O::IntEqual => compare(Bin::Eq),
            O::IntNotEqual => compare(Bin::Neq),
            O::IntLess => compare(Bin::Ult),
            O::IntLessEqual => compare(Bin::Ule),
            O::IntSLess => compare(Bin::Slt),
            O::IntSLessEqual => compare(Bin::Sle),
            O::IntZExt => Ok(Val::Bits(zext(a(), out_bits))),
            O::IntSExt => Ok(Val::Bits(sext(a(), out_bits))),
            O::Int2Comp => Ok(Val::Bits(bin(Bin::Sub, konst(0, width(&bits[0])), a()))),
            O::IntNegate => {
                let w = width(&bits[0]);
                Ok(Val::Bits(bin(Bin::Xor, a(), konst(mask(w), w))))
            }
            O::IntLeft | O::IntRight | O::IntSRight => {
                let kind = match op.opcode {
                    O::IntLeft => Bin::Shl,
                    O::IntRight => Bin::Lshr,
                    _ => Bin::Ashr,
                };
                Ok(Val::Bits(shift(kind, a(), b())))
            }
            O::IntCarry => {
                let sum = bin(Bin::Add, a(), b());
                Ok(Val::Bits(widen_bool(bin(Bin::Ult, sum, a()), out_bits)))
            }
            O::IntSCarry => {
                let w = width(&bits[0]);
                let sum = bin(Bin::Add, a(), b());
                let both = bin(
                    Bin::And,
                    bin(Bin::Xor, a(), sum.clone()),
                    bin(Bin::Xor, b(), sum),
                );
                Ok(Val::Bits(widen_bool(extract(both, w - 1, 1), out_bits)))
            }
            O::IntSBorrow => {
                let w = width(&bits[0]);
                let difference = bin(Bin::Sub, a(), b());
                let both = bin(
                    Bin::And,
                    bin(Bin::Xor, a(), b()),
                    bin(Bin::Xor, a(), difference),
                );
                Ok(Val::Bits(widen_bool(extract(both, w - 1, 1), out_bits)))
            }
            O::BoolNegate => Ok(Val::Bits(widen_bool(not(truthy(a())), out_bits))),
            O::BoolAnd => Ok(Val::Bits(widen_bool(
                bin(Bin::And, truthy(a()), truthy(b())),
                out_bits,
            ))),
            O::BoolOr => Ok(Val::Bits(widen_bool(
                bin(Bin::Or, truthy(a()), truthy(b())),
                out_bits,
            ))),
            O::BoolXor => Ok(Val::Bits(widen_bool(
                bin(Bin::Xor, truthy(a()), truthy(b())),
                out_bits,
            ))),
            O::PopCount => {
                let value = a();
                let count = (0..width(&value))
                    .map(|bit| zext(extract(value.clone(), bit, 1), out_bits))
                    .fold(konst(0, out_bits), |acc, bit| bin(Bin::Add, acc, bit));
                Ok(Val::Bits(count))
            }
            O::SubPiece => {
                let value = a();
                let Some(skip) = as_const(&bits[1]) else {
                    return self.unsupported(op, "SUBPIECE with a symbolic offset");
                };
                let w = width(&value);
                let lsb = skip.saturating_mul(8);
                if lsb >= u64::from(w) {
                    return Ok(Val::Bits(konst(0, out_bits)));
                }
                let lsb = lsb as u32;
                let take = (w - lsb).min(out_bits);
                Ok(Val::Bits(zext(extract(value, lsb, take), out_bits)))
            }
            O::Piece => Ok(Val::Bits(resize(concat(a(), b()), out_bits))),
            O::IntDiv | O::IntRem | O::IntSDiv | O::IntSRem => {
                match (as_const(&bits[0]), as_const(&bits[1])) {
                    (Some(x), Some(y)) if y != 0 => {
                        let w = width(&bits[0]);
                        let sx = signed_of(x, w);
                        let sy = signed_of(y, w);
                        let value = match op.opcode {
                            O::IntDiv => x / y,
                            O::IntRem => x % y,
                            O::IntSDiv => sx.wrapping_div(sy) as u64,
                            _ => sx.wrapping_rem(sy) as u64,
                        };
                        Ok(Val::Bits(konst(value, out_bits)))
                    }
                    _ => self.unsupported(
                        op,
                        "division by a value that may be zero (undefined in P-code)",
                    ),
                }
            }
            other => self.unsupported(op, format!("the {other:?} operation")),
        }
    }
}

fn signed_of(value: u64, bits: u32) -> i64 {
    if bits >= 64 {
        value as i64
    } else {
        ((value << (64 - bits)) as i64) >> (64 - bits)
    }
}

/// A shift whose amount may be wider or narrower than the shifted value.
fn shift(kind: Bin, value: SymExpr, amount: SymExpr) -> SymExpr {
    let bits = width(&value);
    let amount_bits = width(&amount);
    if amount_bits <= bits {
        return bin(kind, value, zext(amount, bits));
    }
    let overflow = match kind {
        Bin::Ashr => {
            let sign = extract(value.clone(), bits - 1, 1);
            ite(sign, konst(mask(bits), bits), konst(0, bits))
        }
        _ => konst(0, bits),
    };
    let too_far = bin(
        Bin::Ule,
        konst(u64::from(bits), amount_bits),
        amount.clone(),
    );
    ite(
        too_far,
        overflow,
        bin(kind, value, extract(amount, 0, bits)),
    )
}

/// Operations with an address operand. Offsets within one region are known;
/// anything that would need the region's absolute address is poison.
fn pointer_op(opcode: PcodeOpcode, out_bits: u32, inputs: &[Val]) -> Val {
    let constant = |v: &Val| match v {
        Val::Bits(value) => as_const(value).map(|c| signed_of(c, width(value))),
        _ => None,
    };
    match (opcode, inputs) {
        (
            PcodeOpcode::Copy,
            [
                Val::Ptr {
                    region,
                    offset,
                    bits,
                },
            ],
        ) if *bits == out_bits => Val::Ptr {
            region: *region,
            offset: *offset,
            bits: *bits,
        },
        (
            PcodeOpcode::IntAdd,
            [
                Val::Ptr {
                    region,
                    offset,
                    bits,
                },
                other,
            ],
        )
        | (
            PcodeOpcode::IntAdd,
            [
                other,
                Val::Ptr {
                    region,
                    offset,
                    bits,
                },
            ],
        ) => match constant(other) {
            Some(delta) => Val::Ptr {
                region: *region,
                offset: offset.wrapping_add(delta),
                bits: *bits,
            },
            None => Val::Poison("an address offset by data"),
        },
        (
            PcodeOpcode::IntSub,
            [
                Val::Ptr {
                    region,
                    offset,
                    bits,
                },
                other,
            ],
        ) if constant(other).is_some() => Val::Ptr {
            region: *region,
            offset: offset.wrapping_sub(constant(other).expect("checked")),
            bits: *bits,
        },
        (
            PcodeOpcode::IntSub,
            [
                Val::Ptr {
                    region: a,
                    offset: x,
                    ..
                },
                Val::Ptr {
                    region: b,
                    offset: y,
                    ..
                },
            ],
        ) if a == b => Val::Bits(konst(x.wrapping_sub(*y) as u64, out_bits)),
        (
            PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual,
            [
                Val::Ptr {
                    region: a,
                    offset: x,
                    ..
                },
                Val::Ptr {
                    region: b,
                    offset: y,
                    ..
                },
            ],
        ) if a == b => {
            let equal = x == y;
            let truth = if opcode == PcodeOpcode::IntEqual {
                equal
            } else {
                !equal
            };
            Val::Bits(konst(u64::from(truth), out_bits))
        }
        _ => Val::Poison("arithmetic on an address the model cannot place"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::eval;

    fn bits(val: &Val) -> &SymExpr {
        match val {
            Val::Bits(value) => value,
            other => panic!("expected bits, got {other:?}"),
        }
    }

    /// A narrow write into a wide value keeps the bytes around it, in both
    /// byte orders, and reads see exactly the bytes the machine would.
    #[test]
    fn partial_overwrites_keep_the_bytes_they_do_not_cover() {
        for big_endian in [false, true] {
            let wide = SymExpr::new_var("wide", 64);
            let narrow = SymExpr::new_var("narrow", 16);
            let (wide_id, narrow_id) = (var_id(&wide), var_id(&narrow));
            let mut cells = Cells::default();
            cells.write(0, 8, Val::Bits(wide), big_endian);
            cells.write(3, 2, Val::Bits(narrow), big_endian);
            let (w, n) = (0x1122_3344_5566_7788u64, 0xaabbu64);
            // Memory as bytes, written the same way.
            let mut memory = if big_endian {
                w.to_be_bytes()
            } else {
                w.to_le_bytes()
            };
            let narrow_bytes = if big_endian {
                (n as u16).to_be_bytes()
            } else {
                (n as u16).to_le_bytes()
            };
            memory[3..5].copy_from_slice(&narrow_bytes);
            let env = BTreeMap::from([(wide_id, w), (narrow_id, n)]);
            for (start, size) in [(0, 8), (0, 4), (2, 4), (4, 4), (3, 2), (5, 3)] {
                let value = cells.read(start, size, big_endian, &mut |_| Val::Poison("unset"));
                let got = eval(bits(&value), &env).unwrap();
                let slice = &memory[start as usize..(start as usize + size as usize)];
                let want = slice.iter().enumerate().fold(0u64, |acc, (i, byte)| {
                    let shift = if big_endian {
                        (slice.len() - 1 - i) * 8
                    } else {
                        i * 8
                    };
                    acc | (u64::from(*byte) << shift)
                });
                assert_eq!(got, want, "read [{start}, +{size}) big_endian={big_endian}");
            }
        }
    }

    #[test]
    fn a_pointer_survives_whole_and_poisons_in_part() {
        let mut cells = Cells::default();
        let pointer = Val::Ptr {
            region: Region::Context,
            offset: 4,
            bits: 64,
        };
        cells.write(-8, 8, pointer.clone(), false);
        let mut unset = |_| Val::Poison("unset");
        assert_eq!(cells.read(-8, 8, false, &mut unset), pointer);
        assert!(matches!(
            cells.read(-8, 4, false, &mut unset),
            Val::Poison(_)
        ));
    }
}
