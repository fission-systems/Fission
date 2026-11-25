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

use std::collections::BTreeMap;

use fission_midend_core::ir::HirFunction;
use fission_midend_prehir::PreHirFunction;

use crate::eval::{UndefinedRead, interpret_hir, interpret_prehir};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};

/// Bounded, deterministic category for why a concrete evaluation sample failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SampleErrorKind {
    DivisionOrModuloByZero,
    FuelExhausted,
    UndefinedOrUnmappedMemory,
    UnboundVariable,
    UnmodeledCallOrIntrinsic,
    UnsupportedConstruct,
    ArgumentMismatch,
    MixedErrorCategories,
    Other,
}

impl SampleErrorKind {
    /// Stable lowercase identifier for serialization and telemetry projections.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DivisionOrModuloByZero => "division_or_modulo_by_zero",
            Self::FuelExhausted => "fuel_exhausted",
            Self::UndefinedOrUnmappedMemory => "undefined_or_unmapped_memory",
            Self::UnboundVariable => "unbound_variable",
            Self::UnmodeledCallOrIntrinsic => "unmodeled_call_or_intrinsic",
            Self::UnsupportedConstruct => "unsupported_construct",
            Self::ArgumentMismatch => "argument_mismatch",
            Self::MixedErrorCategories => "mixed_error_categories",
            Self::Other => "other",
        }
    }
}

/// Explicit sampled-coverage status for the concrete PreHIR/HIR comparator.
///
/// The concrete tier evaluates finite sample tuples and never establishes
/// universal equivalence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcreteDiffStatus {
    /// Every requested sample was comparable and agreed (`requested > 0`,
    /// `agreed == requested`, zero divergences and zero errors).
    CompleteSampledAgreement,
    /// At least one sample was comparable and every comparable sample agreed,
    /// with zero divergences or asymmetric errors, but one or more samples
    /// failed symmetrically on both sides (`0 < agreed < requested`).
    PartialSampledAgreement,
    /// At least one sample diverged (value mismatch or asymmetric error).
    Diverged,
    /// No requested sample produced a comparable `(Ok, Ok)` result and no
    /// asymmetric error occurred (empty sample list or all symmetric errors).
    NoComparableEvidence,
}

impl ConcreteDiffStatus {
    /// Stable lowercase identifier for serialization and telemetry projections.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompleteSampledAgreement => "complete_sampled_agreement",
            Self::PartialSampledAgreement => "partial_sampled_agreement",
            Self::Diverged => "diverged",
            Self::NoComparableEvidence => "no_comparable_evidence",
        }
    }
}

/// Reconciled per-sample accounting for a concrete PreHIR/HIR comparison run.
///
/// Invariants:
/// - `requested == comparable + symmetric_errors + asymmetric_errors`
/// - `requested == agreed + divergent + symmetric_errors`
/// - `divergent == (comparable - agreed) + asymmetric_errors`
/// - `symmetric_error_reasons.values().sum::<usize>() == symmetric_errors`
/// - `asymmetric_error_reasons.values().sum::<usize>() == asymmetric_errors`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConcreteSampleCounts {
    pub requested: usize,
    pub comparable: usize,
    pub agreed: usize,
    pub divergent: usize,
    pub symmetric_errors: usize,
    pub asymmetric_errors: usize,
    pub symmetric_error_reasons: BTreeMap<SampleErrorKind, usize>,
    pub asymmetric_error_reasons: BTreeMap<SampleErrorKind, usize>,
}

impl ConcreteSampleCounts {
    /// Verifies that all sample totals and reason maps reconcile.
    #[must_use]
    pub fn reconciles(&self) -> bool {
        let value_divergences = self.comparable.saturating_sub(self.agreed);
        self.agreed <= self.comparable
            && self.divergent == value_divergences + self.asymmetric_errors
            && self.requested == self.comparable + self.symmetric_errors + self.asymmetric_errors
            && self.requested == self.agreed + self.divergent + self.symmetric_errors
            && self.symmetric_error_reasons.values().sum::<usize>() == self.symmetric_errors
            && self.asymmetric_error_reasons.values().sum::<usize>() == self.asymmetric_errors
    }
}

/// Typed concrete PreHIR/HIR comparison report preserving the full sample
/// denominator and bounded error reasons alongside [`VerifyOutcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcreteDiffReport {
    pub status: ConcreteDiffStatus,
    pub counts: ConcreteSampleCounts,
    pub outcome: VerifyOutcome,
}

impl ConcreteDiffReport {
    /// True only when every requested sample was comparable and agreed.
    #[must_use]
    pub const fn is_complete_sampled_agreement(&self) -> bool {
        matches!(self.status, ConcreteDiffStatus::CompleteSampledAgreement)
    }

    /// True when comparable samples agreed, but some requested samples were
    /// excluded due to symmetric evaluation errors.
    #[must_use]
    pub const fn is_partial_sampled_agreement(&self) -> bool {
        matches!(self.status, ConcreteDiffStatus::PartialSampledAgreement)
    }

    /// True when at least one sample produced a comparable `(Ok, Ok)` pair.
    #[must_use]
    pub const fn has_comparable_evidence(&self) -> bool {
        self.counts.comparable > 0
    }

    /// The concrete tier tests finite samples and never produces a universal
    /// proof of equivalence.
    #[must_use]
    pub const fn is_universally_proven(&self) -> bool {
        false
    }
}

fn classify_eval_error(err: &anyhow::Error) -> SampleErrorKind {
    if let Some(undef) = err.downcast_ref::<UndefinedRead>() {
        if undef.what.contains("frame byte") {
            return SampleErrorKind::UndefinedOrUnmappedMemory;
        }
        return SampleErrorKind::UnboundVariable;
    }
    let msg = err.to_string();
    if msg.contains("division by zero") || msg.contains("modulo by zero") {
        SampleErrorKind::DivisionOrModuloByZero
    } else if msg.contains("did not finish within") || msg.contains("more than ") {
        SampleErrorKind::FuelExhausted
    } else if msg.contains("unmapped address")
        || msg.contains("no address for global")
        || msg.contains("no frame address for local")
    {
        SampleErrorKind::UndefinedOrUnmappedMemory
    } else if msg.contains("no declared type for variable") || msg.contains("never assigned") {
        SampleErrorKind::UnboundVariable
    } else if msg.contains("pseudo-call")
        || msg.contains("no machine call instruction")
        || msg.contains("call")
        || msg.contains("intrinsic")
    {
        SampleErrorKind::UnmodeledCallOrIntrinsic
    } else if msg.contains("args given, function has") {
        SampleErrorKind::ArgumentMismatch
    } else if msg.contains("unsupported")
        || msg.contains("variadic")
        || msg.contains("member access on a non-pointer")
        || msg.contains("unexpected control state")
    {
        SampleErrorKind::UnsupportedConstruct
    } else {
        SampleErrorKind::Other
    }
}

/// Differentially interpret `prehir` and `hir` over `samples`, returning full
/// reconciled sample accounting, bounded error categories, and explicit
/// complete/partial/no-comparable-evidence status alongside [`VerifyOutcome`].
pub fn diff_prehir_hir_detailed(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
) -> ConcreteDiffReport {
    let mut divergences = Vec::new();
    let mut counts = ConcreteSampleCounts {
        requested: samples.len(),
        ..ConcreteSampleCounts::default()
    };

    for args in samples {
        let prehir_r = interpret_prehir(&prehir.body, &prehir.params, &prehir.locals, args);
        let hir_r = interpret_hir(&hir.body, &hir.params, &hir.locals, args);

        match (prehir_r, hir_r) {
            (Ok(prehir_result), Ok(hir_result)) => {
                counts.comparable += 1;
                if prehir_result == hir_result {
                    counts.agreed += 1;
                } else {
                    counts.divergent += 1;
                    divergences.push(Divergence {
                        run: None,
                        args: args.clone(),
                        prehir_result,
                        hir_result,
                        emulator_result: None,
                        detail: None,
                    });
                }
            }
            (Err(prehir_err), Err(hir_err)) => {
                counts.symmetric_errors += 1;
                let pre_kind = classify_eval_error(&prehir_err);
                let hir_kind = classify_eval_error(&hir_err);
                let kind = if pre_kind == hir_kind {
                    pre_kind
                } else {
                    SampleErrorKind::MixedErrorCategories
                };
                *counts.symmetric_error_reasons.entry(kind).or_default() += 1;
            }
            (Ok(prehir_result), Err(hir_err)) => {
                let kind = classify_eval_error(&hir_err);
                counts.asymmetric_errors += 1;
                counts.divergent += 1;
                *counts.asymmetric_error_reasons.entry(kind).or_default() += 1;
                tracing::debug!("HIR-only eval error for {args:?}: {hir_err}");
                divergences.push(Divergence {
                    run: None,
                    args: args.clone(),
                    prehir_result,
                    hir_result: None,
                    emulator_result: None,
                    detail: Some(format!("HIR-only eval error ({})", kind.as_str())),
                });
            }
            (Err(prehir_err), Ok(hir_result)) => {
                let kind = classify_eval_error(&prehir_err);
                counts.asymmetric_errors += 1;
                counts.divergent += 1;
                *counts.asymmetric_error_reasons.entry(kind).or_default() += 1;
                tracing::debug!("PreHIR-only eval error for {args:?}: {prehir_err}");
                divergences.push(Divergence {
                    run: None,
                    args: args.clone(),
                    prehir_result: None,
                    hir_result,
                    emulator_result: None,
                    detail: Some(format!("PreHIR-only eval error ({})", kind.as_str())),
                });
            }
        }
    }

    let (status, outcome) = if !divergences.is_empty() {
        (
            ConcreteDiffStatus::Diverged,
            VerifyOutcome::Diverged(divergences),
        )
    } else if counts.requested == 0 {
        (
            ConcreteDiffStatus::NoComparableEvidence,
            VerifyOutcome::Unsupported(UnsupportedReason::Construct(
                "no concrete samples were requested",
            )),
        )
    } else if counts.comparable == 0 {
        (
            ConcreteDiffStatus::NoComparableEvidence,
            VerifyOutcome::Unsupported(UnsupportedReason::Construct(
                "no sample produced a comparable (Ok, Ok) result -- function likely uses an \
                 unmodeled construct (Load/Store/Call/etc, see eval's module doc)",
            )),
        )
    } else if counts.symmetric_errors > 0 {
        (
            ConcreteDiffStatus::PartialSampledAgreement,
            VerifyOutcome::Equivalent {
                checked: counts.agreed,
            },
        )
    } else {
        (
            ConcreteDiffStatus::CompleteSampledAgreement,
            VerifyOutcome::Equivalent {
                checked: counts.agreed,
            },
        )
    };

    ConcreteDiffReport {
        status,
        counts,
        outcome,
    }
}

/// Differentially interpret `prehir` and `hir` -- the real, independently-typed
/// [`PreHirFunction`]/[`HirFunction`] `fission-pcode`'s production pipeline
/// produces for the same source function -- over each tuple in `samples`,
/// in order, and report the legacy [`VerifyOutcome`].
///
/// Prefer [`diff_prehir_hir_detailed`] when callers need the full sample
/// denominator, partial-agreement distinction, or bounded error categories.
pub fn diff_prehir_hir(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    samples: &[Vec<i64>],
) -> VerifyOutcome {
    diff_prehir_hir_detailed(prehir, hir, samples).outcome
}

/// Maximum number of concrete samples generated by [`default_samples`].
pub const MAX_DEFAULT_SAMPLES: usize = 1024;

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
    if arity <= 3 {
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

    // For arity > 3, an unconstrained Cartesian product produces O(7^N) samples
    // (e.g. 117,649 for 6 args), exploding test runtimes. Instead, employ
    // pairwise and boundary coverage capped at MAX_DEFAULT_SAMPLES.
    pairwise_samples(arity, MAX_DEFAULT_SAMPLES)
}

/// Boundary values in diagonal, single-parameter and pairwise variations,
/// at most `cap` of them.
fn pairwise_samples(arity: usize, cap: usize) -> Vec<Vec<i64>> {
    let seeds: [i64; 7] = [0, 1, -1, 2, -2, i32::MIN as i64, i32::MAX as i64];
    let mut out: Vec<Vec<i64>> = Vec::with_capacity(cap);
    let mut seen = std::collections::HashSet::new();

    let add = |sample: Vec<i64>,
               out: &mut Vec<Vec<i64>>,
               seen: &mut std::collections::HashSet<Vec<i64>>| {
        if out.len() < cap && seen.insert(sample.clone()) {
            out.push(sample);
        }
    };

    // 1. All-same diagonal values: [s, s, s, ...]
    for &s in &seeds {
        add(vec![s; arity], &mut out, &mut seen);
    }

    // 2. Single-parameter variation around baseline 0
    for p in 0..arity {
        for &s in &seeds {
            let mut v = vec![0; arity];
            v[p] = s;
            add(v, &mut out, &mut seen);
        }
    }

    // 3. Single-parameter variation around baseline 1
    for p in 0..arity {
        for &s in &seeds {
            let mut v = vec![1; arity];
            v[p] = s;
            add(v, &mut out, &mut seen);
        }
    }

    // 4. Pairwise parameter variations
    for i in 0..arity {
        for j in (i + 1)..arity {
            for &s1 in &seeds {
                for &s2 in &seeds {
                    let mut v = vec![0; arity];
                    v[i] = s1;
                    v[j] = s2;
                    add(v, &mut out, &mut seen);
                    if out.len() >= cap {
                        return out;
                    }
                }
            }
        }
    }

    out
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

    #[test]
    fn test_default_samples_bounds_and_coverage() {
        assert_eq!(default_samples(0), vec![Vec::<i64>::new()]);
        assert_eq!(default_samples(1).len(), 7);
        assert_eq!(default_samples(2).len(), 49);
        assert_eq!(default_samples(3).len(), 343);

        let samples_4 = default_samples(4);
        assert!(samples_4.len() <= MAX_DEFAULT_SAMPLES);
        assert!(samples_4.len() >= 49);
        for s in &samples_4 {
            assert_eq!(s.len(), 4);
        }

        let samples_6 = default_samples(6);
        assert!(samples_6.len() <= MAX_DEFAULT_SAMPLES);
        for s in &samples_6 {
            assert_eq!(s.len(), 6);
        }
    }

    fn div_ten_by_x_pair() -> (PreHirFunction, HirFunction) {
        let pre = PreHirFunction {
            params: vec![prehir_param("x", 0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Div,
                lhs: Box::new(PreHirExpr::Const(10, i32_ty())),
                rhs: Box::new(prehir_var("x")),
                ty: i32_ty(),
            }))],
            ..Default::default()
        };
        let hir = HirFunction {
            params: vec![hir_param("x", 0)],
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Div,
                lhs: Box::new(HirExpr::Const(10, i32_ty())),
                rhs: Box::new(hvar("x")),
                ty: i32_ty(),
            }))],
            ..Default::default()
        };
        (pre, hir)
    }

    #[test]
    fn detailed_diff_accounts_for_complete_partial_empty_all_error_and_asymmetric_samples() {
        // 1. Complete sampled agreement: all requested samples comparable and agreed.
        let samples_ok = vec![vec![1, 2], vec![5, 3], vec![-1, -4]];
        let complete = diff_prehir_hir_detailed(&max_dir(), &max_hir(), &samples_ok);
        assert_eq!(
            complete.status,
            ConcreteDiffStatus::CompleteSampledAgreement
        );
        assert!(complete.is_complete_sampled_agreement());
        assert!(!complete.is_partial_sampled_agreement());
        assert!(complete.has_comparable_evidence());
        assert!(!complete.is_universally_proven());
        assert!(complete.counts.reconciles());
        assert_eq!(complete.counts.requested, 3);
        assert_eq!(complete.counts.comparable, 3);
        assert_eq!(complete.counts.agreed, 3);
        assert_eq!(complete.counts.divergent, 0);
        assert_eq!(complete.counts.symmetric_errors, 0);
        assert_eq!(complete.counts.asymmetric_errors, 0);
        assert!(complete.counts.symmetric_error_reasons.is_empty());
        assert_eq!(complete.outcome, VerifyOutcome::Equivalent { checked: 3 });

        // 2. Mixed successful and symmetric-error samples (`10 / x` over `[1, 2, 0, 5, 0]`):
        //    report preserves the full 5-sample denominator and `DivisionOrModuloByZero: 2`.
        let (pre_div, hir_div) = div_ten_by_x_pair();
        let mixed_samples = vec![vec![1], vec![2], vec![0], vec![5], vec![0]];
        let partial = diff_prehir_hir_detailed(&pre_div, &hir_div, &mixed_samples);
        assert_eq!(partial.status, ConcreteDiffStatus::PartialSampledAgreement);
        assert!(!partial.is_complete_sampled_agreement());
        assert!(partial.is_partial_sampled_agreement());
        assert!(partial.has_comparable_evidence());
        assert!(!partial.is_universally_proven());
        assert!(partial.counts.reconciles());
        assert_eq!(partial.counts.requested, 5);
        assert_eq!(partial.counts.comparable, 3);
        assert_eq!(partial.counts.agreed, 3);
        assert_eq!(partial.counts.divergent, 0);
        assert_eq!(partial.counts.symmetric_errors, 2);
        assert_eq!(partial.counts.asymmetric_errors, 0);
        assert_eq!(
            partial
                .counts
                .symmetric_error_reasons
                .get(&SampleErrorKind::DivisionOrModuloByZero),
            Some(&2)
        );
        assert_eq!(partial.outcome, VerifyOutcome::Equivalent { checked: 3 });

        // 3. Empty sample list -> NoComparableEvidence and Unsupported.
        let empty = diff_prehir_hir_detailed(&pre_div, &hir_div, &[]);
        assert_eq!(empty.status, ConcreteDiffStatus::NoComparableEvidence);
        assert!(!empty.has_comparable_evidence());
        assert!(!empty.is_universally_proven());
        assert!(empty.counts.reconciles());
        assert_eq!(empty.counts.requested, 0);
        assert!(matches!(empty.outcome, VerifyOutcome::Unsupported(_)));

        // 4. All-errors (`x == 0` repeated) -> NoComparableEvidence with exact error counts.
        let all_err = diff_prehir_hir_detailed(&pre_div, &hir_div, &[vec![0], vec![0]]);
        assert_eq!(all_err.status, ConcreteDiffStatus::NoComparableEvidence);
        assert!(!all_err.has_comparable_evidence());
        assert!(all_err.counts.reconciles());
        assert_eq!(all_err.counts.requested, 2);
        assert_eq!(all_err.counts.comparable, 0);
        assert_eq!(all_err.counts.symmetric_errors, 2);
        assert_eq!(
            all_err
                .counts
                .symmetric_error_reasons
                .get(&SampleErrorKind::DivisionOrModuloByZero),
            Some(&2)
        );
        assert!(matches!(all_err.outcome, VerifyOutcome::Unsupported(_)));

        // 5. Asymmetric error + value divergence + symmetric error + agreed in one batch:
        //    - x = 0: both take `10 / x` -> divide by 0 (symmetric error)
        //    - x = 1: PreHIR `10 / 1 = 10`, HIR `10 / (1 - 2) = -10` (value divergence)
        //    - x = 2: PreHIR `10 / 2 = 5`, HIR `10 / (2 - 2)` -> Err (asymmetric error)
        //    - x = 5: both take `10 / 5 = 2` (agreed)
        let hir_asym = HirFunction {
            params: vec![hir_param("x", 0)],
            body: vec![HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::LogicalOr,
                    lhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::SLe,
                        lhs: Box::new(hvar("x")),
                        rhs: Box::new(HirExpr::Const(0, i32_ty())),
                        ty: bool_ty(),
                    }),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::SGt,
                        lhs: Box::new(hvar("x")),
                        rhs: Box::new(HirExpr::Const(2, i32_ty())),
                        ty: bool_ty(),
                    }),
                    ty: bool_ty(),
                },
                then_body: vec![HirStmt::Return(Some(HirExpr::Binary {
                    op: HirBinaryOp::Div,
                    lhs: Box::new(HirExpr::Const(10, i32_ty())),
                    rhs: Box::new(hvar("x")),
                    ty: i32_ty(),
                }))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Binary {
                    op: HirBinaryOp::Div,
                    lhs: Box::new(HirExpr::Const(10, i32_ty())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(hvar("x")),
                        rhs: Box::new(HirExpr::Const(2, i32_ty())),
                        ty: i32_ty(),
                    }),
                    ty: i32_ty(),
                }))],
            }],
            ..Default::default()
        };
        let asym_report =
            diff_prehir_hir_detailed(&pre_div, &hir_asym, &[vec![0], vec![1], vec![2], vec![5]]);
        assert_eq!(asym_report.status, ConcreteDiffStatus::Diverged);
        assert!(asym_report.counts.reconciles());
        assert_eq!(asym_report.counts.requested, 4);
        assert_eq!(asym_report.counts.comparable, 2); // x=1 (value mismatch) and x=5 (agreed)
        assert_eq!(asym_report.counts.agreed, 1);
        assert_eq!(asym_report.counts.divergent, 2); // x=1 (mismatch) and x=2 (asymmetric error)
        assert_eq!(asym_report.counts.symmetric_errors, 1); // x=0
        assert_eq!(asym_report.counts.asymmetric_errors, 1); // x=2
        assert_eq!(
            asym_report
                .counts
                .symmetric_error_reasons
                .get(&SampleErrorKind::DivisionOrModuloByZero),
            Some(&1)
        );
        assert_eq!(
            asym_report
                .counts
                .asymmetric_error_reasons
                .get(&SampleErrorKind::DivisionOrModuloByZero),
            Some(&1)
        );
        assert!(matches!(asym_report.outcome, VerifyOutcome::Diverged(ref d) if d.len() == 2));
    }
}
