//! Emulator-grounded ground truth: compares PreHIR's and HIR's concrete
//! evaluation ([`crate::eval`]) against what the real machine code, run
//! through the real [`fission_emulator::Emulator`] via
//! [`crate::emu_driver::EmulatorHarness`], actually returns for the same
//! concrete arguments.
//!
//! This is the tier [`crate::diff`] can never provide on its own: PreHIR and
//! HIR are both decompiler-derived, so a bug shared by both structuring
//! stages (the exact same wrong logic surviving the PreHIR->HIR conversion)
//! would make them agree with each other while both disagreeing with the
//! real machine. A three-way result -- `{emulator, dir_eval, hir_eval}` --
//! catches that class of bug.

use fission_loader::loader::LoadedBinary;
use fission_midend_core::ir::{HirFunction, NirBindingOrigin, NirType};
use fission_midend_prehir::PreHirFunction;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::call_sites::MachineFlow;
use crate::emu_driver::{CallStub, EmulatorHarness, ObservedCall, StubbedEnd};
use crate::eval::{
    CallEvent, World, hir_addressed_locals, hir_free_names, hir_mentioned_names, normalize,
    prehir_addressed_locals, run_hir, run_prehir, width_of,
};
use crate::initial_state::{InitialState, RunPlan};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};
use fission_decompiler::PrintedGlobal;

/// Divergent runs a function is checked until.
const MAX_REPORTED_DIVERGENCES: usize = 3;

/// What the comparison needs from outside the two trees and the emulator.
#[derive(Debug, Clone, Default)]
pub struct Grounding {
    /// How the function's machine code calls out and where it runs
    /// ([`crate::call_sites`]). `None` refuses every call: the old tier.
    pub flow: Option<MachineFlow>,
    /// Why `flow` is `None`, when a decode was attempted.
    pub flow_error: Option<String>,
    /// The globals the printed code declares, with their addresses.
    pub globals: HashMap<String, u64>,
    /// Their declared types: the width of every load and store.
    pub global_types: HashMap<String, NirType>,
    /// Declared globals the printed code cannot be tied to one address --
    /// several loader symbols share the name.
    pub ambiguous_globals: BTreeSet<String>,
    /// Each stack slot's offset from the canonical frame address, by name,
    /// as the decompiler placed it. A body that takes the address of a local
    /// missing here is refused.
    pub frame_offsets: BTreeMap<String, i64>,
    /// The address ranges the loaded image covers.
    pub image: Vec<std::ops::Range<u64>>,
    /// Those of them a callee may write.
    pub writable: Vec<std::ops::Range<u64>>,
    /// Callees whose prototype the printed code declares rather than
    /// recovered: an argument that differs at a call to one is a difference,
    /// at any other a suspect.
    pub declared_callees: BTreeSet<String>,
}

impl Grounding {
    /// Everything [`check_ground_truth_with`] can use for the function at
    /// `address`: its machine control flow, and the globals its printed code
    /// declares (`printed`, from the decompile that produced the tree).
    ///
    /// The global set is the printer's, not a guess from names: a builder
    /// temporary can be spelled like an address (`tmp_1a004`), and a local
    /// can shadow a symbol, and only the printer's declarations say which
    /// names are file-scope storage in the text being checked.
    pub fn for_function(
        binary: &LoadedBinary,
        address: u64,
        pair: &crate::decompile::PreHirHirPair,
    ) -> Self {
        let printed: &[PrintedGlobal] = &pair.globals;
        let frame_offsets = pair.frame_offsets.clone();
        let (flow, flow_error) = match crate::call_sites::call_sites(binary, address) {
            Ok(flow) => (Some(flow), None),
            Err(err) => (None, Some(format!("{err:#}"))),
        };
        let sections = &binary.inner().sections;
        let mut grounding = Self {
            flow,
            flow_error,
            frame_offsets,
            declared_callees: pair.declared_callees.clone(),
            image: sections
                .iter()
                .filter(|s| s.virtual_size > 0)
                .map(|s| s.virtual_address..s.virtual_address.saturating_add(s.virtual_size))
                .collect(),
            writable: sections
                .iter()
                .filter(|s| s.virtual_size > 0 && s.is_writable)
                .map(|s| s.virtual_address..s.virtual_address.saturating_add(s.virtual_size))
                .collect(),
            ..Self::default()
        };
        for global in printed {
            grounding
                .global_types
                .insert(global.name.clone(), global.ty.clone());
            match global.address {
                Some(address) => {
                    grounding.globals.insert(global.name.clone(), address);
                }
                None => {
                    grounding.ambiguous_globals.insert(global.name.clone());
                }
            }
        }
        grounding
    }
}

/// Run `prehir`, `hir` and the real machine code from the same initial
/// states (`plan`) and compare what each returns. Calls are refused; see
/// [`check_ground_truth_with`] for the tier that stubs them.
pub fn check_ground_truth(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    plan: &RunPlan,
) -> VerifyOutcome {
    check_ground_truth_with(harness, address, prehir, hir, plan, &Grounding::default())
}

/// For each run of `plan`, run `prehir` and `hir` and the real machine code
/// from the same initial state ([`crate::initial_state`]) with the same
/// stubbed calls, and compare what each returns, every call each makes and
/// every byte each stores outside the function's own frame.
///
/// Every byte the function can read is defined by the run -- arguments,
/// registers, and memory anywhere a pointer leads -- and both sides read the
/// same one, so a pointer argument needs no buffer of its own and neither
/// side needs to know which arguments are pointers.
///
/// A run counts toward `checked` only when all three ran to a comparable
/// end; one the interpreter refuses is skipped and its reason kept, so a
/// function no run could check reports why rather than passing.
pub fn check_ground_truth_with(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    plan: &RunPlan,
    grounding: &Grounding,
) -> VerifyOutcome {
    // The calling convention model only covers integer registers. A floating-point
    // return or argument passes through vector/x87 registers which are not modeled
    // in the integer calling convention harness.
    if matches!(hir.return_type, NirType::Float { .. })
        || hir
            .params
            .iter()
            .any(|p| matches!(p.ty, NirType::Float { .. }))
        || matches!(prehir.return_type, NirType::Float { .. })
        || prehir
            .params
            .iter()
            .any(|p| matches!(p.ty, NirType::Float { .. }))
    {
        return VerifyOutcome::Unsupported(UnsupportedReason::Construct(
            "floating-point ABI in ground-truth",
        ));
    }

    if let Some(name) = hir_mentioned_names(hir)
        .into_iter()
        .find(|name| grounding.ambiguous_globals.contains(name))
    {
        return VerifyOutcome::Unsupported(UnsupportedReason::Harness(format!(
            "global '{name}' names several objects; the text does not say which"
        )));
    }
    // Registers the body reads without ever writing -- caller state the
    // interpreter has no other way to know. Read from the machine once each
    // run is set up, so both sides see the same values however the names
    // overlap (`eax` inside `rax`).
    let mut free = hir_free_names(hir);
    // And locals named after a register. The decompiler declares a variable
    // for a register it reads -- `long long fs_offset;` for the stack
    // canary's segment base, `rdx` for a register a path uses before it
    // assigns it -- and a read before any assignment is that register's
    // value at entry. The machine says what that is. A local the machine has
    // no register for (`local_8`, `tmp_1a004`) is not bound here, and a read
    // of it before an assignment is still refused.
    free.extend(
        hir.locals
            .iter()
            .filter(|local| local.initializer.is_none())
            .map(|local| local.name.clone()),
    );
    free.extend(
        prehir
            .locals
            .iter()
            .filter(|local| local.initializer.is_none())
            .map(|local| local.name.clone()),
    );
    free.sort();
    free.dedup();
    free.retain(|name| !grounding.globals.contains_key(name));
    let slot_bits = harness.pointer_bits();
    let slots = param_slots(hir.params.iter().map(|p| (p.name.as_str(), p.origin)));
    let prehir_slots = param_slots(prehir.params.iter().map(|p| (p.name.as_str(), p.origin)));
    let slot_count = slots
        .iter()
        .chain(&prehir_slots)
        .max()
        .map_or(0, |&s| s + 1);

    // Locals the body takes the address of live in the frame, where the
    // machine keeps them: `&x` handed to a call is the same pointer on both
    // sides, and `*(&x + 4)` reads the bytes next to `x`.
    let hir_local_offsets = match frame_offsets(
        &hir_addressed_locals(&hir.body, &hir.locals),
        &grounding.frame_offsets,
    ) {
        Ok(offsets) => offsets,
        Err(reason) => {
            return VerifyOutcome::Unsupported(UnsupportedReason::Interpreter(format!(
                "hir: {reason}"
            )));
        }
    };
    let prehir_local_offsets = frame_offsets(
        &prehir_addressed_locals(&prehir.body, &prehir.locals),
        &grounding.frame_offsets,
    );

    let empty_flow = MachineFlow::default();
    let flow = grounding.flow.as_ref().unwrap_or(&empty_flow);
    let no_return_sites: BTreeSet<u64> = flow
        .calls
        .iter()
        .filter(|(_, next)| next.is_none())
        .map(|(site, _)| *site)
        .collect();
    let return_bits = width_of(&hir.return_type).clamp(1, 64);
    let declares_value = !matches!(hir.return_type, NirType::Unknown)
        || !matches!(prehir.return_type, NirType::Unknown);
    let no_placed: HashMap<u64, u8> = HashMap::new();
    let mut divergences = Vec::new();
    // Runs whose only difference is an argument at a call with a recovered
    // prototype: reported only when no run differs outright.
    let mut suspects = Vec::new();
    let mut checked = 0usize;
    let mut emulator_errors = 0usize;
    let mut first_interpreter_error: Option<String> = None;
    let mut first_emulator_error: Option<String> = None;

    for run in 0..plan.runs {
        // Three differences are evidence enough; running the rest only
        // costs time on exactly the bodies that loop to their budget.
        if divergences.len() >= MAX_REPORTED_DIVERGENCES {
            break;
        }
        let state = InitialState::new(plan.seed, run as u32);
        // One word per slot up to the last one a parameter uses; a slot no
        // parameter names still holds whatever the caller left there.
        let raw_slots: Vec<u64> = (0..slot_count)
            .map(|slot| match slots.iter().position(|&s| s == slot) {
                Some(i) => state.argument_word(slot, &hir.params[i].ty, slot_bits),
                None => state.argument_word(slot, &NirType::Unknown, slot_bits),
            })
            .collect();
        let args: Vec<i64> = slots
            .iter()
            .zip(&hir.params)
            .map(|(&slot, p)| normalize(raw_slots[slot] as i64, &p.ty))
            .collect();
        let prehir_args: Vec<i64> = prehir_slots
            .iter()
            .zip(&prehir.params)
            .map(|(&slot, p)| normalize(raw_slots[slot] as i64, &p.ty))
            .collect();
        if let Err(err) = harness.begin_run(address, &raw_slots, &state) {
            first_emulator_error.get_or_insert_with(|| format!("call setup failed: {err}"));
            emulator_errors += 1;
            continue;
        }
        let bound = harness.registers(&free);
        let (cfa, frame) = match (harness.cfa(), harness.frame_range()) {
            (Ok(cfa), Ok(frame)) => (cfa, frame),
            (Err(err), _) | (_, Err(err)) => {
                first_emulator_error.get_or_insert_with(|| format!("call setup failed: {err}"));
                emulator_errors += 1;
                continue;
            }
        };
        let place = |offsets: &[(String, i64)]| -> HashMap<String, u64> {
            offsets
                .iter()
                .map(|(name, offset)| (name.clone(), cfa.wrapping_add_signed(*offset)))
                .collect()
        };
        let hir_locals = place(&hir_local_offsets);
        let prehir_locals = prehir_local_offsets.as_deref().map(place);
        let call_return = |n: usize| state.call_result(n);
        let call_clobber = |n: usize, index: usize| state.call_clobber(n, index);
        let tls = harness
            .registers(&["fs_offset".to_string()])
            .first()
            .map(|(_, base)| *base as u64)
            .filter(|base| *base != 0)
            .map(|base| base..base.saturating_add(0x1000));
        let objects: BTreeMap<u64, u64> = hir_locals
            .iter()
            .filter_map(|(name, addr)| {
                let ty = &hir.locals.iter().find(|l| &l.name == name)?.ty;
                Some((*addr, type_bytes(ty)?))
            })
            .chain(grounding.globals.iter().filter_map(|(name, addr)| {
                Some((*addr, type_bytes(grounding.global_types.get(name)?)?))
            }))
            .collect();
        let effects = CallEffects {
            state: &state,
            objects: &objects,
            frame: &frame,
            tls: tls.as_ref(),
            grounding,
        };
        let call_writes = |n: usize, args: &[u64]| effects.writes(n, args);
        let (prehir_r, hir_r) = {
            let backing = |addr: u64| Some(harness.initial_byte(&state, addr));
            let world = World {
                image: &[],
                bound: &bound,
                backing: Some(&backing),
                globals: Some(&grounding.globals),
                global_types: Some(&grounding.global_types),
                call_return: grounding
                    .flow
                    .as_ref()
                    .map(|_| &call_return as &dyn Fn(usize) -> u64),
                no_return_sites: Some(&no_return_sites),
                local_addresses: Some(&hir_locals),
                call_writes: Some(&call_writes),
            };
            let prehir_r = match &prehir_locals {
                Ok(locals) => run_prehir(
                    &prehir.body,
                    &prehir.params,
                    &prehir.locals,
                    &prehir_args,
                    &World {
                        local_addresses: Some(locals),
                        ..world
                    },
                ),
                Err(reason) => Err(anyhow::anyhow!("prehir: {reason}")),
            };
            (
                prehir_r,
                run_hir(&hir.body, &hir.params, &hir.locals, &args, &world),
            )
        };
        // Stores into the frame are the function's own business on both sides.
        let outside = |writes: &BTreeMap<u64, u8>| -> BTreeMap<u64, u8> {
            writes
                .iter()
                .filter(|(addr, _)| !frame.contains(addr))
                .map(|(addr, value)| (*addr, *value))
                .collect()
        };
        // The printed tree is what is being checked. PreHIR is compared when
        // it runs, but a PreHIR the interpreter cannot run (it can name storage
        // the printed tree resolved) does not stop the printed tree's check.
        let hir_run = match &hir_r {
            Ok(run) => run,
            Err(err) => {
                first_interpreter_error.get_or_insert_with(|| format!("hir: {err}"));
                continue;
            }
        };
        let prehir_run = prehir_r.as_ref().ok();

        let stub = CallStub {
            result: &call_return,
            clobber: Some(&call_clobber),
            writes: &call_writes,
        };
        let machine = match harness.run_prepared(flow, &stub) {
            Ok(run) => run,
            Err(err) => {
                tracing::debug!("emulator run {run} failed: {err}");
                first_emulator_error.get_or_insert_with(|| format!("call failed: {err}"));
                emulator_errors += 1;
                continue;
            }
        };
        let (machine_ret, ended_in_noreturn) = match &machine.end {
            StubbedEnd::Returned(raw) => (Some(*raw), false),
            StubbedEnd::NoReturnCall(_) => (None, true),
            StubbedEnd::LeftDecodedCode(pc) => {
                first_emulator_error.get_or_insert_with(|| {
                    format!("machine ran {pc:#x}, outside the decoded function")
                });
                emulator_errors += 1;
                continue;
            }
            StubbedEnd::Other(other) => {
                tracing::debug!("emulator run {run} did not return normally: {other:?}");
                first_emulator_error.get_or_insert_with(|| format!("{other:?}"));
                emulator_errors += 1;
                continue;
            }
        };

        checked += 1;
        let norm = |v: i64| normalize(mask_to_width(v as u64, return_bits), &hir.return_type);
        let emulator_val = machine_ret.map(|raw| norm(raw as i64));
        let prehir_val = prehir_run.and_then(|run| run.ret).map(norm);
        let hir_val = hir_run.ret.map(norm);
        let mut detail = None;
        let mut suspect: Option<String> = None;
        let sides = std::iter::once(("hir", hir_run)).chain(prehir_run.map(|run| ("prehir", run)));
        for (side, run) in sides {
            if detail.is_some() {
                break;
            }
            // A run that ends at a call which does not return still made
            // every store before it: those are compared, only the return
            // value is not.
            detail = compare_calls(
                side,
                &run.calls,
                &machine.calls,
                &grounding.declared_callees,
                &mut suspect,
            )
                .or_else(|| {
                    (run.ended_in_no_return != ended_in_noreturn).then(|| {
                        if ended_in_noreturn {
                            format!("{side}: the machine stopped at a call that does not return; the tree went on")
                        } else {
                            format!("{side}: the tree stopped at a call that does not return; the machine went on")
                        }
                    })
                })
                .or_else(|| {
                    compare_writes(side, &outside(&run.writes), &machine.writes, &no_placed, |a| {
                        Some(harness.initial_byte(&state, a))
                    })
                });
        }
        // `Unknown` is not `void`: the printer declares it as an integer and
        // the tree can return a value from it. Compare whenever a side
        // produced one.
        let returns_value = declares_value || hir_val.is_some() || prehir_val.is_some();
        let return_differs = !ended_in_noreturn
            && returns_value
            && (hir_val != emulator_val || prehir_run.is_some() && prehir_val != emulator_val);
        if return_differs || detail.is_some() {
            divergences.push(Divergence {
                run: Some(state.run()),
                args: args.to_vec(),
                prehir_result: prehir_val,
                hir_result: hir_val,
                emulator_result: emulator_val,
                detail,
            });
        } else if let Some(suspect) = suspect
            && suspects.len() < MAX_REPORTED_DIVERGENCES
        {
            suspects.push(Divergence {
                run: Some(state.run()),
                args: args.to_vec(),
                prehir_result: prehir_val,
                hir_result: hir_val,
                emulator_result: emulator_val,
                detail: Some(suspect),
            });
        }
    }

    if !divergences.is_empty() {
        return VerifyOutcome::Diverged(divergences);
    }
    if !suspects.is_empty() {
        return VerifyOutcome::Suspect(suspects);
    }
    if checked == 0 {
        if emulator_errors > 0 {
            return VerifyOutcome::Unsupported(UnsupportedReason::Harness(
                first_emulator_error.unwrap_or_default(),
            ));
        }
        return VerifyOutcome::Unsupported(UnsupportedReason::Interpreter(
            first_interpreter_error.unwrap_or_else(|| "no runs".to_string()),
        ));
    }
    VerifyOutcome::Equivalent { checked }
}

/// The first way `calls` (what one tree did) differs from `machine`'s, or
/// `None`. Both sides stop at a call that does not return, so the sequences
/// are compared whole.
///
/// An argument that differs at a call whose callee is not in `declared` is
/// not a difference by itself: the decompiler recovered that prototype, and
/// it may be the prototype that is wrong. The first such one goes to
/// `suspect`, and the comparison goes on.
fn compare_calls(
    side: &str,
    calls: &[CallEvent],
    machine: &[ObservedCall],
    declared: &BTreeSet<String>,
    suspect: &mut Option<String>,
) -> Option<String> {
    for (n, observed) in machine.iter().enumerate() {
        let Some(call) = calls.get(n) else {
            return Some(format!(
                "{side}: machine call #{n} at {:#x} is missing from the tree",
                observed.site
            ));
        };
        if call.site.is_some_and(|site| site != observed.site) {
            return Some(format!(
                "{side}: call #{n} is {} at {:#x}, machine called at {:#x}",
                call.target,
                call.site.unwrap_or_default(),
                observed.site
            ));
        }
        for (i, (value, bits)) in call.args.iter().zip(&call.arg_bits).enumerate() {
            let Some(raw) = observed.args.get(i) else {
                break;
            };
            let bits = (*bits).clamp(1, 64);
            if !argument_matches(*value, bits, *raw) {
                let difference = format!(
                    "{side}: call #{n} ({}) argument {i} is {:#x}, machine passed {:#x}",
                    call.target,
                    mask_to_width(*value as u64, bits),
                    mask_to_width(*raw, bits)
                );
                if declared.contains(&call.target) {
                    return Some(difference);
                }
                suspect.get_or_insert(difference);
            }
        }
    }
    if calls.len() > machine.len() {
        let extra = &calls[machine.len()];
        return Some(format!(
            "{side}: tree calls {} ({} calls) where the machine made {}",
            extra.target,
            calls.len(),
            machine.len()
        ));
    }
    None
}

/// Whether the tree's argument `value` (declared `bits` wide) is what the
/// machine passed in a register holding `raw`.
///
/// Compared at the tree's width, with one allowance: a 32-bit value the
/// machine wrote with a 32-bit move has its upper half zeroed, while the tree
/// may carry the same int sign- or zero-extended to 64 bits. A callee taking
/// an `int` sees the same argument either way, and without its prototype the
/// two cannot be told apart, so that pair is not called a difference.
fn argument_matches(value: i64, bits: u32, raw: u64) -> bool {
    if mask_to_width(value as u64, bits) == mask_to_width(raw, bits) {
        return true;
    }
    let low = value as u64 & 0xffff_ffff;
    bits > 32
        && raw >> 32 == 0
        && raw == low
        && (value as u64 == low || value == i64::from(low as u32 as i32))
}

/// The first byte stored differently, or `None`. A byte one side stored and
/// the other did not is compared against what the other holds there.
fn compare_writes(
    side: &str,
    tree: &BTreeMap<u64, u8>,
    machine: &BTreeMap<u64, u8>,
    placed: &HashMap<u64, u8>,
    baseline: impl Fn(u64) -> Option<u8>,
) -> Option<String> {
    let untouched = |addr: u64| placed.get(&addr).copied().or_else(|| baseline(addr));
    for (addr, value) in machine {
        let tree_value = tree.get(addr).copied().or_else(|| untouched(*addr));
        if tree_value != Some(*value) {
            return Some(format!(
                "{side}: byte at {addr:#x} is {} in the tree, machine stored {value:#04x}",
                tree_value.map_or("unmapped".to_string(), |v| format!("{v:#04x}"))
            ));
        }
    }
    for (addr, value) in tree {
        if machine.contains_key(addr) {
            continue;
        }
        let machine_value = untouched(*addr);
        if machine_value != Some(*value) {
            return Some(format!(
                "{side}: tree stored {value:#04x} at {addr:#x}, machine left {}",
                machine_value.map_or("unmapped".to_string(), |v| format!("{v:#04x}"))
            ));
        }
    }
    None
}

/// The longest run of bytes a stub writes from one argument.
const MAX_STUB_OBJECT: u64 = 256;

/// How far past an argument that points at no known object a stub writes.
const STUB_WINDOW: u64 = 16;

/// Bytes a value of `ty` occupies, when its declaration says.
fn type_bytes(ty: &NirType) -> Option<u64> {
    match ty {
        NirType::Aggregate { size, .. } => Some(u64::from(*size)),
        NirType::Unknown => None,
        other => Some(u64::from(width_of(other).div_ceil(8))),
    }
}

/// What a stand-in for a callee writes, the same on both sides of a check.
///
/// A callee may write the image's writable data and the memory it was given
/// pointers to. So each call rewrites every printed global in writable data,
/// the object an argument points at when the tree knows it (a local whose
/// address is passed, a global), and a small window behind an argument that
/// points elsewhere outside the image, the frame and thread-local storage.
/// Never the frame beyond a known object: the function's other locals live
/// there, and a callee writing them would be a broken program, not a model
/// of one. Never the thread block: the stack canary is there.
///
/// A value the decompiled code keeps from before a call that the machine
/// reloads after it then differs.
struct CallEffects<'a> {
    state: &'a InitialState,
    objects: &'a BTreeMap<u64, u64>,
    frame: &'a std::ops::Range<u64>,
    tls: Option<&'a std::ops::Range<u64>>,
    grounding: &'a Grounding,
}

impl CallEffects<'_> {
    fn off_limits(&self, addr: u64) -> bool {
        self.frame.contains(&addr)
            || self.tls.is_some_and(|tls| tls.contains(&addr))
            || self.grounding.image.iter().any(|r| r.contains(&addr))
    }

    fn writes(&self, n: usize, args: &[u64]) -> Vec<(u64, u8)> {
        let mut out = Vec::new();
        let mut write = |addr: u64| out.push((addr, self.state.call_memory_byte(n, addr)));
        for (name, addr) in &self.grounding.globals {
            let writable = self.grounding.writable.iter().any(|r| r.contains(addr));
            if let (true, Some(size)) = (
                writable,
                self.grounding.global_types.get(name).and_then(type_bytes),
            ) {
                (0..size.min(MAX_STUB_OBJECT)).for_each(|i| write(addr.wrapping_add(i)));
            }
        }
        for &arg in args {
            if let Some(&size) = self.objects.get(&arg) {
                let in_image = self.grounding.image.iter().any(|r| r.contains(&arg));
                let writable = self.grounding.writable.iter().any(|r| r.contains(&arg));
                if !in_image || writable {
                    (0..size.min(MAX_STUB_OBJECT)).for_each(|i| write(arg.wrapping_add(i)));
                }
                continue;
            }
            for i in 0..STUB_WINDOW {
                let addr = arg.wrapping_add(i);
                if self.off_limits(addr) {
                    break;
                }
                write(addr);
            }
        }
        out
    }
}

/// The offset from the canonical frame address of each local in
/// `addressed`, as the decompiler placed its stack slot; or why one of them
/// has no place in the frame.
fn frame_offsets(
    addressed: &[String],
    slots: &BTreeMap<String, i64>,
) -> Result<Vec<(String, i64)>, String> {
    addressed
        .iter()
        .map(|name| {
            slots
                .get(name)
                .map(|offset| (name.clone(), *offset))
                .ok_or_else(|| {
                    format!("the address of '{name}' is taken, and it has no placed stack slot")
                })
        })
        .collect()
}

/// Slots past this are not a parameter list the decompiler means; such a
/// tree is checked with its parameters in order.
const MAX_PARAM_SLOT: usize = 16;

/// The argument slot (0-based: registers, then stack words) each parameter
/// of `params` arrives in. A tree that drops a parameter its body never
/// reads keeps the rest where the ABI put them: `f(param_1, param_3)` reads
/// its second parameter from the third slot, not the second. The slot is
/// the binding's `ParamIndex` origin, else the 1-based slot the decompiler
/// names it after (`param_3`); a list where either is missing or repeats,
/// or reaches past [`MAX_PARAM_SLOT`], is taken in order.
fn param_slots<'a>(
    params: impl ExactSizeIterator<Item = (&'a str, Option<NirBindingOrigin>)>,
) -> Vec<usize> {
    let count = params.len();
    let slot_of = |(name, origin): (&str, Option<NirBindingOrigin>)| match origin {
        Some(NirBindingOrigin::ParamIndex(i)) => Some(i),
        _ => name
            .strip_prefix("param_")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1)),
    };
    let slots: Option<Vec<usize>> = params.map(slot_of).collect();
    match slots {
        Some(slots)
            if slots.windows(2).all(|w| w[0] < w[1])
                && slots.last().is_none_or(|&s| s < MAX_PARAM_SLOT) =>
        {
            slots
        }
        _ => (0..count).collect(),
    }
}

/// Truncate `raw` to its low `bits` bits (unsigned) before handing off to
/// [`normalize`] for sign-extension per the function's declared return
/// type -- the emulator's return register can carry garbage in the high
/// bits above the function's actual declared return width.
fn mask_to_width(raw: u64, bits: u32) -> i64 {
    if bits >= 64 {
        raw as i64
    } else {
        (raw & ((1u64 << bits) - 1)) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots_of(params: &[(&str, Option<NirBindingOrigin>)]) -> Vec<usize> {
        param_slots(params.iter().copied())
    }

    #[test]
    fn a_dropped_parameter_leaves_the_rest_in_their_slots() {
        assert_eq!(slots_of(&[("param_1", None), ("param_3", None)]), [0, 2]);
        assert_eq!(
            slots_of(&[
                ("fd", Some(NirBindingOrigin::ParamIndex(1))),
                ("param_4", None)
            ]),
            [1, 3]
        );
        // Names that say nothing, or slots out of order, keep the order.
        assert_eq!(slots_of(&[("argc", None), ("argv", None)]), [0, 1]);
        assert_eq!(slots_of(&[("param_3", None), ("param_1", None)]), [0, 1]);
        assert_eq!(slots_of(&[("param_40", None)]), [0]);
    }

    /// `compare_calls` with every callee's prototype declared: an argument
    /// difference is a difference.
    fn declared_compare(calls: &[CallEvent], machine: &[ObservedCall]) -> Option<String> {
        let declared = calls.iter().map(|c| c.target.clone()).collect();
        compare_calls("hir", calls, machine, &declared, &mut None)
    }

    /// At a call whose prototype was recovered, a differing argument is a
    /// suspect and the rest of the sequence is still compared: a call that is
    /// missing afterwards is a difference outright.
    #[test]
    fn an_argument_at_a_recovered_prototype_is_a_suspect() {
        let tree = [event(0x10, &[1]), event(0x20, &[2])];
        let mut suspect = None;
        let none = BTreeSet::new();
        assert_eq!(
            compare_calls(
                "hir",
                &tree,
                &[observed(0x10, &[9]), observed(0x20, &[2])],
                &none,
                &mut suspect
            ),
            None
        );
        assert!(suspect.is_some_and(|s| s.contains("argument 0")));
        let mut suspect = None;
        assert!(
            compare_calls(
                "hir",
                &tree[..1],
                &[observed(0x10, &[9]), observed(0x20, &[2])],
                &none,
                &mut suspect
            )
            .is_some_and(|d| d.contains("missing from the tree"))
        );
    }

    fn event(site: u64, args: &[i64]) -> CallEvent {
        CallEvent {
            site: Some(site),
            target: format!("sub_{site:x}"),
            args: args.to_vec(),
            arg_bits: vec![32; args.len()],
        }
    }

    fn observed(site: u64, args: &[u64]) -> ObservedCall {
        ObservedCall {
            site,
            args: args.to_vec(),
        }
    }

    #[test]
    fn matching_calls_agree_and_register_garbage_above_the_width_is_ignored() {
        let tree = [event(0x10, &[5, -1])];
        let machine = [observed(0x10, &[0xdead_0000_0005, 0xffff_ffff, 0x77])];
        assert_eq!(declared_compare(&tree, &machine), None);
    }

    #[test]
    fn a_32_bit_argument_extended_to_64_by_the_tree_is_the_same_argument() {
        // `read(-1, ..)`: the tree's int argument sign-extended, the machine's
        // `mov edi, -1` zero-extended.
        let tree = [CallEvent {
            arg_bits: vec![64],
            ..event(0x10, &[-1])
        }];
        assert_eq!(
            declared_compare(&tree, &[observed(0x10, &[0xffff_ffff])]),
            None
        );
        // A different low half is still a different argument, and a 64-bit
        // value the machine really passed in full is compared in full.
        assert!(declared_compare(&tree, &[observed(0x10, &[0xffff_fffe])]).is_some());
        let wide = [CallEvent {
            arg_bits: vec![64],
            ..event(0x10, &[0x1_0000_0001])
        }];
        assert!(declared_compare(&wide, &[observed(0x10, &[0x1])]).is_some());
    }

    #[test]
    fn a_call_the_tree_dropped_or_an_argument_it_changed_is_reported() {
        let machine = [observed(0x10, &[5]), observed(0x20, &[6])];
        let dropped = [event(0x10, &[5])];
        assert!(
            declared_compare(&dropped, &machine)
                .unwrap()
                .contains("missing")
        );
        let changed = [event(0x10, &[5]), event(0x20, &[7])];
        assert!(
            declared_compare(&changed, &machine)
                .unwrap()
                .contains("argument 0")
        );
        let extra = [event(0x10, &[5]), event(0x20, &[6]), event(0x30, &[1])];
        assert!(declared_compare(&extra, &machine).is_some());
    }

    #[test]
    fn a_store_the_tree_dropped_or_invented_is_reported() {
        let baseline = |addr: u64| (addr < 0x2000).then_some(0u8);
        let machine: BTreeMap<u64, u8> = [(0x1000, 7)].into_iter().collect();
        let same: BTreeMap<u64, u8> = [(0x1000, 7)].into_iter().collect();
        assert_eq!(
            compare_writes("hir", &same, &machine, &HashMap::new(), baseline),
            None
        );
        let dropped = BTreeMap::new();
        assert!(compare_writes("hir", &dropped, &machine, &HashMap::new(), baseline).is_some());
        let invented: BTreeMap<u64, u8> = [(0x1000, 7), (0x1004, 1)].into_iter().collect();
        assert!(compare_writes("hir", &invented, &machine, &HashMap::new(), baseline).is_some());
        // Storing the value already there is no difference.
        let redundant: BTreeMap<u64, u8> = [(0x1000, 7), (0x1004, 0)].into_iter().collect();
        assert_eq!(
            compare_writes("hir", &redundant, &machine, &HashMap::new(), baseline),
            None
        );
    }
}
