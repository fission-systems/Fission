//! PreHIR-vs-HIR differential verification (concrete tier, no ground truth).
//!
//! Runs the same concrete argument tuples through [`crate::eval::interpret_prehir`]/
//! [`crate::eval::interpret_hir`] against a function's pre-structuring form
//! (PreHIR) and post-structuring form (HIR) and reports any input where the two
//! disagree -- a real structuring bug, with a concrete repro input, not a
//! hand-diffed hunch.
//!
//! This tier alone does **not** rule out a bug shared by both PreHIR and HIR
//! (both interpreters walking the same wrong logic in lockstep would agree
//! with each other and disagree with reality) -- see [`crate::ground_truth`]
//! for the tier that checks against the real emulator instead.

use fission_midend_core::ir::HirFunction;
use fission_midend_prehir::PreHirFunction;

use crate::eval::{interpret_hir, interpret_prehir};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};

/// Differentially interpret `prehir` and `hir` -- the real, independently-typed
/// [`PreHirFunction`]/[`HirFunction`] `fission-pcode`'s production pipeline
/// produces for the same source function -- over each tuple in `samples`,
/// in order, and report the outcome. Taking the distinct types here (rather
/// than two same-shaped slices) means an accidentally swapped argument
/// order is a compile error, not a silent runtime bug.
///
/// A concrete input that makes *both* sides return `Err` (e.g. a shared
/// division-by-zero) is treated as inconclusive for that sample, not a
/// divergence -- both interpreters hit the same unmodeled/undefined
/// behavior along the same expression, which isn't evidence structuring
/// changed anything. A sample where exactly one side errors *is* reported:
/// that asymmetry is itself suspicious and worth surfacing.
pub fn diff_prehir_hir(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
) -> VerifyOutcome {
    let mut divergences = Vec::new();
    let mut checked = 0usize;

    for args in samples {
        let prehir_r = interpret_prehir(&prehir.body, &prehir.params, &prehir.locals, args);
        let hir_r = interpret_hir(&hir.body, &hir.params, &hir.locals, args);

        match (prehir_r, hir_r) {
            (Ok(prehir_result), Ok(hir_result)) => {
                checked += 1;
                if prehir_result != hir_result {
                    divergences.push(Divergence {
                        args: args.clone(),
                        prehir_result,
                        hir_result,
                        emulator_result: None,
                    });
                }
            }
            (Err(_), Err(_)) => {
                // Both sides hit the same unmodeled construct/UB on this
                // input -- inconclusive, not a pass or a fail.
            }
            (Ok(prehir_result), Err(hir_err)) => {
                tracing::debug!("HIR-only eval error for {args:?}: {hir_err}");
                divergences.push(Divergence {
                    args: args.clone(),
                    prehir_result,
                    hir_result: None,
                    emulator_result: None,
                });
            }
            (Err(prehir_err), Ok(hir_result)) => {
                tracing::debug!("PreHIR-only eval error for {args:?}: {prehir_err}");
                divergences.push(Divergence {
                    args: args.clone(),
                    prehir_result: None,
                    hir_result,
                    emulator_result: None,
                });
            }
        }
    }

    if !divergences.is_empty() {
        return VerifyOutcome::Diverged(divergences);
    }
    if checked == 0 {
        return VerifyOutcome::Unsupported(UnsupportedReason::Construct(
            "no sample produced a comparable (Ok, Ok) result -- function likely uses an \
             unmodeled construct (Load/Store/Call/etc, see eval's module doc)",
        ));
    }
    VerifyOutcome::Equivalent { checked }
}

/// A small, deliberately boundary-heavy set of concrete argument tuples for
/// an `arity`-parameter function: zero, one, all-bits-set, `i32::MIN`, and a
/// few small values in every combination. Not exhaustive -- the symbolic
/// tier ([`crate::lower_sym`]) is what closes that gap; this is meant to
/// catch the same class of bug hand-auditing already found once this
/// session (specific edge-case inputs), cheaply.
pub fn default_samples(arity: usize) -> Vec<Vec<i64>> {
    let seeds: [i64; 7] = [0, 1, -1, 2, -2, i32::MIN as i64, i32::MAX as i64];
    if arity == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    let mut idx = vec![0usize; arity];
    loop {
        out.push(idx.iter().map(|&i| seeds[i]).collect());
        let mut pos = arity;
        loop {
            if pos == 0 {
                return out;
            }
            pos -= 1;
            idx[pos] += 1;
            if idx[pos] < seeds.len() {
                break;
            }
            idx[pos] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::ir::{
        HirBinaryOp, HirExpr, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType,
    };
    use fission_midend_prehir::{
        PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirLValue, PreHirStmt,
    };

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    fn bool_ty() -> NirType {
        NirType::Bool
    }

    fn prehir_param(name: &str, idx: usize) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: i32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(idx)),
            initializer: None,
        }
    }

    fn hir_param(name: &str, idx: usize) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: i32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(idx)),
            initializer: None,
        }
    }

    fn prehir_var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn hvar(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    fn prehir_a_gt_b() -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::SGt,
            lhs: Box::new(prehir_var("a")),
            rhs: Box::new(prehir_var("b")),
            ty: bool_ty(),
        }
    }

    fn hir_a_gt_b() -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::SGt,
            lhs: Box::new(hvar("a")),
            rhs: Box::new(hvar("b")),
            ty: bool_ty(),
        }
    }

    fn prehir_func(body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            params: vec![prehir_param("a", 0), prehir_param("b", 1)],
            body,
            ..Default::default()
        }
    }

    /// HIR (structured): `if (a > b) { return a; } else { return b; }`.
    fn max_hir() -> HirFunction {
        HirFunction {
            params: vec![hir_param("a", 0), hir_param("b", 1)],
            body: vec![HirStmt::If {
                cond: hir_a_gt_b(),
                then_body: vec![HirStmt::Return(Some(hvar("a")))],
                else_body: vec![HirStmt::Return(Some(hvar("b")))],
            }],
            ..Default::default()
        }
    }

    /// PreHIR (flattened goto/label form of the *same* logic): a conditional
    /// jump (`If` with a single `Goto` in `then_body`, no `else_body` --
    /// this is how a p-code CBranch naturally maps onto `PreHirStmt` without a
    /// dedicated "conditional goto" variant) to a label guarding the
    /// then-arm, with the else-arm falling straight through.
    fn max_dir() -> PreHirFunction {
        prehir_func(vec![
            PreHirStmt::If {
                cond: prehir_a_gt_b(),
                then_body: vec![PreHirStmt::Goto("L_then".to_string())].into(),
                else_body: vec![].into(),
            },
            PreHirStmt::Return(Some(prehir_var("b"))),
            PreHirStmt::Label("L_then".to_string()),
            PreHirStmt::Return(Some(prehir_var("a"))),
        ])
    }

    #[test]
    fn equivalent_dir_and_hir_report_equivalent() {
        let outcome = diff_prehir_hir(&max_dir(), &max_hir(), &default_samples(2));
        match outcome {
            VerifyOutcome::Equivalent { checked } => {
                assert_eq!(checked, default_samples(2).len());
            }
            other => panic!("expected Equivalent, got {other:?}"),
        }
    }

    /// A deliberately buggy PreHIR that always returns `a` (as if a
    /// structuring-style bug had collapsed the conditional away) --
    /// proves the harness actually catches a real divergence instead of
    /// trivially reporting everything as equivalent.
    #[test]
    fn diverging_dir_and_hir_are_caught_with_a_concrete_repro() {
        let buggy_prehir = prehir_func(vec![PreHirStmt::Return(Some(prehir_var("a")))]);
        let outcome = diff_prehir_hir(&buggy_prehir, &max_hir(), &default_samples(2));
        match outcome {
            VerifyOutcome::Diverged(divs) => {
                assert!(!divs.is_empty());
                // Every sample with a < b should have diverged (buggy PreHIR
                // returns `a`, correct HIR returns `b`).
                let found = divs.iter().any(|d| d.args == vec![-2, -1]);
                assert!(found, "expected a divergence at args=[-2,-1], got {divs:?}");
            }
            other => panic!("expected Diverged, got {other:?}"),
        }
    }

    #[test]
    fn assign_and_while_loop_interpreted_correctly() {
        // sum(n) = 0 + 1 + ... + n, via a while loop -- exercises Assign,
        // While, and the loop-carried accumulator/counter pattern.
        let params = vec![hir_param("n", 0)];
        let locals = vec![
            NirBinding {
                name: "acc".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Const(0, i32_ty())),
            },
            NirBinding {
                name: "i".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: Some(HirExpr::Const(0, i32_ty())),
            },
        ];
        let body = vec![
            HirStmt::While {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLe,
                    lhs: Box::new(hvar("i")),
                    rhs: Box::new(hvar("n")),
                    ty: bool_ty(),
                },
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("acc".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(hvar("acc")),
                            rhs: Box::new(hvar("i")),
                            ty: i32_ty(),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("i".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(hvar("i")),
                            rhs: Box::new(HirExpr::Const(1, i32_ty())),
                            ty: i32_ty(),
                        },
                    },
                ],
            },
            HirStmt::Return(Some(hvar("acc"))),
        ];
        let result = interpret_hir(&body, &params, &locals, &[5]).expect("interpret");
        assert_eq!(result, Some(15)); // 0+1+2+3+4+5
    }

    #[test]
    fn dir_assign_and_while_loop_interpreted_correctly() {
        // Same as `assign_and_while_loop_interpreted_correctly` but on the
        // PreHIR side -- proves the macro-generated `prehir::interpret` isn't
        // accidentally different from `hir::interpret`.
        let params = vec![prehir_param("n", 0)];
        let locals = vec![
            PreHirBinding {
                name: "acc".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: Some(PreHirExpr::Const(0, i32_ty())),
            },
            PreHirBinding {
                name: "i".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: Some(PreHirExpr::Const(0, i32_ty())),
            },
        ];
        let body = vec![
            PreHirStmt::While {
                cond: PreHirExpr::Binary {
                    op: PreHirBinaryOp::SLe,
                    lhs: Box::new(prehir_var("i")),
                    rhs: Box::new(prehir_var("n")),
                    ty: bool_ty(),
                },
                body: vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("acc".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(prehir_var("acc")),
                            rhs: Box::new(prehir_var("i")),
                            ty: i32_ty(),
                        },
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("i".to_string()),
                        rhs: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Add,
                            lhs: Box::new(prehir_var("i")),
                            rhs: Box::new(PreHirExpr::Const(1, i32_ty())),
                            ty: i32_ty(),
                        },
                    },
                ]
                .into(),
            },
            PreHirStmt::Return(Some(prehir_var("acc"))),
        ];
        let result = interpret_prehir(&body, &params, &locals, &[5]).expect("interpret");
        assert_eq!(result, Some(15)); // 0+1+2+3+4+5
    }
}
