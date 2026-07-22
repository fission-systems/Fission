//! Emulator-grounded ground truth: compares DIR's and HIR's concrete
//! evaluation ([`crate::eval`]) against what the real machine code, run
//! through the real [`fission_emulator::Emulator`] via
//! [`crate::emu_driver::EmulatorHarness`], actually returns for the same
//! concrete arguments.
//!
//! This is the tier [`crate::diff`] can never provide on its own: DIR and
//! HIR are both decompiler-derived, so a bug shared by both structuring
//! stages (the exact same wrong logic surviving the DIR->HIR conversion)
//! would make them agree with each other while both disagreeing with the
//! real machine. A three-way result -- `{emulator, dir_eval, hir_eval}` --
//! catches that class of bug.

use fission_midend_core::ir::HirFunction;
use fission_midend_dir::DirFunction;

use crate::emu_driver::{CallOutcome, EmulatorHarness};
use crate::eval::{interpret_dir, interpret_hir, normalize, width_of};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};

/// For each `args` tuple, evaluate `dir`/`hir` concretely and call the real
/// emulator at `address` with the same arguments, then compare all three.
/// A sample is only counted toward `checked`/reported as a divergence when
/// **both** interpreters produced a comparable `Ok(Some(_))` result --
/// exactly [`crate::diff::diff_dir_hir`]'s "both sides agree it's
/// evaluable" gate, so this tier never claims to ground a sample the
/// concrete tier itself couldn't evaluate.
pub fn check_ground_truth(
    harness: &mut EmulatorHarness,
    address: u64,
    dir: &DirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
) -> VerifyOutcome {
    let return_bits = width_of(&hir.return_type).clamp(1, 64);
    let mut divergences = Vec::new();
    let mut checked = 0usize;
    let mut emulator_errors = 0usize;

    for args in samples {
        let dir_r = interpret_dir(&dir.body, &dir.params, &dir.locals, args);
        let hir_r = interpret_hir(&hir.body, &hir.params, &hir.locals, args);
        let (Ok(Some(dir_val)), Ok(Some(hir_val))) = (&dir_r, &hir_r) else {
            // Not this tier's job to explain an unmodeled construct or a
            // void return -- `diff_dir_hir` already reports that. Skip.
            continue;
        };

        let u_args: Vec<u64> = args.iter().map(|&a| a as u64).collect();
        let call = match harness.call(address, &u_args) {
            Ok(c) => c,
            Err(err) => {
                tracing::debug!("emulator call failed for {args:?}: {err}");
                emulator_errors += 1;
                continue;
            }
        };
        let emulator_val = match call {
            CallOutcome::Returned(raw) => normalize(mask_to_width(raw, return_bits), &hir.return_type),
            other => {
                tracing::debug!("emulator call for {args:?} did not return normally: {other:?}");
                emulator_errors += 1;
                continue;
            }
        };

        checked += 1;
        if *dir_val != emulator_val || *hir_val != emulator_val {
            divergences.push(Divergence {
                args: args.clone(),
                dir_result: Some(*dir_val),
                hir_result: Some(*hir_val),
                emulator_result: Some(emulator_val),
            });
        }
    }

    if !divergences.is_empty() {
        return VerifyOutcome::Diverged(divergences);
    }
    if checked == 0 {
        let reason = if emulator_errors > 0 {
            "every sample either failed concrete DIR/HIR evaluation or the emulator call itself \
             (see debug logs) -- not evidence of correctness or a bug"
        } else {
            "no sample produced a comparable concrete DIR/HIR result -- see diff_dir_hir for the \
             concrete-tier reason"
        };
        return VerifyOutcome::Unsupported(UnsupportedReason::Construct(reason));
    }
    VerifyOutcome::Equivalent { checked }
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
