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
use fission_midend_core::ir::{HirExpr, HirFunction, HirLValue, HirStmt, NirType};
use fission_midend_prehir::PreHirFunction;
use std::collections::{BTreeMap, HashMap};

use crate::emu_driver::{EmulatorHarness, ObservedCall, StubbedEnd};
use crate::eval::{CallEvent, World, hir_free_names, normalize, run_hir, run_prehir, width_of};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};

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
fn scratch_contents(num_regions: usize) -> Vec<u8> {
    let num_regions = num_regions.max(1);
    let mut bytes = vec![0u8; num_regions * SCRATCH_REGION_SIZE];
    for (chunk, slot) in bytes.chunks_mut(16).enumerate() {
        slot[..4].copy_from_slice(&((chunk as u32) + 1).to_le_bytes());
    }
    bytes
}

/// What the comparison needs from outside the two trees and the emulator.
#[derive(Debug, Clone, Default)]
pub struct Grounding {
    /// Call instructions in the function and where each continues
    /// ([`crate::call_sites`]). `None` refuses every call: the old tier.
    pub call_sites: Option<BTreeMap<u64, Option<u64>>>,
    /// Globals the bodies name, with their addresses.
    pub globals: HashMap<String, u64>,
}

impl Grounding {
    /// Everything [`check_ground_truth_with`] can use for the function at
    /// `address`: its call sites, and an address for every global the two
    /// trees name -- a symbol the loader knows, or an address-named one
    /// (`tmp_1a004`, `DAT_1a004`).
    pub fn for_function(
        binary: &LoadedBinary,
        address: u64,
        prehir: &PreHirFunction,
        hir: &HirFunction,
    ) -> Self {
        let call_sites = crate::call_sites::call_sites(binary, address).ok();
        let by_name: HashMap<&str, u64> = binary
            .global_symbols
            .iter()
            .map(|(addr, name)| (name.as_str(), *addr))
            .collect();
        let mut globals = HashMap::new();
        let names = crate::eval::hir_free_names(hir)
            .into_iter()
            .chain(crate::eval::prehir_free_names(prehir))
            .chain(hir.locals.iter().map(|l| l.name.clone()))
            .chain(prehir.locals.iter().map(|l| l.name.clone()))
            .chain(global_references(hir));
        for name in names {
            if globals.contains_key(&name)
                || hir.params.iter().any(|p| p.name == name)
                || prehir.params.iter().any(|p| p.name == name)
            {
                continue;
            }
            let address =
                address_named_global(&name).or_else(|| by_name.get(name.as_str()).copied());
            if let Some(address) = address {
                globals.insert(name, address);
            }
        }
        Self {
            call_sites,
            globals,
        }
    }
}

/// `tmp_1a004` / `DAT_1a004`: a global the binary has no symbol for, printed
/// under its address.
fn address_named_global(name: &str) -> Option<u64> {
    let hex = name
        .strip_prefix("tmp_")
        .or_else(|| name.strip_prefix("DAT_"))?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(hex, 16).ok()
}

/// Names the body takes the address of as globals (`&name`).
fn global_references(hir: &HirFunction) -> Vec<String> {
    fn expr(e: &HirExpr, out: &mut Vec<String>) {
        match e {
            HirExpr::AddressOfGlobal(name) => out.push(name.clone()),
            HirExpr::Cast { expr: inner, .. }
            | HirExpr::Unary { expr: inner, .. }
            | HirExpr::Load { ptr: inner, .. }
            | HirExpr::PtrOffset { base: inner, .. }
            | HirExpr::FieldAccess { base: inner, .. }
            | HirExpr::AggregateCopy { src: inner, .. } => expr(inner, out),
            HirExpr::Binary { lhs, rhs, .. } => {
                expr(lhs, out);
                expr(rhs, out);
            }
            HirExpr::Index { base, index, .. } => {
                expr(base, out);
                expr(index, out);
            }
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                expr(cond, out);
                expr(then_expr, out);
                expr(else_expr, out);
            }
            HirExpr::Call { args, .. } => args.iter().for_each(|a| expr(a, out)),
            _ => {}
        }
    }
    fn lvalue(l: &HirLValue, out: &mut Vec<String>) {
        match l {
            HirLValue::Var(_) => {}
            HirLValue::Deref { ptr, .. } => expr(ptr, out),
            HirLValue::Index { base, index, .. } => {
                expr(base, out);
                expr(index, out);
            }
            HirLValue::FieldAccess { base, .. } => expr(base, out),
        }
    }
    fn stmts(body: &[HirStmt], out: &mut Vec<String>) {
        for s in body {
            match s {
                HirStmt::Assign { lhs, rhs } => {
                    lvalue(lhs, out);
                    expr(rhs, out);
                }
                HirStmt::Expr(e) | HirStmt::Return(Some(e)) => expr(e, out),
                HirStmt::Block(b) => stmts(b, out),
                HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    expr(cond, out);
                    stmts(then_body, out);
                    stmts(else_body, out);
                }
                HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                    expr(cond, out);
                    stmts(body, out);
                }
                HirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
                    if let Some(init) = init {
                        stmts(std::slice::from_ref(init.as_ref()), out);
                    }
                    if let Some(cond) = cond {
                        expr(cond, out);
                    }
                    if let Some(update) = update {
                        stmts(std::slice::from_ref(update.as_ref()), out);
                    }
                    stmts(body, out);
                }
                HirStmt::Switch {
                    expr: e,
                    cases,
                    default,
                } => {
                    expr(e, out);
                    for case in cases {
                        stmts(&case.body, out);
                    }
                    stmts(default, out);
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    stmts(&hir.body, &mut out);
    out
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
    let contents = scratch_contents(SCRATCH_REGIONS + RESULT_REGIONS);
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
    let mut free = hir_free_names(hir);
    free.retain(|name| !grounding.globals.contains_key(name));
    let bound = harness.entry_registers(&free);

    let empty_sites = BTreeMap::new();
    let sites = grounding.call_sites.as_ref().unwrap_or(&empty_sites);
    let return_bits = width_of(&hir.return_type).clamp(1, 64);
    let returns_value = !matches!(hir.return_type, NirType::Unknown)
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
                call_return: grounding
                    .call_sites
                    .as_ref()
                    .map(|_| &call_return as &dyn Fn(usize) -> u64),
            };
            (
                run_prehir(&prehir.body, &prehir.params, &prehir.locals, &args, &world),
                run_hir(&hir.body, &hir.params, &hir.locals, &args, &world),
            )
        };
        let (Ok(prehir_run), Ok(hir_run)) = (&prehir_r, &hir_r) else {
            if first_interpreter_error.is_none() {
                first_interpreter_error = Some(match (&hir_r, &prehir_r) {
                    (Err(err), _) => format!("hir: {err}"),
                    (_, Err(err)) => format!("prehir: {err}"),
                    _ => unreachable!(),
                });
            }
            continue;
        };

        let u_args: Vec<u64> = args.iter().map(|&a| a as u64).collect();
        let machine = match harness.call_stubbed(address, &u_args, sites, &call_return) {
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
        let prehir_val = prehir_run.ret.map(norm);
        let hir_val = hir_run.ret.map(norm);
        let mut detail = None;
        for (side, run) in [("hir", hir_run), ("prehir", prehir_run)] {
            if detail.is_some() {
                break;
            }
            detail =
                compare_calls(side, &run.calls, &machine.calls, ended_in_noreturn).or_else(|| {
                    if ended_in_noreturn {
                        None
                    } else {
                        compare_writes(side, &run.writes, &machine.writes, &placed, |a| {
                            harness.baseline_byte(a)
                        })
                    }
                });
        }
        let return_differs = !ended_in_noreturn
            && returns_value
            && (prehir_val != emulator_val || hir_val != emulator_val);
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
/// `None`. When the machine stopped at a call that does not return, the tree
/// is only held to the calls up to it.
fn compare_calls(
    side: &str,
    calls: &[CallEvent],
    machine: &[ObservedCall],
    machine_stopped: bool,
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
                return Some(format!(
                    "{side}: call #{n} ({}) argument {i} is {:#x}, machine passed {:#x}",
                    call.target,
                    mask_to_width(*value as u64, bits),
                    mask_to_width(*raw, bits)
                ));
            }
        }
    }
    if !machine_stopped && calls.len() > machine.len() {
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
        assert_eq!(compare_calls("hir", &tree, &machine, false), None);
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
            compare_calls("hir", &tree, &[observed(0x10, &[0xffff_ffff])], false),
            None
        );
        // A different low half is still a different argument, and a 64-bit
        // value the machine really passed in full is compared in full.
        assert!(compare_calls("hir", &tree, &[observed(0x10, &[0xffff_fffe])], false).is_some());
        let wide = [CallEvent {
            arg_bits: vec![64],
            ..event(0x10, &[0x1_0000_0001])
        }];
        assert!(compare_calls("hir", &wide, &[observed(0x10, &[0x1])], false).is_some());
    }

    #[test]
    fn a_call_the_tree_dropped_or_an_argument_it_changed_is_reported() {
        let machine = [observed(0x10, &[5]), observed(0x20, &[6])];
        let dropped = [event(0x10, &[5])];
        assert!(
            compare_calls("hir", &dropped, &machine, false)
                .unwrap()
                .contains("missing")
        );
        let changed = [event(0x10, &[5]), event(0x20, &[7])];
        assert!(
            compare_calls("hir", &changed, &machine, false)
                .unwrap()
                .contains("argument 0")
        );
        let extra = [event(0x10, &[5]), event(0x20, &[6]), event(0x30, &[1])];
        assert!(compare_calls("hir", &extra, &machine, false).is_some());
        // After a call that does not return, the tree is held only to the
        // calls up to it.
        assert_eq!(compare_calls("hir", &extra, &machine, true), None);
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

    #[test]
    fn address_named_globals_parse_and_ordinary_names_do_not() {
        assert_eq!(address_named_global("tmp_1a004"), Some(0x1a004));
        assert_eq!(address_named_global("DAT_dd048"), Some(0xdd048));
        assert_eq!(address_named_global("tmp_"), None);
        assert_eq!(address_named_global("tmp_count"), None);
        assert_eq!(address_named_global("local_8"), None);
    }
}
