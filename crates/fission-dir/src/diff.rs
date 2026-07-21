//! DIR-vs-HIR differential verification harness.
//!
//! Runs the same concrete argument tuples through [`crate::interp::interpret`]
//! against a function's pre-structuring body (DIR) and post-structuring body
//! (HIR) and reports any input where the two disagree -- a real structuring
//! bug, with a concrete repro input, not a hand-diffed hunch.

use fission_midend_core::ir::{Dir, Hir, NirBinding};

use crate::interp::interpret;

/// One concrete input where DIR and HIR produced different results.
#[derive(Debug, Clone)]
pub struct Divergence {
    pub args: Vec<i64>,
    pub dir_result: Option<i64>,
    pub hir_result: Option<i64>,
}

/// Outcome of attempting to verify one function.
#[derive(Debug)]
pub enum VerifyOutcome {
    /// All `samples` produced identical DIR/HIR results.
    Equivalent { checked: usize },
    /// At least one input diverged -- a real, reproducible structuring bug.
    Diverged(Vec<Divergence>),
    /// Neither confirmed nor refuted -- the function (or one of the two
    /// snapshots) uses a construct `interp` doesn't model yet (Load/Store/
    /// Call/etc, see `interp`'s module doc). Not a pass, not a fail.
    Unsupported { reason: String },
}

/// Differentially interpret `dir` and `hir` (same `params`/`locals`, per
/// [`interpret`]'s documented assumption) over each tuple in `samples`, in
/// order, and report the outcome. Taking the distinct [`Dir`]/[`Hir`]
/// newtypes here (rather than two same-shaped `&[HirStmt]` slices) means an
/// accidentally swapped argument order is a compile error, not a silent
/// runtime bug.
///
/// A concrete input that makes *both* sides return `Err` (e.g. a shared
/// division-by-zero) is treated as inconclusive for that sample, not a
/// divergence -- both interpreters hit the same unmodeled/undefined
/// behavior along the same expression, which isn't evidence structuring
/// changed anything. A sample where exactly one side errors *is* reported:
/// that asymmetry is itself suspicious and worth surfacing.
pub fn diff_dir_hir(
    dir: &Dir,
    hir: &Hir,
    params: &[NirBinding],
    locals: &[NirBinding],
    samples: &[Vec<i64>],
) -> VerifyOutcome {
    let mut divergences = Vec::new();
    let mut checked = 0usize;

    for args in samples {
        let dir_r = interpret(&dir.0, params, locals, args);
        let hir_r = interpret(&hir.0, params, locals, args);

        match (dir_r, hir_r) {
            (Ok(dir_result), Ok(hir_result)) => {
                checked += 1;
                if dir_result != hir_result {
                    divergences.push(Divergence {
                        args: args.clone(),
                        dir_result,
                        hir_result,
                    });
                }
            }
            (Err(_), Err(_)) => {
                // Both sides hit the same unmodeled construct/UB on this
                // input -- inconclusive, not a pass or a fail.
            }
            (Ok(dir_result), Err(hir_err)) => {
                tracing::debug!("HIR-only eval error for {args:?}: {hir_err}");
                divergences.push(Divergence {
                    args: args.clone(),
                    dir_result,
                    hir_result: None,
                });
            }
            (Err(dir_err), Ok(hir_result)) => {
                tracing::debug!("DIR-only eval error for {args:?}: {dir_err}");
                divergences.push(Divergence {
                    args: args.clone(),
                    dir_result: None,
                    hir_result,
                });
            }
        }
    }

    if !divergences.is_empty() {
        return VerifyOutcome::Diverged(divergences);
    }
    if checked == 0 {
        return VerifyOutcome::Unsupported {
            reason: "no sample produced a comparable (Ok, Ok) result -- function likely uses an \
                     unmodeled construct (Load/Store/Call/etc, see interp's module doc)"
                .to_string(),
        };
    }
    VerifyOutcome::Equivalent { checked }
}

/// A small, deliberately boundary-heavy set of concrete argument tuples for
/// an `arity`-parameter function: zero, one, all-bits-set, `i32::MIN`, and a
/// few small values in every combination. Not exhaustive -- Phase 2's
/// solver-backed equivalence check is what closes that gap; this is meant
/// to catch the same class of bug this session already found by hand
/// (specific edge-case inputs), cheaply.
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
    use fission_midend_core::ir::{HirBinaryOp, HirExpr, HirLValue, HirStmt, NirBindingOrigin};

    fn i32_ty() -> fission_midend_core::ir::NirType {
        fission_midend_core::ir::NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    fn bool_ty() -> fission_midend_core::ir::NirType {
        fission_midend_core::ir::NirType::Bool
    }

    fn param(name: &str, idx: usize) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: i32_ty(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(idx)),
            initializer: None,
        }
    }

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    /// `a > b` -- used by both fixtures below.
    fn a_gt_b() -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::SGt,
            lhs: Box::new(var("a")),
            rhs: Box::new(var("b")),
            ty: bool_ty(),
        }
    }

    /// HIR (structured): `if (a > b) { return a; } else { return b; }`.
    fn max_hir() -> Hir {
        Hir(vec![HirStmt::If {
            cond: a_gt_b(),
            then_body: vec![HirStmt::Return(Some(var("a")))],
            else_body: vec![HirStmt::Return(Some(var("b")))],
        }])
    }

    /// DIR (flattened goto/label form of the *same* logic): a conditional
    /// jump (`If` with a single `Goto` in `then_body`, no `else_body` --
    /// this is how a p-code CBranch naturally maps onto `HirStmt` without a
    /// dedicated "conditional goto" variant) to a label guarding the
    /// then-arm, with the else-arm falling straight through.
    fn max_dir() -> Dir {
        Dir(vec![
            HirStmt::If {
                cond: a_gt_b(),
                then_body: vec![HirStmt::Goto("L_then".to_string())],
                else_body: vec![],
            },
            HirStmt::Return(Some(var("b"))),
            HirStmt::Label("L_then".to_string()),
            HirStmt::Return(Some(var("a"))),
        ])
    }

    fn max_params() -> Vec<NirBinding> {
        vec![param("a", 0), param("b", 1)]
    }

    #[test]
    fn equivalent_dir_and_hir_report_equivalent() {
        let outcome = diff_dir_hir(
            &max_dir(),
            &max_hir(),
            &max_params(),
            &[],
            &default_samples(2),
        );
        match outcome {
            VerifyOutcome::Equivalent { checked } => {
                assert_eq!(checked, default_samples(2).len());
            }
            other => panic!("expected Equivalent, got {other:?}"),
        }
    }

    /// A deliberately buggy DIR that always returns `a` (as if a
    /// structuring-style bug had collapsed the conditional away) --
    /// proves the harness actually catches a real divergence instead of
    /// trivially reporting everything as equivalent.
    #[test]
    fn diverging_dir_and_hir_are_caught_with_a_concrete_repro() {
        let buggy_dir = Dir(vec![HirStmt::Return(Some(var("a")))]);
        let outcome = diff_dir_hir(&buggy_dir, &max_hir(), &max_params(), &[], &default_samples(2));
        match outcome {
            VerifyOutcome::Diverged(divs) => {
                assert!(!divs.is_empty());
                // Every sample with a < b should have diverged (buggy DIR
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
        let params = vec![param("n", 0)];
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
                    lhs: Box::new(var("i")),
                    rhs: Box::new(var("n")),
                    ty: bool_ty(),
                },
                body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("acc".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(var("acc")),
                            rhs: Box::new(var("i")),
                            ty: i32_ty(),
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("i".to_string()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(var("i")),
                            rhs: Box::new(HirExpr::Const(1, i32_ty())),
                            ty: i32_ty(),
                        },
                    },
                ],
            },
            HirStmt::Return(Some(var("acc"))),
        ];
        let result = interpret(&body, &params, &locals, &[5]).expect("interpret");
        assert_eq!(result, Some(15)); // 0+1+2+3+4+5
    }
}
