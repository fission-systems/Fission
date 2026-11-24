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
//! address, a pointer the slice computes from data, a user-defined
//! operation, a floating-point value -- stops the summary with the reason
//! instead of being approximated.
//!
//! A call is an observable effect (see [`crate::calls`]): the path records
//! its target and argument registers, the return register becomes the next
//! call-return input port, every register the convention does not preserve
//! becomes poison, and the path goes on at the call's fall-through -- or
//! ends there, when the callee is proven not to return. A jump out of the
//! slice is a call whose return is the step's.

use std::collections::{BTreeMap, BTreeSet};

use fission_pcode_ir::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_solver::{SatResult, Solver, SymExpr, SymNodeId};
use serde::Serialize;

use crate::boundary::{
    AbiModel, MemoryLayout, PointerRegion, QuotientBoundary, QuotientBudget, ReturnAddress,
};
use crate::calls::{CallFact, CallFacts, CallModel, CallSiteModel, MAX_CALLS_PER_STEP};
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
    /// Internal: the slice dereferences a pointer field that has no region
    /// yet, or reads a discovered region past its end. [`summarize`] applies
    /// the change and runs again; it never leaves this module.
    #[error("the pointer regions change: {0:?}")]
    Regions(RegionChange),
}

/// A change to the boundary's pointer regions the summary asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionChange {
    /// The pointer at `field` of `parent` (the context when `None`) is
    /// dereferenced: give it a region.
    Discover { parent: Option<usize>, field: u32 },
    /// A discovered region must hold at least `bytes`.
    Grow { region: usize, bytes: u32 },
    /// Global memory `[start, end)` at an absolute address is read or
    /// written: cover it with a global region (a nearby one grows).
    Global { start: u64, end: u64 },
    /// Memory at `register + [0, bytes)` is read: the register gets a block
    /// of its own (the thread pointer and thread-local storage).
    Register { name: String, bytes: u32 },
}

/// The bytes the binary's image holds at `[address, +len)`, zero where a
/// section holds none (`.bss`); `None` where nothing is mapped.
pub type Image<'a> = &'a dyn Fn(u64, usize) -> Option<Vec<u8>>;

/// Two global ranges closer than this share one region.
const GLOBAL_GAP: u64 = 64;

/// A contiguous piece of memory -- the context or a pointer region -- the
/// slice reads or writes as a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// Where the piece starts in the byte space of [`QuotientBoundary::layout`]
    /// (the context first, then each region).
    pub offset: u32,
    pub size: u32,
    /// The slot's value at the start of a step.
    pub var: SymExpr,
    /// The pointer region it lies in; `None` for the context.
    pub region: Option<usize>,
    /// Its name in reports: `ctx[0x4:4]`, or `ctx[0x8]->[0x10:1]` in a
    /// region.
    pub label: String,
}

impl Slot {
    #[must_use]
    pub fn id(&self) -> SymNodeId {
        var_id(&self.var)
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.label.clone()
    }
}

/// How a path's step ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathExit {
    Returned,
    /// At a call proven not to return; no later step runs.
    NoReturn,
    /// An access at `site` would leave its region's bytes. Not behaviour of
    /// the machine -- the model has nothing to say about it -- so a quotient
    /// in which this path can run is unsupported.
    LeftRegion {
        site: u64,
    },
}

/// One call a path makes, as observed: its target and the argument
/// registers its call site observes ([`CallSiteModel::arg_bits`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallObservation {
    pub site: u64,
    pub target: u64,
    /// The observed arguments, in register order, each as wide as observed.
    pub args: Vec<SymExpr>,
}

/// One feasible path through one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSummary {
    /// Conjunction of the branch conditions this path took, over the input
    /// and slot variables.
    pub guard: SymExpr,
    /// The observed result: the low `output_bits` of the return register
    /// (zero on a path that does not return).
    pub output: SymExpr,
    /// Each slot's value when the step returns, in slot order (unchanged on
    /// a path that does not return: nothing observes it again).
    pub post: Vec<SymExpr>,
    /// P-code blocks the path ran, in order.
    pub blocks: Vec<u32>,
    /// The calls the path makes, in order. The `n`th one's return is input
    /// port `declared_inputs + n`.
    pub calls: Vec<CallObservation>,
    pub exit: PathExit,
}

/// A branch side no input and no state can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PrunedBranch {
    #[serde(serialize_with = "crate::report::hex_address")]
    pub address: u64,
    /// Whether the refuted side is the taken one.
    pub taken: bool,
}

#[derive(Debug, Clone)]
pub struct SliceSummary {
    /// One variable per input port: the boundary's ports, then one per call
    /// a step makes (in dynamic order), whose value is what that call
    /// returns -- the environment chooses it.
    pub inputs: Vec<SymExpr>,
    /// How many of `inputs` are the boundary's own ports.
    pub declared_inputs: usize,
    /// How every call site is observed, on both sides.
    pub calls: CallModel,
    pub slots: Vec<Slot>,
    pub paths: Vec<PathSummary>,
    pub pruned: Vec<PrunedBranch>,
    /// Feasibility questions the solver could not decide; both sides were
    /// kept, which over-approximates the path set but never loses one.
    pub solver_unknowns: u32,
    /// The pointer regions the summary was computed over: the boundary's
    /// declared ones, then those it discovered.
    pub regions: Vec<PointerRegion>,
}

impl SliceSummary {
    /// Human-readable names for every variable the summary introduced.
    #[must_use]
    pub fn names(&self, boundary: &QuotientBoundary) -> BTreeMap<SymNodeId, String> {
        let mut names = BTreeMap::new();
        for (port, var) in boundary.inputs.iter().zip(&self.inputs) {
            names.insert(var_id(var), port.name.clone());
        }
        for (n, var) in self.inputs.iter().skip(self.declared_inputs).enumerate() {
            names.insert(var_id(var), format!("ret{n}"));
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

/// Summarize one step of `function` over `boundary`; `calls` says what is
/// known about each call it makes.
///
/// A pointer field the slice dereferences gets a region of its own (zeroed,
/// `budget.region_bytes` long, grown to cover the constant offsets read),
/// and the summary is computed again over it, until no new field turns up.
/// The regions used are in the result.
pub fn summarize(
    function: &PcodeFunction,
    abi: &AbiModel,
    boundary: &QuotientBoundary,
    budget: &QuotientBudget,
    calls: &CallFacts,
    image: Image,
) -> Result<SliceSummary, SummaryError> {
    let mut boundary = boundary.clone();
    // Each change adds a region (bounded by the budget) or grows one
    // (bounded by its size cap), so this ends; the cap is a backstop.
    for _ in 0..256 {
        match summarize_over(function, abi, &boundary, budget, calls, image) {
            Err(SummaryError::Regions(change)) => match change {
                RegionChange::Discover { parent, field } => {
                    boundary.regions.push(PointerRegion {
                        parent,
                        field,
                        initial: vec![0; budget.region_bytes as usize],
                        discovered: true,
                        address: None,
                        register: None,
                    });
                }
                RegionChange::Grow { region, bytes } => {
                    boundary.regions[region].initial.resize(bytes as usize, 0);
                }
                RegionChange::Global { start, end } => {
                    cover_global(&mut boundary, start, end, budget, image)?;
                }
                RegionChange::Register { name, bytes } => {
                    match boundary
                        .regions
                        .iter_mut()
                        .find(|region| region.register.as_deref() == Some(name.as_str()))
                    {
                        Some(region) => region.initial.resize(bytes as usize, 0),
                        None => boundary.regions.push(PointerRegion {
                            parent: None,
                            field: 0,
                            initial: vec![0; bytes as usize],
                            discovered: true,
                            address: None,
                            register: Some(name),
                        }),
                    }
                }
            },
            other => return other,
        }
    }
    Err(SummaryError::Budget(
        "the pointer regions did not settle".to_string(),
    ))
}

/// Cover global memory `[start, end)` with a region: grow the region within
/// `GLOBAL_GAP` of it, or add one. Its bytes are the image's.
fn cover_global(
    boundary: &mut QuotientBoundary,
    start: u64,
    end: u64,
    budget: &QuotientBudget,
    image: Image,
) -> Result<(), SummaryError> {
    // Merge with a nearby region only within one section of the image.
    let near = boundary.regions.iter().position(|region| {
        region.address.is_some_and(|at| {
            let region_end = at + region.initial.len() as u64;
            let (low, high) = (start.min(at), end.max(region_end));
            start <= region_end + GLOBAL_GAP
                && at <= end + GLOBAL_GAP
                && image(low, (high - low) as usize).is_some()
        })
    });
    let (start, end) = match near {
        Some(index) => {
            let region = &boundary.regions[index];
            let at = region.address.expect("a global region");
            (start.min(at), end.max(at + region.initial.len() as u64))
        }
        None => (start, end),
    };
    if end - start > u64::from(budget.max_region_bytes) {
        return Err(SummaryError::Budget(format!(
            "global memory [{start:#x}, {end:#x}) is wider than a region may be ({} bytes)",
            budget.max_region_bytes
        )));
    }
    let initial = image(start, (end - start) as usize).ok_or_else(|| {
        SummaryError::Budget(format!("global memory at {start:#x} is not in the image"))
    })?;
    let region = PointerRegion {
        parent: None,
        field: 0,
        initial,
        discovered: true,
        address: Some(start),
        register: None,
    };
    match near {
        Some(index) => boundary.regions[index] = region,
        None => boundary.regions.push(region),
    }
    Ok(())
}

fn summarize_over(
    function: &PcodeFunction,
    abi: &AbiModel,
    boundary: &QuotientBoundary,
    budget: &QuotientBudget,
    calls: &CallFacts,
    image: Image,
) -> Result<SliceSummary, SummaryError> {
    validate_boundary(abi, boundary)?;
    let mut inputs = boundary
        .inputs
        .iter()
        .map(|port| SymExpr::new_var(&port.name, port.bits))
        .collect::<Vec<_>>();
    let declared_inputs = inputs.len();
    // What each call returns, whole register wide: the environment may leave
    // anything in it, the upper half of a 32-bit result included.
    let returns = (0..MAX_CALLS_PER_STEP)
        .map(|n| SymExpr::new_var(&format!("ret{n}"), abi.return_value.size * 8))
        .collect::<Vec<_>>();
    let with_returns = inputs.iter().chain(&returns).cloned().collect::<Vec<_>>();

    // First pass: byte-granular memory, to learn which pieces the slice
    // reads and writes as units. Second pass: one variable per piece, so
    // summaries name `ctx[0x4:4]` rather than four bytes.
    let layout = boundary.layout();
    let size = layout.len as u32;
    // A region's pointer is where state lives, not state: its bytes are
    // never a slot.
    let pointer = abi.pointer_bits / 8;
    let bytes = (0..size)
        .filter(|offset| {
            !layout
                .fields
                .iter()
                .flatten()
                .any(|field| (*field..*field + pointer).contains(offset))
        })
        .map(|offset| slot_at(boundary, &layout, offset, 1, "byte_"))
        .collect::<Vec<_>>();
    let probe = Executor::new(
        function,
        abi,
        boundary,
        budget,
        &with_returns,
        bytes,
        calls,
        image,
    )
    .run(declared_inputs)?;
    let slots = merge_ranges(&probe.context_accesses)
        .into_iter()
        .map(|(offset, size)| slot_at(boundary, &layout, offset, size, ""))
        .collect::<Vec<_>>();
    let run = Executor::new(
        function,
        abi,
        boundary,
        budget,
        &with_returns,
        slots.clone(),
        calls,
        image,
    )
    .run(declared_inputs)?;
    let allowed = with_returns
        .iter()
        .map(var_id)
        .chain(slots.iter().map(Slot::id))
        .collect::<BTreeSet<_>>();
    let (paths, model) = observe_calls(abi, boundary, run.paths, &allowed)?;
    let used = paths.iter().map(|path| path.calls.len()).max().unwrap_or(0);
    inputs.extend(returns.into_iter().take(used));
    Ok(SliceSummary {
        inputs,
        declared_inputs,
        calls: model,
        slots,
        paths,
        pruned: run.pruned.into_iter().collect(),
        solver_unknowns: run.solver_unknowns,
        regions: boundary.regions.clone(),
    })
}

/// The slot `[offset, +size)` of the byte space, named after the region it
/// lies in.
fn slot_at(
    boundary: &QuotientBoundary,
    layout: &MemoryLayout,
    offset: u32,
    size: u32,
    prefix: &str,
) -> Slot {
    let region = layout
        .bases
        .iter()
        .zip(&boundary.regions)
        .rposition(|(base, region)| {
            *base <= offset && offset < *base + region.initial.len() as u32
        });
    let (label, var) = match region {
        None => (
            format!("ctx[{offset:#x}:{size}]"),
            format!("ctx_{prefix}{offset:x}"),
        ),
        Some(index) => {
            let within = offset - layout.bases[index];
            (
                format!("{}->[{within:#x}:{size}]", boundary.region_name(index)),
                format!("r{index}_{prefix}{within:x}"),
            )
        }
    };
    Slot {
        offset,
        size,
        var: SymExpr::new_var(&var, size * 8),
        region,
        label,
    }
}

/// A path as the executor leaves it: calls with their raw argument values,
/// before the call sites' observation is decided.
struct RawPath {
    guard: SymExpr,
    output: SymExpr,
    post: Vec<SymExpr>,
    blocks: Vec<u32>,
    calls: Vec<RawCall>,
    exit: PathExit,
}

#[derive(Debug, Clone)]
struct RawCall {
    site: u64,
    target: u64,
    tail: bool,
    fact: CallFact,
    /// Every argument register, in convention order.
    args: Vec<Val>,
}

/// The widest low part of `value` -- 64, 32, 16 or 8 bits -- that only the
/// boundary defines; 0 when not even its low byte is.
fn defined_bits(value: &SymExpr, allowed: &BTreeSet<SymNodeId>) -> u32 {
    let bits = width(value);
    [64, 32, 16, 8]
        .into_iter()
        .filter(|w| *w <= bits)
        .find(|w| free_vars(&extract(value.clone(), 0, *w)).is_subset(allowed))
        .unwrap_or(0)
}

/// Decide how each call site is observed, the same way on every path that
/// reaches it, and observe every call accordingly.
///
/// A site's observed width per argument register is the least, over the
/// paths, of the widest low part the boundary defines there. A library
/// callee observes exactly its declared parameters and each must be defined
/// at least as wide as its type; an unknown callee observes every argument
/// register the boundary defines. An address of the context or the frame
/// in an observed register -- for an unknown callee, in any argument
/// register -- stops the summary: the callee may write through it.
fn observe_calls(
    abi: &AbiModel,
    boundary: &QuotientBoundary,
    raw: Vec<RawPath>,
    allowed: &BTreeSet<SymNodeId>,
) -> Result<(Vec<PathSummary>, CallModel), SummaryError> {
    let registers = abi.arguments.len();
    struct Seen<'a> {
        call: &'a RawCall,
        bits: Vec<u32>,
        pointer: Vec<Option<Region>>,
    }
    let mut seen: BTreeMap<u64, Seen> = BTreeMap::new();
    for path in &raw {
        for call in &path.calls {
            let entry = seen.entry(call.site).or_insert_with(|| Seen {
                call,
                bits: vec![64; registers],
                pointer: vec![None; registers],
            });
            for (index, value) in call.args.iter().enumerate() {
                let bits = match value {
                    Val::Bits(value) => defined_bits(value, allowed),
                    Val::Ptr { region, .. } => {
                        entry.pointer[index].get_or_insert(*region);
                        0
                    }
                    Val::Poison(_) => 0,
                };
                entry.bits[index] = entry.bits[index].min(bits);
            }
        }
    }
    let mut model = CallModel::default();
    for (site, seen) in &seen {
        let call = seen.call;
        let callee = call
            .fact
            .callee
            .clone()
            .unwrap_or_else(|| format!("the callee at {:#x}", call.target));
        let unsupported = |reason: String| SummaryError::Unsupported {
            address: *site,
            reason,
        };
        let region = |region: Region| region.describe(boundary);
        let arity = call
            .fact
            .prototype
            .as_ref()
            .filter(|prototype| !prototype.variadic)
            .map(|prototype| prototype.params.len());
        let mut arg_bits = vec![0; registers];
        match arity {
            Some(count) => {
                if count > registers {
                    return Err(unsupported(format!(
                        "a call to {callee}, whose {count} parameters do not all fit the                          {registers} argument registers"
                    )));
                }
                let params = &call.fact.prototype.as_ref().expect("arity").params;
                for (index, param) in params.iter().enumerate() {
                    if let Some(at) = seen.pointer[index] {
                        return Err(unsupported(format!(
                            "a call passes an address of {} to {callee} (parameter {index}), \
                             which may write through it",
                            region(at)
                        )));
                    }
                    let bits = seen.bits[index];
                    if bits == 0 || param.bits.is_some_and(|declared| bits < declared) {
                        return Err(unsupported(format!(
                            "a call to {callee}: parameter {index} ({}) is not defined by the \
                             boundary",
                            param.type_name
                        )));
                    }
                    arg_bits[index] = bits;
                }
            }
            None => {
                if let Some((index, at)) = seen
                    .pointer
                    .iter()
                    .enumerate()
                    .find_map(|(index, at)| at.map(|at| (index, at)))
                {
                    return Err(unsupported(format!(
                        "a call passes an address of {} to {callee} (argument register {index}), \
                         an unknown callee that may write through it",
                        region(at)
                    )));
                }
                arg_bits.clone_from(&seen.bits);
            }
        }
        model.sites.insert(
            *site,
            CallSiteModel {
                site: *site,
                target: call.target,
                callee: call.fact.callee.clone(),
                class: call.fact.class(),
                tail: call.tail,
                arg_bits,
            },
        );
    }
    let paths = raw
        .into_iter()
        .map(|path| {
            let calls = path
                .calls
                .iter()
                .map(|call| {
                    let site = &model.sites[&call.site];
                    let args = site
                        .arg_bits
                        .iter()
                        .zip(&call.args)
                        .filter(|(bits, _)| **bits > 0)
                        .map(|(bits, value)| match value {
                            Val::Bits(value) => extract(value.clone(), 0, *bits),
                            _ => unreachable!("an observed argument is defined bits"),
                        })
                        .collect();
                    CallObservation {
                        site: call.site,
                        target: call.target,
                        args,
                    }
                })
                .collect();
            PathSummary {
                guard: path.guard,
                output: path.output,
                post: path.post,
                blocks: path.blocks,
                calls,
                exit: path.exit,
            }
        })
        .collect();
    Ok((paths, model))
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
    let pointer = abi.pointer_bits / 8;
    let mut fields: Vec<(Option<usize>, u32)> = Vec::new();
    for (index, region) in boundary.regions.iter().enumerate() {
        if !region.is_pointee() {
            if region.initial.is_empty() {
                return Err(unsupported(format!("region {index} has no bytes")));
            }
            continue;
        }
        let parent_len = match region.parent {
            None => boundary.context.initial.len(),
            Some(parent) if parent < index => boundary.regions[parent].initial.len(),
            Some(_) => {
                return Err(unsupported(format!(
                    "region {index} is listed before the region holding its pointer"
                )));
            }
        };
        if region.initial.is_empty() || region.field as usize + pointer as usize > parent_len {
            return Err(unsupported(format!(
                "region {index}: its pointer does not fit its parent, or it has no bytes"
            )));
        }
        if fields.iter().any(|(parent, field)| {
            *parent == region.parent && field.abs_diff(region.field) < pointer
        }) {
            return Err(unsupported(format!(
                "region {index}: its pointer overlaps another region's"
            )));
        }
        fields.push((region.parent, region.field));
    }
    Ok(())
}

/// The context's slots: each union of overlapping `(offset, size)`
/// accesses, in offset order -- or, where a union is wider than a value can
/// be (8 bytes; two overlapping 8-byte stores make 12), that union cut at
/// every access boundary inside it. Each piece lies inside one access, so
/// none is wider than 8 bytes.
fn merge_ranges(accesses: &BTreeSet<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for &(offset, size) in accesses {
        let end = offset + size;
        match merged.last_mut() {
            Some((start, len)) if offset < *start + *len => *len = (*len).max(end - *start),
            _ => merged.push((offset, size)),
        }
    }
    let mut slots = Vec::new();
    for (start, len) in merged {
        if len <= 8 {
            slots.push((start, len));
            continue;
        }
        let end = start + len;
        let cuts = accesses
            .iter()
            .flat_map(|&(offset, size)| [offset, offset + size])
            .filter(|cut| (start..=end).contains(cut))
            .collect::<BTreeSet<_>>();
        let cuts = cuts.into_iter().collect::<Vec<_>>();
        slots.extend(cuts.windows(2).map(|pair| (pair[0], pair[1] - pair[0])));
    }
    slots
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Region {
    Stack,
    Context,
    /// The region behind pointer field `n` of the boundary.
    Pointee(usize),
}

impl Region {
    fn describe(self, boundary: &QuotientBoundary) -> String {
        match self {
            Region::Stack => "the stack frame".to_string(),
            Region::Context => "the context".to_string(),
            Region::Pointee(index) => format!("the region {}", boundary.region_name(index)),
        }
    }
}

/// A value in the separated memory model.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Val {
    Bits(SymExpr),
    /// An address `region + offset (+ index)`, `bits` wide. Its absolute
    /// value is never known; only offsets within one region are compared.
    /// `index` is a part of the offset the slice computed from data
    /// (`buf[i]`); a stack address never has one.
    Ptr {
        region: Region,
        offset: i64,
        index: Option<SymExpr>,
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
    calls: Vec<RawCall>,
    /// A call has returned: a register byte nothing has written since holds
    /// whatever the callee left, not the caller's value.
    after_call: bool,
}

struct RunResult {
    paths: Vec<RawPath>,
    pruned: BTreeSet<PrunedBranch>,
    solver_unknowns: u32,
    context_accesses: BTreeSet<(u32, u32)>,
}

struct Executor<'a> {
    function: &'a PcodeFunction,
    abi: &'a AbiModel,
    boundary: &'a QuotientBoundary,
    budget: &'a QuotientBudget,
    /// The boundary's ports, then the call-return ports.
    inputs: &'a [SymExpr],
    declared_inputs: usize,
    calls: &'a CallFacts,
    slots: Vec<Slot>,
    /// Where each machine instruction's P-code starts: a branch may target
    /// an instruction inside a block (a conditional move skips to the next
    /// instruction), not only a block's first one.
    op_at: BTreeMap<u64, (usize, usize)>,
    /// Caller values of registers the slice reads without writing, one
    /// variable per pointer-sized word, shared by every path.
    live_ins: BTreeMap<u64, SymExpr>,
    return_address: SymExpr,
    allowed: BTreeSet<SymNodeId>,
    /// Where the context and each region sit in the byte space.
    layout: MemoryLayout,
    /// Each slot variable's `(offset, size)` in the byte space, to recognize
    /// a pointer read straight out of memory.
    var_ranges: BTreeMap<SymNodeId, (u32, u32)>,
    image: Image<'a>,
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
        calls: &'a CallFacts,
        image: Image<'a>,
    ) -> Self {
        let mut op_at = BTreeMap::new();
        for (index, block) in function.blocks.iter().enumerate() {
            op_at.entry(block.start_address).or_insert((index, 0));
            for (op_index, op) in block.ops.iter().enumerate() {
                op_at.entry(op.address).or_insert((index, op_index));
            }
        }
        let allowed = inputs
            .iter()
            .map(var_id)
            .chain(slots.iter().map(Slot::id))
            .collect();
        let var_ranges = slots
            .iter()
            .map(|slot| (slot.id(), (slot.offset, slot.size)))
            .collect();
        Self {
            function,
            abi,
            boundary,
            budget,
            inputs,
            declared_inputs: inputs.len(),
            calls,
            slots,
            op_at,
            live_ins: BTreeMap::new(),
            return_address: SymExpr::new_var("return_address", abi.pointer_bits),
            allowed,
            layout: boundary.layout(),
            var_ranges,
            image,
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
                index: None,
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
                index: None,
                bits: context.size * 8,
            },
            big,
        );
        // Each region's pointer, where its parent holds it.
        let mut memory = Cells::default();
        for (index, field) in self.layout.fields.iter().enumerate() {
            let Some(field) = field else { continue };
            memory.write(
                i64::from(*field),
                abi.pointer_bits / 8,
                Val::Ptr {
                    region: Region::Pointee(index),
                    offset: 0,
                    index: None,
                    bits: abi.pointer_bits,
                },
                big,
            );
        }
        for (port, var) in self
            .boundary
            .inputs
            .iter()
            .zip(&self.inputs[..self.declared_inputs])
        {
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
        // A register that anchors a region points to it.
        for (index, region) in self.boundary.regions.iter().enumerate() {
            let Some(name) = &region.register else {
                continue;
            };
            if let Some(slot) = abi.registers.iter().find(|slot| slot.name == *name) {
                registers.write(
                    slot.offset as i64,
                    slot.size,
                    Val::Ptr {
                        region: Region::Pointee(index),
                        offset: 0,
                        index: None,
                        bits: slot.size * 8,
                    },
                    big,
                );
            }
        }
        PathState {
            spaces: BTreeMap::from([(abi.register_space, registers)]),
            stack: Cells::default(),
            context: memory,
            conjuncts: Vec::new(),
            blocks: Vec::new(),
            ops: 0,
            calls: Vec::new(),
            after_call: false,
        }
    }

    fn run(mut self, declared_inputs: usize) -> Step<RunResult> {
        self.declared_inputs = declared_inputs;
        if self.function.blocks.is_empty() {
            return Err(SummaryError::Unsupported {
                address: self.boundary.entry,
                reason: "the slice has no P-code".to_string(),
            });
        }
        let entry = self
            .op_at
            .get(&self.boundary.entry)
            .map(|(block, _)| *block)
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
                PcodeOpcode::Branch if self.leaves_slice(op) => {
                    // A jump out of the slice: a call whose return is the
                    // step's return (a tail call).
                    if self.calls.error.is_some() || self.calls.tail_jumps.contains_key(&op.address)
                    {
                        self.call(&mut state, op, true)?;
                        return self.finish(state, op);
                    }
                    return self.unsupported(
                        op,
                        format!("a branch to {:#x}, outside the slice", op.inputs[0].offset),
                    );
                }
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
                PcodeOpcode::Call => {
                    if !self.call(&mut state, op, false)? {
                        return Ok(());
                    }
                    op_index += 1;
                }
                PcodeOpcode::CallInd => {
                    return self.unsupported(op, "an indirect call (its target is computed)");
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
                    if !self.step(&mut state, op)? {
                        return Ok(());
                    }
                    op_index += 1;
                }
            }
        }
    }

    /// Where execution of the instruction at `address` starts in the P-code.
    /// An instruction without P-code of its own (a `nop`) does nothing and
    /// falls through, so a branch to it lands on the next decoded
    /// instruction that has some.
    fn place_of(&self, address: u64) -> Option<(usize, usize)> {
        if let Some(place) = self.op_at.get(&address) {
            return Some(*place);
        }
        if !self.calls.instructions.contains(&address) {
            return None;
        }
        self.calls
            .instructions
            .range(address + 1..)
            .take(16)
            .find_map(|next| self.op_at.get(next).copied())
    }

    /// Whether `op` (a BRANCH) jumps to code outside the slice.
    fn leaves_slice(&self, op: &PcodeOp) -> bool {
        let target = &op.inputs[0];
        !(target.is_constant || target.space_id == 0)
            && target.space_id == self.abi.ram_space
            && self.place_of(target.offset).is_none()
    }

    /// Make the call `op`: record it with every argument register, then --
    /// unless the callee is proven not to return, which ends the path here
    /// -- go on as the callee would leave things. Returns whether the path
    /// continues. A `tail` call's return is left in the return register for
    /// the caller to finish the step with.
    fn call(&mut self, state: &mut PathState, op: &PcodeOp, tail: bool) -> Step<bool> {
        let facts = self.calls;
        if let Some(error) = &facts.error {
            return self.unsupported(
                op,
                format!("a call, but the function's call sites could not be listed: {error}"),
            );
        }
        let table = if tail {
            &facts.tail_jumps
        } else {
            &facts.sites
        };
        let Some(fact) = table.get(&op.address) else {
            return self.unsupported(op, "a call the machine-code decode does not list");
        };
        let target = &op.inputs[0];
        if target.is_constant || target.space_id != self.abi.ram_space {
            return self.unsupported(op, "a call to a computed target");
        }
        let n = state.calls.len();
        if self.declared_inputs + n >= self.inputs.len() {
            return Err(SummaryError::Budget(format!(
                "more than {MAX_CALLS_PER_STEP} calls in one step"
            )));
        }
        let abi = self.abi;
        let args = abi
            .arguments
            .iter()
            .map(|register| {
                self.read_register(
                    state,
                    abi.register_space,
                    register.offset as i64,
                    register.size,
                )
            })
            .collect();
        state.calls.push(RawCall {
            site: op.address,
            target: target.offset,
            tail,
            fact: fact.clone(),
            args,
        });
        if !fact.returns {
            self.finish_no_return(state, op)?;
            return Ok(false);
        }
        self.return_from_call(state, op, n, tail)?;
        Ok(true)
    }

    /// The registers as the `n`th call leaves them: the convention's
    /// preserved registers as they were, the return register holding call
    /// return port `n`, everything else poison. A call that pushed its return
    /// address has it popped again.
    fn return_from_call(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        n: usize,
        tail: bool,
    ) -> Step<()> {
        let abi = self.abi;
        if abi.preserved.is_empty() {
            return self.unsupported(
                op,
                "a call, but the calling convention names no caller-saved registers",
            );
        }
        let big = abi.big_endian;
        let mut kept = Cells::default();
        // The convention's preserved registers, and a register that anchors
        // a region when the convention does not list it as changed by a
        // call (the thread pointer).
        let anchors = self
            .boundary
            .regions
            .iter()
            .filter_map(|region| region.register.as_ref())
            .filter(|name| !abi.killed.contains(name))
            .filter_map(|name| abi.registers.iter().find(|slot| slot.name == *name));
        for register in abi.preserved.iter().chain(anchors) {
            let value = self.read_register(
                state,
                abi.register_space,
                register.offset as i64,
                register.size,
            );
            kept.write(register.offset as i64, register.size, value, big);
        }
        if !tail && matches!(abi.return_address, ReturnAddress::Stack) {
            let sp = &abi.stack_pointer;
            let popped = match kept.read(sp.offset as i64, sp.size, big, &mut |_| {
                Val::Poison("unset")
            }) {
                Val::Ptr {
                    region: Region::Stack,
                    offset,
                    index: None,
                    bits,
                } => Val::Ptr {
                    region: Region::Stack,
                    offset: offset + i64::from(abi.pointer_bits / 8),
                    index: None,
                    bits,
                },
                _ => {
                    return self
                        .unsupported(op, "a call made with the stack pointer off the frame");
                }
            };
            kept.write(sp.offset as i64, sp.size, popped, big);
        }
        let ret = &abi.return_value;
        let value = resize(self.inputs[self.declared_inputs + n].clone(), ret.size * 8);
        kept.write(ret.offset as i64, ret.size, Val::Bits(value), big);
        state.spaces.insert(abi.register_space, kept);
        state.after_call = true;
        Ok(())
    }

    /// End a path at a call that does not return.
    fn finish_no_return(&mut self, state: &mut PathState, op: &PcodeOp) -> Step<()> {
        for conjunct in &state.conjuncts {
            self.check_vars(op, conjunct, "a branch condition")?;
        }
        self.result.paths.push(RawPath {
            guard: and_all(std::mem::take(&mut state.conjuncts)),
            output: konst(0, self.boundary.output_bits),
            post: self.slots.iter().map(|slot| slot.var.clone()).collect(),
            blocks: std::mem::take(&mut state.blocks),
            calls: std::mem::take(&mut state.calls),
            exit: PathExit::NoReturn,
        });
        Ok(())
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
        match self.place_of(target.offset) {
            Some(place) => Ok(place),
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
        self.result.paths.push(RawPath {
            guard: and_all(state.conjuncts),
            output,
            post,
            blocks: state.blocks,
            calls: state.calls,
            exit: PathExit::Returned,
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
        let after_call = state.after_call;
        let cells = state.spaces.entry(space).or_default();
        cells.read(start, size, big, &mut |byte| {
            if space != register_space {
                return Val::Poison("a temporary read before it is written");
            }
            if after_call {
                return Val::Poison("a register a call may have overwritten");
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

    /// Where `region` starts in the byte space, and its length.
    fn span(&self, region: Region) -> (i64, i64) {
        match region {
            Region::Context => (0, self.boundary.context.initial.len() as i64),
            Region::Pointee(index) => (
                i64::from(self.layout.bases[index]),
                self.boundary.regions[index].initial.len() as i64,
            ),
            Region::Stack => unreachable!("the frame is not part of the byte space"),
        }
    }

    /// The region whose pointer overlaps `[start, +size)` of the byte space.
    fn field_in(&self, start: i64, size: u32) -> Option<usize> {
        let pointer = i64::from(self.abi.pointer_bits / 8);
        self.layout.fields.iter().position(|field| {
            field.is_some_and(|field| {
                let field = i64::from(field);
                start < field + pointer && field < start + i64::from(size)
            })
        })
    }

    /// Check that `[offset, +size)` lies inside `region`, and give its place
    /// in the byte space. A discovered region grows to cover a constant
    /// offset past its end, up to the budget.
    fn place_access(&self, op: &PcodeOp, region: Region, offset: i64, size: u32) -> Step<i64> {
        let (base, len) = self.span(region);
        let end = offset + i64::from(size);
        if offset >= 0 && end <= len {
            return Ok(base + offset);
        }
        // A global region covers more of the image instead.
        if let Region::Pointee(index) = region
            && let Some(at) = self.boundary.regions[index].address
            && self.boundary.regions[index].discovered
        {
            let start = at.wrapping_add(offset.min(0) as u64);
            let stop = at.wrapping_add(end.max(len) as u64);
            if stop > start
                && stop - start <= u64::from(self.budget.max_region_bytes)
                && (self.image)(start, (stop - start) as usize).is_some()
            {
                return Err(SummaryError::Regions(RegionChange::Global {
                    start: at.wrapping_add(offset as u64),
                    end: at.wrapping_add(end as u64),
                }));
            }
        }
        if let Region::Pointee(index) = region
            && self.boundary.regions[index].address.is_none()
            && offset >= 0
            && self.boundary.regions[index].discovered
            && end <= i64::from(self.budget.max_region_bytes)
        {
            let bytes = (end as u32).div_ceil(64) * 64;
            return Err(SummaryError::Regions(RegionChange::Grow {
                region: index,
                bytes: bytes.min(self.budget.max_region_bytes),
            }));
        }
        self.unsupported(
            op,
            format!(
                "an access [{offset:#x}, +{size}) outside the {len} bytes of {}",
                region.describe(self.boundary)
            ),
        )
    }

    /// Read memory, recording the access so the second pass makes it a
    /// slot. A region's pointer reads as that pointer and is never a slot:
    /// it is not state, it is where the state is.
    fn read_context(
        &mut self,
        state: &PathState,
        op: &PcodeOp,
        region: Region,
        offset: i64,
        size: u32,
    ) -> Step<Val> {
        let start = self.place_access(op, region, offset, size)?;
        if self.field_in(start, size).is_none() {
            self.result.context_accesses.insert((start as u32, size));
        }
        Ok(self.context_value(state, start, size))
    }

    /// Write memory at a constant place. A region's pointer is never
    /// overwritten (the region would move), and no address is ever stored:
    /// a stored pointer would be an alias the model cannot follow.
    fn write_context(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        region: Region,
        offset: i64,
        size: u32,
        value: Val,
    ) -> Step<()> {
        let start = self.place_access(op, region, offset, size)?;
        if let Some(index) = self.field_in(start, size) {
            return self.unsupported(
                op,
                format!(
                    "a store over the pointer to {}: the region would move",
                    Region::Pointee(index).describe(self.boundary)
                ),
            );
        }
        match &value {
            Val::Bits(_) => {}
            Val::Ptr { region: at, .. } => {
                return self.unsupported(
                    op,
                    format!(
                        "an address of {} is stored in {}",
                        at.describe(self.boundary),
                        region.describe(self.boundary)
                    ),
                );
            }
            Val::Poison(reason) => {
                return self.unsupported(
                    op,
                    format!("{} is given {reason}", region.describe(self.boundary)),
                );
            }
        }
        // Recording first, so a write-only piece still becomes a slot.
        self.result.context_accesses.insert((start as u32, size));
        state.context.write(start, size, value, self.abi.big_endian);
        Ok(())
    }

    /// An access through `region + offset + index`, `size` bytes: split the
    /// path where the index leaves the region (that side ends in
    /// [`PathExit::LeftRegion`]), and list the offsets it can take inside.
    /// `None` when no input and no state keeps it inside.
    fn indexed_places(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        region: Region,
        offset: i64,
        index: &SymExpr,
        size: u32,
    ) -> Step<Option<(SymExpr, Vec<i64>)>> {
        let bits = width(index);
        let at = bin(Bin::Add, konst(offset as u64, bits), index.clone());
        self.check_vars(op, &at, "an index")?;
        let (_, len) = self.span(region);
        // A discovered region grows to what the index can reach, when the
        // inputs and the path bound it within the budget: its footprint.
        if let Region::Pointee(number) = region
            && self.boundary.regions[number].discovered
        {
            let mut solver = Solver::new();
            solver.set_conflict_budget(self.budget.conflict_budget);
            for conjunct in &state.conjuncts {
                solver.assert(conjunct.clone());
            }
            if let Some(high) = solver.max(&at)
                && high < u64::from(self.budget.max_region_bytes)
            {
                let end = high as i64 + i64::from(size);
                if end > len && end <= i64::from(self.budget.max_region_bytes) {
                    return Err(SummaryError::Regions(
                        match self.boundary.regions[number].address {
                            // A global covers what the image holds there.
                            Some(start) if (self.image)(start, end as usize).is_some() => {
                                RegionChange::Global {
                                    start,
                                    end: start + end as u64,
                                }
                            }
                            Some(_) => {
                                return self.unsupported(
                                    op,
                                    "an indexed access past the end of the image's global memory",
                                );
                            }
                            None => RegionChange::Grow {
                                region: number,
                                bytes: ((end as u32).div_ceil(64) * 64)
                                    .min(self.budget.max_region_bytes),
                            },
                        },
                    ));
                }
            }
        }
        let last = len - i64::from(size);
        let inside = if last < 0 {
            konst(0, 1)
        } else {
            bin(Bin::Ule, at.clone(), konst(last as u64, bits))
        };
        let can_stay = self.feasible(&state.conjuncts, &inside);
        let can_leave = self.feasible(&state.conjuncts, &not(inside.clone()));
        if can_leave != SatResult::Unsat {
            let mut conjuncts = state.conjuncts.clone();
            conjuncts.push(not(inside.clone()));
            for conjunct in &conjuncts {
                self.check_vars(op, conjunct, "a branch condition")?;
            }
            self.result.paths.push(RawPath {
                guard: and_all(conjuncts),
                output: konst(0, self.boundary.output_bits),
                post: self.slots.iter().map(|slot| slot.var.clone()).collect(),
                blocks: state.blocks.clone(),
                calls: state.calls.clone(),
                exit: PathExit::LeftRegion { site: op.address },
            });
        }
        if can_stay == SatResult::Unsat {
            return Ok(None);
        }
        if can_leave != SatResult::Unsat {
            state.conjuncts.push(inside);
        }
        // The offsets the index can take there, as a range and a stride.
        let mut solver = Solver::new();
        solver.set_conflict_budget(self.budget.conflict_budget);
        for conjunct in &state.conjuncts {
            solver.assert(conjunct.clone());
        }
        let low = solver.min(&at).map_or(0, |value| value as i64);
        let high = solver.max(&at).map_or(last, |value| value as i64).min(last);
        let mut stride = 1;
        for candidate in [8u64, 4, 2] {
            if candidate > u64::from(size) {
                continue;
            }
            let misaligned = bin(
                Bin::Neq,
                bin(
                    Bin::And,
                    bin(Bin::Sub, at.clone(), konst(low as u64, bits)),
                    konst(candidate - 1, bits),
                ),
                konst(0, bits),
            );
            if self.feasible(&state.conjuncts, &misaligned) == SatResult::Unsat {
                stride = candidate as i64;
                break;
            }
        }
        let places = (low..=high).step_by(stride as usize).collect::<Vec<_>>();
        if places.len() > self.budget.max_indexed_positions as usize {
            return self.unsupported(
                op,
                format!(
                    "an indexed access into {} that can reach {} places (more than {})",
                    region.describe(self.boundary),
                    places.len(),
                    self.budget.max_indexed_positions
                ),
            );
        }
        Ok(Some((at, places)))
    }

    /// `buf[i]`: the value at every place the index can take, chosen by it.
    fn indexed_load(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        region: Region,
        offset: i64,
        index: &SymExpr,
        size: u32,
    ) -> Step<Option<Val>> {
        let Some((at, places)) = self.indexed_places(state, op, region, offset, index, size)?
        else {
            return Ok(None);
        };
        let bits = width(&at);
        let mut values = Vec::with_capacity(places.len());
        for place in &places {
            match self.read_context(state, op, region, *place, size)? {
                Val::Bits(value) => values.push(value),
                Val::Ptr { .. } => {
                    return self.unsupported(
                        op,
                        format!(
                            "an indexed read of a pointer in {}: which region it names is data",
                            region.describe(self.boundary)
                        ),
                    );
                }
                poison @ Val::Poison(_) => return Ok(Some(poison)),
            }
        }
        let last = values.pop().expect("at least one place");
        let value = places
            .iter()
            .zip(values)
            .rev()
            .fold(last, |otherwise, (place, value)| {
                ite(
                    bin(Bin::Eq, at.clone(), konst(*place as u64, bits)),
                    value,
                    otherwise,
                )
            });
        Ok(Some(Val::Bits(value)))
    }

    /// `buf[i] = v`: every place the index can take keeps its value unless
    /// the index chose it. Returns whether the path goes on.
    #[allow(clippy::too_many_arguments)]
    fn indexed_store(
        &mut self,
        state: &mut PathState,
        op: &PcodeOp,
        region: Region,
        offset: i64,
        index: &SymExpr,
        size: u32,
        value: Val,
    ) -> Step<bool> {
        let Some((at, places)) = self.indexed_places(state, op, region, offset, index, size)?
        else {
            return Ok(false);
        };
        let Val::Bits(value) = value else {
            // Refused the way a constant-offset store of it is.
            self.write_context(state, op, region, places[0], size, value)?;
            unreachable!("a pointer or poison store is refused");
        };
        let bits = width(&at);
        let chosen = |place: i64| bin(Bin::Eq, at.clone(), konst(place as u64, bits));
        let big = self.abi.big_endian;
        let stride = places
            .get(1)
            .map_or(i64::from(size), |next| next - places[0]);
        if stride >= i64::from(size) {
            // The places do not overlap: one piece each.
            for place in &places {
                let old = match self.read_context(state, op, region, *place, size)? {
                    Val::Bits(old) => old,
                    _ => return self.refuse_indexed_store(op, region),
                };
                let new = Val::Bits(ite(chosen(*place), value.clone(), old));
                self.write_context(state, op, region, *place, size, new)?;
            }
            return Ok(true);
        }
        // Overlapping places: byte by byte, each byte taking the stored
        // value's byte at its distance from the chosen place.
        let first = places[0];
        let end = places[places.len() - 1] + i64::from(size);
        for byte in first..end {
            let mut new = match self.read_context(state, op, region, byte, 1)? {
                Val::Bits(old) => old,
                _ => return self.refuse_indexed_store(op, region),
            };
            for place in places.iter().rev() {
                let distance = byte - place;
                if !(0..i64::from(size)).contains(&distance) {
                    continue;
                }
                let lsb = if big {
                    (i64::from(size) - 1 - distance) * 8
                } else {
                    distance * 8
                };
                new = ite(chosen(*place), extract(value.clone(), lsb as u32, 8), new);
            }
            self.write_context(state, op, region, byte, 1, Val::Bits(new))?;
        }
        Ok(true)
    }

    fn refuse_indexed_store<T>(&self, op: &PcodeOp, region: Region) -> Step<T> {
        self.unsupported(
            op,
            format!(
                "an indexed store into {} may overwrite a pointer",
                region.describe(self.boundary)
            ),
        )
    }

    /// A load or store through `address`, computed from data: when it is a
    /// pointer read straight out of memory plus an offset, the pointer's
    /// field needs a region of its own.
    fn discover(&self, op: &PcodeOp, address: &SymExpr, what: &str) -> Step<()> {
        let pointer = self.abi.pointer_bits / 8;
        let fields = addends(address)
            .into_iter()
            .filter_map(|term| self.memory_read(&term))
            .filter(|(_, size)| *size == pointer)
            .collect::<Vec<_>>();
        let [(start, _)] = fields.as_slice() else {
            return Ok(());
        };
        let start = *start;
        let regions = &self.boundary.regions;
        let parent = self
            .layout
            .bases
            .iter()
            .zip(regions)
            .position(|(base, region)| {
                *base <= start && start < *base + region.initial.len() as u32
            });
        let field = start - parent.map_or(0, |index| self.layout.bases[index]);
        let place = match parent {
            None => format!("ctx[{field:#x}]"),
            Some(index) => format!("{}->[{field:#x}]", self.boundary.region_name(index)),
        };
        let discovered = regions.iter().filter(|region| region.discovered).count();
        if discovered >= self.budget.max_regions {
            return self.unsupported(
                op,
                format!(
                    "{what} through the pointer at {place}: more pointer regions than the budget \
                     ({})",
                    self.budget.max_regions
                ),
            );
        }
        let depth = 1 + parent.map_or(0, |index| self.boundary.region_depth(index));
        if depth > self.budget.max_region_depth {
            return self.unsupported(
                op,
                format!(
                    "{what} through the pointer at {place}: {depth} pointers deep, past the \
                     budget ({})",
                    self.budget.max_region_depth
                ),
            );
        }
        Err(SummaryError::Regions(RegionChange::Discover {
            parent,
            field,
        }))
    }

    /// `address` as a place in global memory, when its constant part is an
    /// address the image maps: a pointer into the global region covering it
    /// (the rest of the address, if any, is an index). A global no region
    /// covers yet is asked for. `None` when the address is not global.
    fn global_pointer(&self, op: &PcodeOp, address: &SymExpr, size: u32) -> Step<Option<Val>> {
        let bits = width(address);
        let mut constant = 0u64;
        let mut rest = Vec::new();
        for term in addends(address) {
            match as_const(&term) {
                Some(value) => constant = constant.wrapping_add(value),
                None => rest.push(term),
            }
        }
        let constant = constant & mask(bits);
        if constant == 0 || (self.image)(constant, 1).is_none() {
            return Ok(None);
        }
        // A pointer read out of memory, or a register the caller set up,
        // plus a constant is not a global: the constant is an offset.
        let pointer = self.abi.pointer_bits / 8;
        if rest.iter().any(|term| {
            self.memory_read(term)
                .is_some_and(|(_, size)| size == pointer)
                || self.live_ins.values().any(|var| var == term)
        }) {
            return Ok(None);
        }
        let index = rest
            .into_iter()
            .map(|term| resize(term, bits))
            .reduce(|a, b| bin(Bin::Add, a, b));
        let reach = if index.is_some() { 1 } else { u64::from(size) };
        let covering = self.boundary.regions.iter().position(|region| {
            region.address.is_some_and(|at| {
                at <= constant && constant + reach <= at + region.initial.len() as u64
            })
        });
        if let Some(number) = covering {
            let at = self.boundary.regions[number].address.expect("global");
            return Ok(Some(Val::Ptr {
                region: Region::Pointee(number),
                offset: (constant - at) as i64,
                index,
                bits,
            }));
        }
        let globals = self
            .boundary
            .regions
            .iter()
            .filter(|region| region.address.is_some())
            .count();
        let near = self.boundary.regions.iter().any(|region| {
            region.address.is_some_and(|at| {
                constant <= at + region.initial.len() as u64 + GLOBAL_GAP
                    && at <= constant + reach + GLOBAL_GAP
            })
        });
        if !near && globals >= self.budget.max_global_regions {
            return self.unsupported(
                op,
                format!(
                    "global memory at {constant:#x}: more global regions than the budget ({})",
                    self.budget.max_global_regions
                ),
            );
        }
        Err(SummaryError::Regions(RegionChange::Global {
            start: constant,
            end: constant + reach,
        }))
    }

    /// An access at a constant offset from a register the caller set up:
    /// that register gets a block of its own, read as a region.
    fn anchor_register(&self, op: &PcodeOp, address: &SymExpr, size: u32) -> Step<()> {
        let Some((base, offset)) = self.environment_place(address) else {
            return Ok(());
        };
        let pointer = self.abi.pointer_bits / 8;
        let Some(register) = self.abi.register_at(base, pointer) else {
            return self.unsupported(op, ENVIRONMENT_READ);
        };
        let end = offset + i64::from(size);
        if offset < 0 || end > i64::from(self.budget.max_region_bytes) {
            return self.unsupported(op, ENVIRONMENT_READ);
        }
        Err(SummaryError::Regions(RegionChange::Register {
            name: register.name.clone(),
            bytes: ((end as u32).div_ceil(64) * 64).max(self.budget.region_bytes),
        }))
    }

    /// Whether `address` is a register the caller set up and the slice
    /// never wrote (a whole word) plus a constant: the thread pointer of
    /// thread-local storage, where the stack protector keeps its canary.
    /// Not a pointer field and not a region; refused with its own reason.
    fn environment_place(&self, address: &SymExpr) -> Option<(u64, i64)> {
        let mut base = None;
        let mut offset = 0i64;
        for term in addends(address) {
            if let Some(constant) = as_const(&term) {
                offset = offset.wrapping_add(signed_of(constant, width(&term)));
                continue;
            }
            let register = self
                .live_ins
                .iter()
                .find(|(_, var)| **var == term)
                .map(|(register, _)| *register)?;
            if base.replace(register).is_some() {
                return None;
            }
        }
        base.map(|base| (base, offset))
    }

    /// Whether `value` is memory read as it was when the step began --
    /// one slot variable, or adjacent pieces of them in byte order -- and
    /// if so, the `(offset, size)` it was read from.
    fn memory_read(&self, value: &SymExpr) -> Option<(u32, u32)> {
        let big = self.abi.big_endian;
        let mut pieces = Vec::new();
        flatten_concat(value, &mut pieces);
        // Least significant first.
        pieces.reverse();
        let mut bytes: Vec<u32> = Vec::new();
        for piece in pieces {
            let (var, lsb, size) = match piece {
                SymExpr::Var { id, .. } => (*id, 0, width(piece)),
                SymExpr::Extract { expr, lsb, size } => match expr.as_ref() {
                    SymExpr::Var { id, .. } => (*id, *lsb, *size),
                    _ => return None,
                },
                _ => return None,
            };
            let (offset, slot_size) = *self.var_ranges.get(&var)?;
            if lsb % 8 != 0 || size % 8 != 0 {
                return None;
            }
            let (first, count) = (lsb / 8, size / 8);
            for k in 0..count {
                let significance = first + k;
                let byte = if big {
                    offset + slot_size - 1 - significance
                } else {
                    offset + significance
                };
                bytes.push(byte);
            }
        }
        let step = |a: u32, b: u32| {
            if big {
                a.checked_sub(1) == Some(b)
            } else {
                a + 1 == b
            }
        };
        if bytes.is_empty() || !bytes.windows(2).all(|pair| step(pair[0], pair[1])) {
            return None;
        }
        let start = *bytes.iter().min().expect("non-empty");
        Some((start, bytes.len() as u32))
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
        // A register or temporary wider than a value (an x87 register) holds
        // poison: refused where something observes it.
        if varnode.size > 8 && !varnode.is_constant && varnode.space_id != self.abi.ram_space {
            return Ok(Val::Poison("a value wider than 8 bytes"));
        }
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
            let address = konst(varnode.offset, self.abi.pointer_bits);
            return match self.global_pointer(op, &address, varnode.size)? {
                Some(Val::Ptr { region, offset, .. }) => {
                    self.read_context(state, op, region, offset, varnode.size)
                }
                _ => self.unsupported(
                    op,
                    format!(
                        "absolute memory at {:#x} the image does not map",
                        varnode.offset
                    ),
                ),
            };
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
        if varnode.size > 8 && varnode.space_id != self.abi.ram_space {
            state.spaces.entry(varnode.space_id).or_default().write(
                varnode.offset as i64,
                varnode.size,
                Val::Poison("a value wider than 8 bytes"),
                self.abi.big_endian,
            );
            return Ok(());
        }
        if varnode.size == 0 || varnode.size > 8 {
            return self.unsupported(op, format!("a {}-byte value", varnode.size));
        }
        if varnode.space_id == self.abi.ram_space {
            let address = konst(varnode.offset, self.abi.pointer_bits);
            return match self.global_pointer(op, &address, varnode.size)? {
                Some(Val::Ptr { region, offset, .. }) => {
                    self.write_context(state, op, region, offset, varnode.size, val)
                }
                _ => self.unsupported(
                    op,
                    format!(
                        "absolute memory at {:#x} the image does not map",
                        varnode.offset
                    ),
                ),
            };
        }
        state.spaces.entry(varnode.space_id).or_default().write(
            varnode.offset as i64,
            varnode.size,
            val,
            self.abi.big_endian,
        );
        Ok(())
    }

    /// Execute `op`; whether the path goes on (an indexed access may find
    /// no place inside its region).
    fn step(&mut self, state: &mut PathState, op: &PcodeOp) -> Step<bool> {
        match op.opcode {
            PcodeOpcode::Load => {
                let output = op.output.as_ref().expect("validated: LOAD has an output");
                let address = match self.read(state, op, &op.inputs[1])? {
                    Val::Bits(address) => self
                        .global_pointer(op, &address, output.size)?
                        .unwrap_or(Val::Bits(address)),
                    other => other,
                };
                let value = match address {
                    Val::Ptr {
                        region: Region::Stack,
                        offset,
                        ..
                    } => self.read_stack(state, offset, output.size),
                    Val::Ptr {
                        region,
                        offset,
                        index: None,
                        ..
                    } => self.read_context(state, op, region, offset, output.size)?,
                    Val::Ptr {
                        region,
                        offset,
                        index: Some(index),
                        ..
                    } => match self.indexed_load(state, op, region, offset, &index, output.size)? {
                        Some(value) => value,
                        None => return Ok(false),
                    },
                    Val::Bits(address) => {
                        self.discover(op, &address, "a load")?;
                        self.anchor_register(op, &address, output.size)?;
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
                self.write(state, op, output, value)?;
                Ok(true)
            }
            PcodeOpcode::Store => {
                let value = self.read(state, op, &op.inputs[2])?;
                let size = op.inputs[2].size;
                let address = match self.read(state, op, &op.inputs[1])? {
                    Val::Bits(address) => self
                        .global_pointer(op, &address, size)?
                        .unwrap_or(Val::Bits(address)),
                    other => other,
                };
                match address {
                    Val::Ptr {
                        region: Region::Stack,
                        offset,
                        ..
                    } => {
                        state.stack.write(offset, size, value, self.abi.big_endian);
                        Ok(true)
                    }
                    Val::Ptr {
                        region,
                        offset,
                        index: None,
                        ..
                    } => {
                        self.write_context(state, op, region, offset, size, value)?;
                        Ok(true)
                    }
                    Val::Ptr {
                        region,
                        offset,
                        index: Some(index),
                        ..
                    } => self.indexed_store(state, op, region, offset, &index, size, value),
                    Val::Bits(address) if self.environment_place(&address).is_some() => {
                        self.anchor_register(op, &address, size)?;
                        self.unsupported(op, ENVIRONMENT_READ)
                    }
                    Val::Bits(address) => {
                        self.discover(op, &address, "a store")?;
                        self.unsupported(
                            op,
                            match as_const(&address) {
                                Some(at) => format!("a store to absolute address {at:#x}"),
                                None => "a store through an address computed from data".to_string(),
                            },
                        )
                    }
                    Val::Poison(reason) => {
                        self.unsupported(op, format!("a store through {reason}"))
                    }
                }
            }
            // `x ^ x` and `x - x` are zero whatever x holds -- a pointer, a
            // byte nothing defined. x86 clears a register this way.
            PcodeOpcode::IntXor | PcodeOpcode::IntSub
                if op.inputs.len() == 2 && op.inputs[0] == op.inputs[1] =>
            {
                let output = op.output.as_ref().expect("validated: has an output");
                let zero = Val::Bits(konst(0, output.size * 8));
                self.write(state, op, output, zero)?;
                Ok(true)
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
                if output.size > 8 && output.space_id != self.abi.ram_space {
                    self.write(state, op, output, Val::Poison("a value wider than 8 bytes"))?;
                    return Ok(true);
                }
                let value = self.apply(op, output.size * 8, inputs)?;
                self.write(state, op, output, value)?;
                Ok(true)
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
            let region_len = |region: Region| match region {
                Region::Stack => None,
                other => Some(self.span(other).1),
            };
            return Ok(pointer_op(op.opcode, out_bits, &inputs, &region_len));
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
            // A floating-point result is poison: refused where something
            // observes it, harmless where nothing does (the
            // `-fzero-call-used-regs` epilogue loads zeros into the x87
            // stack and pops them).
            O::FloatEqual
            | O::FloatNotEqual
            | O::FloatLess
            | O::FloatLessEqual
            | O::FloatNan
            | O::FloatAdd
            | O::FloatDiv
            | O::FloatMult
            | O::FloatSub
            | O::FloatNeg
            | O::FloatAbs
            | O::FloatSqrt
            | O::FloatInt2Float
            | O::FloatFloat2Float
            | O::FloatTrunc
            | O::FloatCeil
            | O::FloatFloor
            | O::FloatRound => Ok(Val::Poison("a floating-point value")),
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

/// Why a load or store at a constant offset from a register the caller set
/// up is refused. That memory is the environment's (thread-local storage;
/// on x86-64 the stack protector's `fs:[0x28]`), outside every boundary, and
/// the emulator's call harness gives the thread pointer nothing to point at.
const ENVIRONMENT_READ: &str = "an access at a constant offset from a register the caller set up \
     (thread-local storage, such as the stack protector's canary)";

/// The terms of a sum, flattened.
fn addends(value: &SymExpr) -> Vec<SymExpr> {
    match value {
        SymExpr::Add(a, b) => {
            let mut terms = addends(a);
            terms.extend(addends(b));
            terms
        }
        other => vec![other.clone()],
    }
}

/// The leaves of a concatenation, most significant first.
fn flatten_concat<'e>(value: &'e SymExpr, out: &mut Vec<&'e SymExpr>) {
    match value {
        SymExpr::Concat(hi, lo) => {
            flatten_concat(hi, out);
            flatten_concat(lo, out);
        }
        other => out.push(other),
    }
}

/// A pointer's offset within its region, as one term `bits` wide.
fn offset_term(offset: i64, index: Option<&SymExpr>, bits: u32) -> SymExpr {
    let constant = konst(offset as u64, bits);
    match index {
        Some(index) => bin(Bin::Add, constant, resize(index.clone(), bits)),
        None => constant,
    }
}

/// Operations with an address operand. Offsets within one region are known
/// or computed from data; anything that would need a region's absolute
/// address -- or would say whether two regions alias -- is poison.
/// `region_len` gives a pointer region's length.
fn pointer_op(
    opcode: PcodeOpcode,
    out_bits: u32,
    inputs: &[Val],
    region_len: &dyn Fn(Region) -> Option<i64>,
) -> Val {
    use PcodeOpcode as O;
    let constant = |v: &Val| match v {
        Val::Bits(value) => as_const(value).map(|c| signed_of(c, width(value))),
        _ => None,
    };
    // Add `delta` (data) to a pointer's offset.
    let shift = |ptr: &Val, delta: &SymExpr, negate: bool| -> Val {
        let Val::Ptr {
            region,
            offset,
            index,
            bits,
        } = ptr
        else {
            unreachable!("called on a pointer")
        };
        if let Some(c) = as_const(delta) {
            let c = signed_of(c, width(delta));
            return Val::Ptr {
                region: *region,
                offset: if negate {
                    offset.wrapping_sub(c)
                } else {
                    offset.wrapping_add(c)
                },
                index: index.clone(),
                bits: *bits,
            };
        }
        if *region == Region::Stack {
            return Val::Poison("an address offset by data");
        }
        let delta = resize(delta.clone(), *bits);
        let delta = if negate {
            bin(Bin::Sub, konst(0, *bits), delta)
        } else {
            delta
        };
        Val::Ptr {
            region: *region,
            offset: *offset,
            index: Some(match index {
                Some(index) => bin(Bin::Add, index.clone(), delta),
                None => delta,
            }),
            bits: *bits,
        }
    };
    let compare = |kind: Bin, a: &SymExpr, b: &SymExpr| {
        Val::Bits(widen_bool(bin(kind, a.clone(), b.clone()), out_bits))
    };
    match (opcode, inputs) {
        (O::Copy, [ptr @ Val::Ptr { bits, .. }]) if *bits == out_bits => ptr.clone(),
        (O::IntAdd, [ptr @ Val::Ptr { .. }, Val::Bits(delta)])
        | (O::IntAdd, [Val::Bits(delta), ptr @ Val::Ptr { .. }]) => shift(ptr, delta, false),
        (O::IntSub, [ptr @ Val::Ptr { .. }, Val::Bits(delta)]) => shift(ptr, delta, true),
        // `x & x` and `x | x` are x: how a register is tested against zero.
        (O::IntAnd | O::IntOr, [a @ Val::Ptr { .. }, b]) if a == b => a.clone(),
        (
            O::IntSub | O::IntEqual | O::IntNotEqual | O::IntLess | O::IntLessEqual,
            [
                Val::Ptr {
                    region: a,
                    offset: x,
                    index: xi,
                    ..
                },
                Val::Ptr {
                    region: b,
                    offset: y,
                    index: yi,
                    ..
                },
            ],
        ) => {
            if a != b {
                return Val::Poison(
                    "a comparison of addresses in two regions (whether they alias is assumed,                      not known)",
                );
            }
            // Offsets within one allocation: the region does not wrap the
            // address space, so they order the way the addresses do.
            let bits = out_bits.max(64);
            let (x, y) = (
                offset_term(*x, xi.as_ref(), bits),
                offset_term(*y, yi.as_ref(), bits),
            );
            match opcode {
                O::IntSub => Val::Bits(resize(bin(Bin::Sub, x, y), out_bits)),
                O::IntEqual => compare(Bin::Eq, &x, &y),
                O::IntNotEqual => compare(Bin::Neq, &x, &y),
                O::IntLess => compare(Bin::Ult, &x, &y),
                _ => compare(Bin::Ule, &x, &y),
            }
        }
        // A region's pointer is not null: the boundary gives every region an
        // allocation of its own, and an address inside one is never zero.
        (
            O::IntEqual | O::IntNotEqual,
            [
                Val::Ptr {
                    region: region @ Region::Pointee(_),
                    offset,
                    index: None,
                    ..
                },
                other,
            ],
        )
        | (
            O::IntEqual | O::IntNotEqual,
            [
                other,
                Val::Ptr {
                    region: region @ Region::Pointee(_),
                    offset,
                    index: None,
                    ..
                },
            ],
        ) if constant(other) == Some(0)
            && region_len(*region).is_some_and(|len| (0..=len).contains(offset)) =>
        {
            Val::Bits(konst(u64::from(opcode == O::IntNotEqual), out_bits))
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

    /// Two overlapping 8-byte stores (`[0, 8)` and `[4, 12)`) beside 4-byte
    /// loads: the 12-byte union is cut at the access boundaries.
    #[test]
    fn a_union_wider_than_a_value_is_cut_at_access_boundaries() {
        let accesses = BTreeSet::from([(0, 4), (4, 4), (8, 4), (12, 4), (0, 8), (4, 8)]);
        assert_eq!(merge_ranges(&accesses), [(0, 4), (4, 4), (8, 4), (12, 4)]);
        let narrow = BTreeSet::from([(0, 4), (0, 1), (4, 4)]);
        assert_eq!(merge_ranges(&narrow), [(0, 4), (4, 4)]);
    }

    #[test]
    fn a_pointer_survives_whole_and_poisons_in_part() {
        let mut cells = Cells::default();
        let pointer = Val::Ptr {
            region: Region::Context,
            offset: 4,
            index: None,
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
