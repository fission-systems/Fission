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
use fission_midend_core::ir::{HirFunction, NirType};
use fission_midend_prehir::PreHirFunction;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::call_sites::MachineFlow;
use crate::emu_driver::{EmulatorHarness, ObservedCall, StubbedEnd};
use crate::eval::{
    CallEvent, World, hir_free_names, hir_mentioned_names, normalize, run_hir, run_prehir, width_of,
};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};
use fission_decompiler::PrintedGlobal;

/// Size of the scratch region allocated for each pointer parameter.
const SCRATCH_REGION_SIZE: usize = 128;

/// Pointer parameters one call can be given a region for.
const SCRATCH_REGIONS: usize = 8;

/// Regions after the parameter ones that stubbed calls return pointers to.
const RESULT_REGIONS: usize = 8;

/// Divergent samples a function is checked until.
const MAX_REPORTED_DIVERGENCES: usize = 3;

/// Bytes the scratch buffer is filled with.
///
/// Zero, except the first four bytes of every sixteen. Zeros make any
/// pointer-sized field a null terminator, so a linked structure walked from
/// here ends rather than chasing garbage into unmapped memory and faulting the
/// call; the non-zero words give the arithmetic something to do so a body that
/// reads a field is not just returning zero either.
///
/// Allocates disjoint 128-byte regions for each pointer parameter so multiple
/// pointer arguments (e.g. memcpy, strcmp) do not artificially alias.
///
/// Only the parameter regions are seeded. The regions after them are what a
/// stubbed call returns a pointer to, and those are all zero: a fresh result
/// buffer whose first word read 65 sent bodies down paths, and chased
/// pointers, that a zeroed one does not.
fn scratch_contents(param_regions: usize, result_regions: usize) -> Vec<u8> {
    let param_regions = param_regions.max(1);
    let mut bytes = vec![0u8; (param_regions + result_regions) * SCRATCH_REGION_SIZE];
    for (chunk, slot) in bytes[..param_regions * SCRATCH_REGION_SIZE]
        .chunks_mut(16)
        .enumerate()
    {
        slot[..4].copy_from_slice(&((chunk as u32) + 1).to_le_bytes());
    }
    bytes
}

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
    pub fn for_function(binary: &LoadedBinary, address: u64, printed: &[PrintedGlobal]) -> Self {
        let (flow, flow_error) = match crate::call_sites::call_sites(binary, address) {
            Ok(flow) => (Some(flow), None),
            Err(err) => (None, Some(format!("{err:#}"))),
        };
        let mut grounding = Self {
            flow,
            flow_error,
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

/// For each `args` tuple, evaluate `prehir`/`hir` concretely and call the real
/// emulator at `address` with the same arguments, then compare all three.
/// Calls are refused; see [`check_ground_truth_with`] for the tier that
/// stubs them.
pub fn check_ground_truth(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
) -> VerifyOutcome {
    check_ground_truth_with(
        harness,
        address,
        prehir,
        hir,
        samples,
        &Grounding::default(),
    )
}

/// The value the `n`th stubbed call returns under `policy`.
///
/// Two policies, alternated across samples: every call returns zero (the
/// failure path of most library calls), or a pointer to a zeroed region of
/// its own (the success path, and a non-zero integer). Both sides use the
/// same function, so they always agree on it.
fn stub_return(scratch: u64, policy: usize, n: usize) -> u64 {
    if policy % 2 == 0 {
        0
    } else {
        let region = SCRATCH_REGIONS + n % RESULT_REGIONS;
        scratch.wrapping_add((region * SCRATCH_REGION_SIZE) as u64)
    }
}

/// For each `args` tuple, run `prehir` and `hir` and the real machine code
/// on the same arguments, the same memory and the same stubbed calls, and
/// compare what each returns, every call each makes and every byte each
/// stores outside the function's own frame.
///
/// A sample counts toward `checked` only when all three ran to a comparable
/// end; one the interpreter refuses is skipped and its reason kept, so a
/// function no sample could check reports why rather than passing.
pub fn check_ground_truth_with(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
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

    let num_ptrs = hir
        .params
        .iter()
        .filter(|p| matches!(p.ty, NirType::Ptr(_)))
        .count();
    if num_ptrs > SCRATCH_REGIONS {
        return VerifyOutcome::Unsupported(UnsupportedReason::Construct(
            "more pointer parameters than scratch regions",
        ));
    }
    // Always the same buffer: a harness serves every function of a batch and
    // installs its scratch once, so a size chosen per function would leave a
    // later function's regions outside the bytes the emulator holds. It also
    // holds the regions stubbed calls return pointers to.
    let contents = scratch_contents(SCRATCH_REGIONS, RESULT_REGIONS);
    let scratch = match harness.install_scratch(&contents) {
        Ok(addr) => addr,
        Err(err) => {
            return VerifyOutcome::Unsupported(UnsupportedReason::Harness(format!(
                "scratch buffer: {err}"
            )));
        }
    };
    let image: Vec<(u64, u8)> = contents
        .iter()
        .enumerate()
        .map(|(i, b)| (scratch.wrapping_add(i as u64), *b))
        .collect();
    let placed: HashMap<u64, u8> = image.iter().copied().collect();

    // Registers the body reads without ever writing -- caller state the
    // interpreter has no other way to know. Taken from the machine that is
    // about to run the same call, so both sides see the same thing.
    if let Some(name) = hir_mentioned_names(hir)
        .into_iter()
        .find(|name| grounding.ambiguous_globals.contains(name))
    {
        return VerifyOutcome::Unsupported(UnsupportedReason::Harness(format!(
            "global '{name}' names several objects; the text does not say which"
        )));
    }
    let mut free = hir_free_names(hir);
    free.retain(|name| !grounding.globals.contains_key(name));
    let bound = harness.entry_registers(&free);

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
    let mut divergences = Vec::new();
    let mut checked = 0usize;
    let mut emulator_errors = 0usize;
    let mut first_interpreter_error: Option<String> = None;
    let mut first_emulator_error: Option<String> = None;

    for (sample, args) in samples.iter().enumerate() {
        // Three differences are evidence enough; running the rest only
        // costs time on exactly the bodies that loop to their budget.
        if divergences.len() >= MAX_REPORTED_DIVERGENCES {
            break;
        }
        // A pointer parameter takes its disjoint buffer region address rather
        // than the sweep's integer, preventing false buffer aliasing across
        // multi-pointer arguments.
        let mut ptr_idx = 0usize;
        let args: Vec<i64> = args
            .iter()
            .zip(&hir.params)
            .map(|(v, p)| match &p.ty {
                NirType::Ptr(_) => {
                    let offset = (ptr_idx * SCRATCH_REGION_SIZE) as u64;
                    ptr_idx += 1;
                    scratch.wrapping_add(offset) as i64
                }
                _ => *v,
            })
            .collect();
        let call_return = |n: usize| stub_return(scratch, sample, n);
        let (prehir_r, hir_r) = {
            let backing = |addr: u64| harness.baseline_byte(addr);
            let world = World {
                image: &image,
                bound: &bound,
                backing: Some(&backing),
                globals: Some(&grounding.globals),
                global_types: Some(&grounding.global_types),
                call_return: grounding
                    .flow
                    .as_ref()
                    .map(|_| &call_return as &dyn Fn(usize) -> u64),
                no_return_sites: Some(&no_return_sites),
            };
            (
                run_prehir(&prehir.body, &prehir.params, &prehir.locals, &args, &world),
                run_hir(&hir.body, &hir.params, &hir.locals, &args, &world),
            )
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

        let u_args: Vec<u64> = args.iter().map(|&a| a as u64).collect();
        let machine = match harness.call_stubbed(address, &u_args, flow, &call_return) {
            Ok(run) => run,
            Err(err) => {
                tracing::debug!("emulator call failed for {args:?}: {err}");
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
                tracing::debug!("emulator call for {args:?} did not return normally: {other:?}");
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
        let sides = std::iter::once(("hir", hir_run)).chain(prehir_run.map(|run| ("prehir", run)));
        for (side, run) in sides {
            if detail.is_some() {
                break;
            }
            // A run that ends at a call which does not return still made
            // every store before it: those are compared, only the return
            // value is not.
            detail = compare_calls(side, &run.calls, &machine.calls)
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
                    compare_writes(side, &run.writes, &machine.writes, &placed, |a| {
                        harness.baseline_byte(a)
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
                args: args.to_vec(),
                prehir_result: prehir_val,
                hir_result: hir_val,
                emulator_result: emulator_val,
                detail,
            });
        }
    }

    if !divergences.is_empty() {
        return VerifyOutcome::Diverged(divergences);
    }
    if checked == 0 {
        if emulator_errors > 0 {
            return VerifyOutcome::Unsupported(UnsupportedReason::Harness(
                first_emulator_error.unwrap_or_default(),
            ));
        }
        return VerifyOutcome::Unsupported(UnsupportedReason::Interpreter(
            first_interpreter_error.unwrap_or_else(|| "no samples".to_string()),
        ));
    }
    VerifyOutcome::Equivalent { checked }
}

/// The first way `calls` (what one tree did) differs from `machine`'s, or
/// `None`. Both sides stop at a call that does not return, so the sequences
/// are compared whole.
fn compare_calls(side: &str, calls: &[CallEvent], machine: &[ObservedCall]) -> Option<String> {
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
                return Some(format!(
                    "{side}: call #{n} ({}) argument {i} is {:#x}, machine passed {:#x}",
                    call.target,
                    mask_to_width(*value as u64, bits),
                    mask_to_width(*raw, bits)
                ));
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
        assert_eq!(compare_calls("hir", &tree, &machine), None);
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
            compare_calls("hir", &tree, &[observed(0x10, &[0xffff_ffff])]),
            None
        );
        // A different low half is still a different argument, and a 64-bit
        // value the machine really passed in full is compared in full.
        assert!(compare_calls("hir", &tree, &[observed(0x10, &[0xffff_fffe])]).is_some());
        let wide = [CallEvent {
            arg_bits: vec![64],
            ..event(0x10, &[0x1_0000_0001])
        }];
        assert!(compare_calls("hir", &wide, &[observed(0x10, &[0x1])]).is_some());
    }

    #[test]
    fn a_call_the_tree_dropped_or_an_argument_it_changed_is_reported() {
        let machine = [observed(0x10, &[5]), observed(0x20, &[6])];
        let dropped = [event(0x10, &[5])];
        assert!(
            compare_calls("hir", &dropped, &machine)
                .unwrap()
                .contains("missing")
        );
        let changed = [event(0x10, &[5]), event(0x20, &[7])];
        assert!(
            compare_calls("hir", &changed, &machine)
                .unwrap()
                .contains("argument 0")
        );
        let extra = [event(0x10, &[5]), event(0x20, &[6]), event(0x30, &[1])];
        assert!(compare_calls("hir", &extra, &machine).is_some());
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

    /// The regions a stubbed call returns a pointer to start zeroed, as the
    /// stub policy says; only the parameter regions carry seeded words.
    #[test]
    fn result_regions_are_zero_and_parameter_regions_are_seeded() {
        let bytes = scratch_contents(2, 3);
        assert_eq!(bytes.len(), 5 * SCRATCH_REGION_SIZE);
        let (params, results) = bytes.split_at(2 * SCRATCH_REGION_SIZE);
        assert!(results.iter().all(|b| *b == 0));
        assert_eq!(&params[..4], &1u32.to_le_bytes());
    }
}
