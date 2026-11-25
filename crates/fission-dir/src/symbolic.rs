//! Solver-backed PreHIR/HIR symbolic equivalence (the symbolic tier).
//!
//! Orchestrates [`crate::lower_sym`] (AST -> `SymExpr`) and
//! `fission_solver::Solver` (the real SAT/SMT-style solver): lower PreHIR's and
//! HIR's return value to one closed-form `SymExpr` each over a *shared* set
//! of parameter `Var`s, assert they're unequal, and ask the solver to
//! decide. `Unsat` is a genuine proof of equivalence (for the modeled
//! subset -- see [`crate::lower_sym`]'s scope notes), not "no counterexample
//! found among a few samples" the way [`crate::diff`]'s boundary-value
//! sweep is. `Sat` gives a real counterexample -- the exact input the
//! solver found the two sides diverge on, immediately replayable through
//! [`crate::ground_truth`] to see what the real machine actually computes
//! there.

use std::collections::{HashMap, HashSet};

use fission_midend_core::ir::{HirFunction, NirBindingOrigin, NirType};
use fission_midend_prehir::PreHirFunction;
use fission_solver::Solver;
use fission_solver::ast::{Sort, SymExpr, SymNodeId};
use fission_solver::solver::SatResult;

use crate::eval::{interpret_hir, interpret_prehir};
use crate::lower_sym::{self, LowerSymError, checked_scalar_type};
use crate::product::{
    AssumptionKind, AssuranceBand, AssuranceClassification, AssuranceContractError,
    AssuranceReport, CallObservation, DirCounterexample, EvidenceStrength, FoundationIdentity,
    ObservationScope, ValidationAssumption, ValidationBudget, ValidationEvidence,
    ValidationVerdict,
};
use crate::report::UnsupportedReason;

/// Explicit observation scope for the symbolic PreHIR/HIR tier: checks AST
/// scalar return values for pure loop-free bodies only, never whole-machine
/// state, memory side effects, external calls, or traps.
#[must_use]
pub fn symbolic_ast_return_scope() -> ObservationScope {
    ObservationScope {
        return_value: true,
        exit_kind: false,
        live_out_locations: Vec::new(),
        memory_ranges: Vec::new(),
        calls: CallObservation::Ignored,
        traps: false,
    }
}

/// A validated solver-found counterexample: PreHIR and HIR provably diverge on
/// `args` (each carrying its exact declared bit width in `arg_widths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterexample {
    pub args: Vec<u64>,
    pub arg_widths: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolicOutcome {
    /// `Unsat` -- proved equivalent for the modeled subset and checked
    /// input/output correspondence.
    Equivalent,
    /// `Sat` -- a genuine, solver-found counterexample confirmed by symbolic
    /// evaluation and independent concrete PreHIR/HIR replay.
    Diverged(Counterexample),
    /// Not checkable at this tier (contains a loop/unsupported construct or
    /// unsupported type), or the solver returned `Unknown`.
    Unsupported(UnsupportedReason),
    /// Input correspondence, width/signedness domain, or witness validation
    /// prevents claiming equivalence or confirmed divergence.
    Inconclusive { reason: String },
}

/// Basis establishing why parameter `index` in PreHIR corresponds to parameter
/// `index` in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamIdentityBasis {
    /// Both sides share the same explicit storage/ABI origin.
    StorageOrigin(NirBindingOrigin),
    /// Neither side carries a storage origin (nor conflicting `param_N` hints);
    /// pairing rests on parameter list position and makes equivalence conditional.
    PositionalFallback,
}

/// Checked input correspondence record for one parameter pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamCorrespondence {
    pub index: usize,
    pub prehir_name: String,
    pub hir_name: String,
    pub basis: ParamIdentityBasis,
    pub prehir_type: NirType,
    pub hir_type: NirType,
    pub prehir_bits: u32,
    pub hir_bits: u32,
    pub shared_domain_bits: u32,
    pub width_matches: bool,
    pub signedness_matches: bool,
}

/// How PreHIR and HIR return values are compared at the symbolic tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReturnComparisonMode {
    /// Require identical scalar width and signedness on declared types and
    /// lowered expressions; no implicit widening or sign reinterpretation.
    ExactScalar,
    /// Compare exact mathematical integer values (widening narrower same-sign
    /// values explicitly, and checking signed vs unsigned value ranges without
    /// silently conflating negative signed values with large unsigned bits).
    /// Any cross-width or cross-signedness check records an explicit assumption.
    #[default]
    LanguageValue,
    /// Require equal bit width on both sides and compare raw bit patterns
    /// directly without zero/sign padding across widths.
    BitPattern,
}

/// Exact return-value observation relation used for a symbolic query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnObservationRelation {
    ExactScalar {
        bits: u32,
        signed: bool,
    },
    LanguageValueWidened {
        prehir_bits: u32,
        hir_bits: u32,
        cmp_bits: u32,
        signed: bool,
    },
    LanguageValueSignednessChecked {
        prehir_bits: u32,
        prehir_signed: bool,
        hir_bits: u32,
        hir_signed: bool,
        cmp_bits: u32,
    },
    RawBitPattern {
        bits: u32,
        prehir_signed: bool,
        hir_signed: bool,
    },
}

/// Checked parameter and return-observation mapping between PreHIR and HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolicInputCorrespondence {
    pub params: Vec<ParamCorrespondence>,
    pub return_mode: ReturnComparisonMode,
    pub return_relation: ReturnObservationRelation,
    pub prehir_return_type: NirType,
    pub hir_return_type: NirType,
    pub prehir_return_bits: u32,
    pub hir_return_bits: u32,
    pub comparison_bits: u32,
}

/// Full symbolic verification report reusing [`crate::product`]'s verdict,
/// scope, assumption, and evidence taxonomy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolicEquivalenceReport {
    pub outcome: SymbolicOutcome,
    pub correspondence: Option<SymbolicInputCorrespondence>,
    pub rejection_reason: Option<String>,
    pub scope: ObservationScope,
    pub verdict: ValidationVerdict,
    pub evidence: ValidationEvidence,
}

impl SymbolicEquivalenceReport {
    /// User-facing assurance band derived identically to [`AssuranceReport::band`].
    #[must_use]
    pub fn band(&self) -> AssuranceBand {
        if self.verdict != ValidationVerdict::Equivalent {
            return AssuranceBand::Unverified;
        }
        match self.evidence.strength {
            EvidenceStrength::Universal
                if self.evidence.assumptions.is_empty()
                    && self.evidence.unresolved_effects.is_empty() =>
            {
                AssuranceBand::Proven
            }
            EvidenceStrength::Universal | EvidenceStrength::Conditional
                if self.evidence.unresolved_effects.is_empty() =>
            {
                AssuranceBand::ConditionallyProven
            }
            EvidenceStrength::Universal | EvidenceStrength::Conditional => {
                AssuranceBand::Unverified
            }
            EvidenceStrength::BoundedPaths => AssuranceBand::PathValidated,
            EvidenceStrength::Traces => AssuranceBand::TraceValidated,
            EvidenceStrength::None => AssuranceBand::Unverified,
        }
    }

    /// Exhaustive classification matching [`AssuranceReport::classification`].
    #[must_use]
    pub fn classification(&self) -> AssuranceClassification {
        match self.verdict {
            ValidationVerdict::Counterexample => AssuranceClassification::Refuted,
            ValidationVerdict::Inconclusive => AssuranceClassification::Inconclusive,
            ValidationVerdict::Unsupported => AssuranceClassification::Unsupported,
            ValidationVerdict::Equivalent => match self.band() {
                AssuranceBand::Proven => AssuranceClassification::UniversallyProven,
                AssuranceBand::ConditionallyProven => AssuranceClassification::ConditionallyProven,
                AssuranceBand::PathValidated => AssuranceClassification::PathSampledOnly,
                AssuranceBand::TraceValidated => AssuranceClassification::TraceSampledOnly,
                AssuranceBand::Unverified => AssuranceClassification::Inconclusive,
            },
        }
    }

    /// True only when equivalence was proved with matched parameter widths,
    /// matched storage/ABI origins, exact scalar return equality, and zero
    /// additional assumptions.
    #[must_use]
    pub fn is_unconditionally_proven(&self) -> bool {
        self.band().is_unconditionally_proven()
    }

    /// True for universal or conditional symbolic proofs.
    #[must_use]
    pub fn is_proven(&self) -> bool {
        self.band().is_proven()
    }

    /// Project this report into a canonical [`AssuranceReport`] bound to a
    /// P-code [`FoundationIdentity`].
    pub fn to_assurance_report(
        &self,
        foundation: FoundationIdentity,
    ) -> Result<AssuranceReport, AssuranceContractError> {
        AssuranceReport::new(
            foundation,
            self.scope.clone(),
            self.verdict,
            self.evidence.clone(),
        )
    }
}

/// The only solver verdict that proves equivalence is `Unsat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SolverVerdict {
    Equivalent,
    Counterexample,
    Unknown,
}

pub(crate) fn classify_solver_result(result: SatResult) -> SolverVerdict {
    match result {
        SatResult::Unsat => SolverVerdict::Equivalent,
        SatResult::Sat => SolverVerdict::Counterexample,
        SatResult::Unknown => SolverVerdict::Unknown,
    }
}

fn validate_scalar_type(ty: &NirType) -> Result<(u32, bool), &'static str> {
    checked_scalar_type(ty)
}

fn is_valid_param_origin(origin: NirBindingOrigin) -> bool {
    matches!(
        origin,
        NirBindingOrigin::ParamIndex(_)
            | NirBindingOrigin::StackOffset(_)
            | NirBindingOrigin::HomeSlot(_)
            | NirBindingOrigin::DerivedFromStackOffset(_)
    )
}

fn parse_positional_slot_hint(name: &str) -> Option<usize> {
    name.strip_prefix("param_")
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
}

#[derive(Debug, Clone)]
struct ParamSpec {
    name: String,
    var: SymExpr,
    var_id: SymNodeId,
    width_bits: u32,
}

enum PreparedInequality {
    DirectPair {
        prehir_cmp: SymExpr,
        hir_cmp: SymExpr,
    },
    SignedUnsignedPair {
        prehir_expr: SymExpr,
        hir_expr: SymExpr,
        predicate: SymExpr,
    },
}

struct PreparedSymbolicQuery {
    prehir_expr: SymExpr,
    hir_expr: SymExpr,
    inequality: PreparedInequality,
    correspondence: SymbolicInputCorrespondence,
    assumptions: Vec<ValidationAssumption>,
    param_specs: Vec<ParamSpec>,
}

fn format_failure_evidence(detail: impl Into<String>) -> ValidationEvidence {
    ValidationEvidence::none(detail)
}

fn inconclusive_report(
    correspondence: Option<SymbolicInputCorrespondence>,
    assumptions: Vec<ValidationAssumption>,
    reason: impl Into<String>,
) -> SymbolicEquivalenceReport {
    let reason = reason.into();
    let mut evidence = format_failure_evidence(reason.clone());
    evidence.assumptions = assumptions;
    SymbolicEquivalenceReport {
        outcome: SymbolicOutcome::Inconclusive {
            reason: reason.clone(),
        },
        correspondence,
        rejection_reason: Some(reason),
        scope: symbolic_ast_return_scope(),
        verdict: ValidationVerdict::Inconclusive,
        evidence,
    }
}

fn unsupported_report(
    correspondence: Option<SymbolicInputCorrespondence>,
    assumptions: Vec<ValidationAssumption>,
    reason: UnsupportedReason,
    detail: impl Into<String>,
) -> SymbolicEquivalenceReport {
    let detail = detail.into();
    let mut evidence = format_failure_evidence(detail.clone());
    evidence.assumptions = assumptions;
    SymbolicEquivalenceReport {
        outcome: SymbolicOutcome::Unsupported(reason),
        correspondence,
        rejection_reason: Some(detail),
        scope: symbolic_ast_return_scope(),
        verdict: ValidationVerdict::Unsupported,
        evidence,
    }
}

fn inconclusive_err(
    correspondence: Option<SymbolicInputCorrespondence>,
    assumptions: Vec<ValidationAssumption>,
    reason: impl Into<String>,
) -> Box<SymbolicEquivalenceReport> {
    Box::new(inconclusive_report(correspondence, assumptions, reason))
}

fn unsupported_err(
    correspondence: Option<SymbolicInputCorrespondence>,
    assumptions: Vec<ValidationAssumption>,
    reason: UnsupportedReason,
    detail: impl Into<String>,
) -> Box<SymbolicEquivalenceReport> {
    Box::new(unsupported_report(
        correspondence,
        assumptions,
        reason,
        detail,
    ))
}

fn validate_abi_signature(
    prehir: &PreHirFunction,
    hir: &HirFunction,
) -> Result<(), Box<SymbolicEquivalenceReport>> {
    if hir.has_unknown_arity_prototype {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "HIR function has unknown-arity prototype; cannot establish exact symbolic input domain",
        ));
    }
    if prehir.variadic_fixed_arity.is_some() || hir.variadic_fixed_arity.is_some() {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "variadic functions cannot establish a complete symbolic input domain",
        ));
    }
    if prehir.is_64bit != hir.is_64bit {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            format!(
                "PreHIR and HIR disagree on pointer/register bitness ({} vs {})",
                if prehir.is_64bit { 64 } else { 32 },
                if hir.is_64bit { 64 } else { 32 }
            ),
        ));
    }
    if prehir.is_big_endian != hir.is_big_endian {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "PreHIR and HIR disagree on endianness",
        ));
    }
    if prehir.suppress_entry_register_params != hir.suppress_entry_register_params {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "PreHIR and HIR disagree on entry register parameter suppression",
        ));
    }
    if prehir.calling_convention != hir.calling_convention {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            format!(
                "PreHIR and HIR disagree on calling convention ({:?} vs {:?})",
                prehir.calling_convention, hir.calling_convention
            ),
        ));
    }
    if !prehir.int_param_offsets.is_empty()
        && !hir.int_param_offsets.is_empty()
        && prehir.int_param_offsets != hir.int_param_offsets
    {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "PreHIR and HIR disagree on integer parameter ABI slot offsets",
        ));
    }
    if !prehir.float_param_offsets.is_empty()
        && !hir.float_param_offsets.is_empty()
        && prehir.float_param_offsets != hir.float_param_offsets
    {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            "PreHIR and HIR disagree on float parameter ABI slot offsets",
        ));
    }
    Ok(())
}

fn validate_side_bindings<F>(
    side: &str,
    params_len: usize,
    params: F,
) -> Result<(), Box<SymbolicEquivalenceReport>>
where
    F: Iterator<Item = (usize, String, Option<NirBindingOrigin>, bool)>,
{
    let mut seen_names = HashSet::with_capacity(params_len);
    let mut seen_origins: Vec<NirBindingOrigin> = Vec::with_capacity(params_len);
    let mut seen_hints = HashSet::with_capacity(params_len);

    for (idx, name, origin, has_init) in params {
        if name.is_empty() {
            return Err(inconclusive_err(
                None,
                Vec::new(),
                format!("{side} parameter at index {idx} has an empty name"),
            ));
        }
        if !seen_names.insert(name.clone()) {
            return Err(inconclusive_err(
                None,
                Vec::new(),
                format!("{side} declares duplicate parameter name '{name}'"),
            ));
        }
        if has_init {
            return Err(inconclusive_err(
                None,
                Vec::new(),
                format!("{side} parameter '{name}' unexpectedly carries an initializer"),
            ));
        }
        if let Some(org) = origin {
            if !is_valid_param_origin(org) {
                return Err(inconclusive_err(
                    None,
                    Vec::new(),
                    format!(
                        "{side} parameter '{name}' carries non-parameter storage origin {org:?}"
                    ),
                ));
            }
            if seen_origins.contains(&org) {
                return Err(inconclusive_err(
                    None,
                    Vec::new(),
                    format!("{side} declares duplicate parameter origin {org:?}"),
                ));
            }
            seen_origins.push(org);
        } else if let Some(slot) = parse_positional_slot_hint(&name)
            && !seen_hints.insert(slot)
        {
            return Err(inconclusive_err(
                None,
                Vec::new(),
                format!("{side} declares duplicate implicit parameter slot hint '{name}'"),
            ));
        }
    }
    Ok(())
}

fn prepare_symbolic_query(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    return_mode: ReturnComparisonMode,
) -> Result<PreparedSymbolicQuery, Box<SymbolicEquivalenceReport>> {
    if lower_sym::prehir::contains_loop(&prehir.body) || lower_sym::hir::contains_loop(&hir.body) {
        let reason = UnsupportedReason::ContainsLoop;
        let detail = reason.to_string();
        return Err(unsupported_err(None, Vec::new(), reason, detail));
    }

    validate_abi_signature(prehir, hir)?;

    if prehir.params.len() != hir.params.len() {
        return Err(inconclusive_err(
            None,
            Vec::new(),
            format!(
                "PreHIR has {} parameter(s) while HIR has {} -- cannot align input domains",
                prehir.params.len(),
                hir.params.len()
            ),
        ));
    }

    validate_side_bindings(
        "PreHIR",
        prehir.params.len(),
        prehir
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.clone(), p.origin, p.initializer.is_some())),
    )?;
    validate_side_bindings(
        "HIR",
        hir.params.len(),
        hir.params
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.clone(), p.origin, p.initializer.is_some())),
    )?;

    let (pre_ret_bits, pre_ret_signed) =
        validate_scalar_type(&prehir.return_type).map_err(|msg| {
            let reason = UnsupportedReason::UnsupportedType(msg);
            let detail = reason.to_string();
            unsupported_err(None, Vec::new(), reason, detail)
        })?;
    let (hir_ret_bits, hir_ret_signed) = validate_scalar_type(&hir.return_type).map_err(|msg| {
        let reason = UnsupportedReason::UnsupportedType(msg);
        let detail = reason.to_string();
        unsupported_err(None, Vec::new(), reason, detail)
    })?;

    let mut assumptions = Vec::new();
    let mut param_correspondences = Vec::with_capacity(hir.params.len());
    let mut param_specs = Vec::with_capacity(hir.params.len());
    let mut param_vars = Vec::with_capacity(hir.params.len());

    for (idx, (pre_p, hir_p)) in prehir.params.iter().zip(&hir.params).enumerate() {
        let (pre_bits, pre_signed) = validate_scalar_type(&pre_p.ty).map_err(|msg| {
            let reason = UnsupportedReason::UnsupportedType(msg);
            let detail = reason.to_string();
            unsupported_err(None, Vec::new(), reason, detail)
        })?;
        let (hir_bits, hir_signed) = validate_scalar_type(&hir_p.ty).map_err(|msg| {
            let reason = UnsupportedReason::UnsupportedType(msg);
            let detail = reason.to_string();
            unsupported_err(None, Vec::new(), reason, detail)
        })?;

        let basis = match (pre_p.origin, hir_p.origin) {
            (Some(pre_origin), Some(hir_origin)) => {
                if pre_origin != hir_origin {
                    return Err(inconclusive_err(
                        None,
                        assumptions,
                        format!(
                            "parameter {idx} storage/ABI origin mismatch: PreHIR '{}' has {:?} \
                             while HIR '{}' has {:?}",
                            pre_p.name, pre_origin, hir_p.name, hir_origin
                        ),
                    ));
                }
                ParamIdentityBasis::StorageOrigin(pre_origin)
            }
            (Some(pre_origin), None) => {
                return Err(inconclusive_err(
                    None,
                    assumptions,
                    format!(
                        "parameter {idx} provenance mismatch: PreHIR '{}' has origin {:?} \
                         while HIR '{}' has no origin",
                        pre_p.name, pre_origin, hir_p.name
                    ),
                ));
            }
            (None, Some(hir_origin)) => {
                return Err(inconclusive_err(
                    None,
                    assumptions,
                    format!(
                        "parameter {idx} provenance mismatch: PreHIR '{}' has no origin \
                         while HIR '{}' has origin {:?}",
                        pre_p.name, hir_p.name, hir_origin
                    ),
                ));
            }
            (None, None) => {
                if let (Some(pre_slot), Some(hir_slot)) = (
                    parse_positional_slot_hint(&pre_p.name),
                    parse_positional_slot_hint(&hir_p.name),
                ) && pre_slot != hir_slot
                {
                    return Err(inconclusive_err(
                        None,
                        assumptions,
                        format!(
                            "parameter {idx} implicit ABI slot hint mismatch: PreHIR '{}' vs HIR '{}'",
                            pre_p.name, hir_p.name
                        ),
                    ));
                }
                assumptions.push(ValidationAssumption {
                    kind: AssumptionKind::CallingConvention,
                    description: format!(
                        "parameter {idx} ('{}' <-> '{}') paired positionally without stable \
                         storage/ABI origin metadata",
                        pre_p.name, hir_p.name
                    ),
                });
                ParamIdentityBasis::PositionalFallback
            }
        };

        let shared_domain_bits = pre_bits.max(hir_bits);
        let var = SymExpr::new_var(&hir_p.name, shared_domain_bits);
        let SymExpr::Var { id: var_id, .. } = var else {
            unreachable!("SymExpr::new_var always returns Var");
        };
        param_vars.push(var.clone());
        param_specs.push(ParamSpec {
            name: hir_p.name.clone(),
            var,
            var_id,
            width_bits: shared_domain_bits,
        });
        param_correspondences.push(ParamCorrespondence {
            index: idx,
            prehir_name: pre_p.name.clone(),
            hir_name: hir_p.name.clone(),
            basis,
            prehir_type: pre_p.ty.clone(),
            hir_type: hir_p.ty.clone(),
            prehir_bits: pre_bits,
            hir_bits,
            shared_domain_bits,
            width_matches: pre_bits == hir_bits,
            signedness_matches: pre_signed == hir_signed,
        });
    }

    let prehir_result = match lower_sym::prehir::lower_checked(
        &prehir.body,
        &prehir.params,
        &prehir.locals,
        &param_vars,
    ) {
        Ok(e) => e,
        Err(err) => return Err(Box::new(map_lowering_error("PreHIR", err, assumptions))),
    };
    let hir_result =
        match lower_sym::hir::lower_checked(&hir.body, &hir.params, &hir.locals, &param_vars) {
            Ok(e) => e,
            Err(err) => return Err(Box::new(map_lowering_error("HIR", err, assumptions))),
        };

    let pre_expr_bits = checked_sym_bits(&prehir_result, assumptions.clone())?;
    let hir_expr_bits = checked_sym_bits(&hir_result, assumptions.clone())?;

    let (return_relation, cmp_bits, inequality) = build_return_inequality(
        prehir_result.clone(),
        hir_result.clone(),
        pre_expr_bits,
        pre_ret_bits,
        pre_ret_signed,
        hir_expr_bits,
        hir_ret_bits,
        hir_ret_signed,
        return_mode,
        &mut assumptions,
    )?;

    let correspondence = SymbolicInputCorrespondence {
        params: param_correspondences,
        return_mode,
        return_relation,
        prehir_return_type: prehir.return_type.clone(),
        hir_return_type: hir.return_type.clone(),
        prehir_return_bits: pre_ret_bits,
        hir_return_bits: hir_ret_bits,
        comparison_bits: cmp_bits,
    };

    Ok(PreparedSymbolicQuery {
        prehir_expr: prehir_result,
        hir_expr: hir_result,
        inequality,
        correspondence,
        assumptions,
        param_specs,
    })
}

fn map_lowering_error(
    side: &str,
    err: LowerSymError,
    assumptions: Vec<ValidationAssumption>,
) -> SymbolicEquivalenceReport {
    tracing::debug!("lower_sym: {side} side failed: {err}");
    match err {
        LowerSymError::UnsupportedType(msg) => {
            let reason = UnsupportedReason::UnsupportedType(msg);
            let detail = reason.to_string();
            unsupported_report(None, assumptions, reason, detail)
        }
        LowerSymError::InvalidBinding(msg) => inconclusive_report(None, assumptions, msg),
        LowerSymError::UnsupportedConstruct(detail) => {
            let reason = UnsupportedReason::Construct(if side == "PreHIR" {
                "PreHIR side uses a construct the symbolic tier doesn't model (see debug logs)"
            } else {
                "HIR side uses a construct the symbolic tier doesn't model (see debug logs)"
            });
            unsupported_report(None, assumptions, reason, detail)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_return_inequality(
    prehir_result: SymExpr,
    hir_result: SymExpr,
    pre_expr_bits: u32,
    pre_ret_bits: u32,
    pre_ret_signed: bool,
    hir_expr_bits: u32,
    hir_ret_bits: u32,
    hir_ret_signed: bool,
    return_mode: ReturnComparisonMode,
    assumptions: &mut Vec<ValidationAssumption>,
) -> Result<(ReturnObservationRelation, u32, PreparedInequality), Box<SymbolicEquivalenceReport>> {
    match return_mode {
        ReturnComparisonMode::ExactScalar => {
            if pre_ret_bits != hir_ret_bits
                || pre_ret_signed != hir_ret_signed
                || pre_expr_bits != pre_ret_bits
                || hir_expr_bits != hir_ret_bits
            {
                return Err(inconclusive_err(
                    None,
                    assumptions.clone(),
                    format!(
                        "ExactScalar return comparison requires identical width and signedness \
                         (PreHIR: declared {}b {}, lowered {}b; HIR: declared {}b {}, lowered {}b)",
                        pre_ret_bits,
                        signed_label(pre_ret_signed),
                        pre_expr_bits,
                        hir_ret_bits,
                        signed_label(hir_ret_signed),
                        hir_expr_bits
                    ),
                ));
            }
            Ok((
                ReturnObservationRelation::ExactScalar {
                    bits: pre_ret_bits,
                    signed: pre_ret_signed,
                },
                pre_ret_bits,
                PreparedInequality::DirectPair {
                    prehir_cmp: prehir_result,
                    hir_cmp: hir_result,
                },
            ))
        }
        ReturnComparisonMode::BitPattern => {
            if pre_ret_bits != hir_ret_bits
                || pre_expr_bits != pre_ret_bits
                || hir_expr_bits != hir_ret_bits
            {
                return Err(inconclusive_err(
                    None,
                    assumptions.clone(),
                    format!(
                        "BitPattern return comparison requires matching bit widths with no \
                         implicit extension (PreHIR: declared {}b / lowered {}b; HIR: declared {}b / lowered {}b)",
                        pre_ret_bits, pre_expr_bits, hir_ret_bits, hir_expr_bits
                    ),
                ));
            }
            if pre_ret_signed != hir_ret_signed {
                assumptions.push(ValidationAssumption {
                    kind: AssumptionKind::CallingConvention,
                    description: format!(
                        "return values compared as raw {}-bit bit-patterns despite signedness \
                         difference (PreHIR: {}, HIR: {})",
                        pre_ret_bits,
                        signed_label(pre_ret_signed),
                        signed_label(hir_ret_signed)
                    ),
                });
            }
            Ok((
                ReturnObservationRelation::RawBitPattern {
                    bits: pre_ret_bits,
                    prehir_signed: pre_ret_signed,
                    hir_signed: hir_ret_signed,
                },
                pre_ret_bits,
                PreparedInequality::DirectPair {
                    prehir_cmp: prehir_result,
                    hir_cmp: hir_result,
                },
            ))
        }
        ReturnComparisonMode::LanguageValue => {
            if pre_expr_bits != pre_ret_bits || hir_expr_bits != hir_ret_bits {
                return Err(inconclusive_err(
                    None,
                    assumptions.clone(),
                    format!(
                        "lowered return expression width differs from declared return type \
                         (PreHIR: declared {}b vs lowered {}b; HIR: declared {}b vs lowered {}b)",
                        pre_ret_bits, pre_expr_bits, hir_ret_bits, hir_expr_bits
                    ),
                ));
            }
            let cmp_bits = pre_expr_bits.max(hir_expr_bits);
            if pre_ret_signed == hir_ret_signed {
                let relation = if pre_expr_bits == hir_expr_bits {
                    ReturnObservationRelation::ExactScalar {
                        bits: cmp_bits,
                        signed: pre_ret_signed,
                    }
                } else {
                    assumptions.push(ValidationAssumption {
                        kind: AssumptionKind::CallingConvention,
                        description: format!(
                            "language-value return comparison {}-extends PreHIR ({}-bit) and HIR \
                             ({}-bit) to common {}-bit domain",
                            if pre_ret_signed { "sign" } else { "zero" },
                            pre_expr_bits,
                            hir_expr_bits,
                            cmp_bits
                        ),
                    });
                    ReturnObservationRelation::LanguageValueWidened {
                        prehir_bits: pre_expr_bits,
                        hir_bits: hir_expr_bits,
                        cmp_bits,
                        signed: pre_ret_signed,
                    }
                };
                let prehir_cmp =
                    lower_sym::resize(prehir_result, pre_expr_bits, cmp_bits, pre_ret_signed);
                let hir_cmp =
                    lower_sym::resize(hir_result, hir_expr_bits, cmp_bits, hir_ret_signed);
                Ok((
                    relation,
                    cmp_bits,
                    PreparedInequality::DirectPair {
                        prehir_cmp,
                        hir_cmp,
                    },
                ))
            } else {
                assumptions.push(ValidationAssumption {
                    kind: AssumptionKind::CallingConvention,
                    description: format!(
                        "language-value return comparison relates PreHIR ({}-bit {}) and HIR \
                         ({}-bit {}) over exact signed/unsigned integer values",
                        pre_expr_bits,
                        signed_label(pre_ret_signed),
                        hir_expr_bits,
                        signed_label(hir_ret_signed)
                    ),
                });
                let (signed_expr, signed_bits, unsigned_expr, unsigned_bits) = if pre_ret_signed {
                    (
                        prehir_result.clone(),
                        pre_expr_bits,
                        hir_result.clone(),
                        hir_expr_bits,
                    )
                } else {
                    (
                        hir_result.clone(),
                        hir_expr_bits,
                        prehir_result.clone(),
                        pre_expr_bits,
                    )
                };
                let sign_bit = SymExpr::Extract {
                    expr: Box::new(signed_expr.clone()),
                    lsb: signed_bits - 1,
                    size: 1,
                };
                let signed_is_neg = SymExpr::new_eq(sign_bit, SymExpr::Const { val: 1, size: 1 });
                let signed_zext = lower_sym::resize(signed_expr, signed_bits, cmp_bits, false);
                let unsigned_zext =
                    lower_sym::resize(unsigned_expr, unsigned_bits, cmp_bits, false);
                let vals_differ = SymExpr::new_neq(signed_zext, unsigned_zext);
                let predicate = SymExpr::Ite {
                    cond: Box::new(signed_is_neg),
                    t: Box::new(SymExpr::Const { val: 1, size: 1 }),
                    f: Box::new(vals_differ),
                };
                Ok((
                    ReturnObservationRelation::LanguageValueSignednessChecked {
                        prehir_bits: pre_expr_bits,
                        prehir_signed: pre_ret_signed,
                        hir_bits: hir_expr_bits,
                        hir_signed: hir_ret_signed,
                        cmp_bits,
                    },
                    cmp_bits,
                    PreparedInequality::SignedUnsignedPair {
                        prehir_expr: prehir_result,
                        hir_expr: hir_result,
                        predicate,
                    },
                ))
            }
        }
    }
}

fn signed_label(signed: bool) -> &'static str {
    if signed { "signed" } else { "unsigned" }
}

/// Check whether `prehir` and `hir` compute the same return value for every
/// input, for the symbolic tier's modeled subset (see [`crate::lower_sym`]'s
/// scope notes -- acyclic, `Bool`/`Int` only, no `Div`/`Mod`/`Sar`).
pub fn check_symbolic_equivalence(prehir: &PreHirFunction, hir: &HirFunction) -> SymbolicOutcome {
    check_symbolic_equivalence_report(prehir, hir).outcome
}

/// Check symbolic equivalence under the default [`ReturnComparisonMode::LanguageValue`]
/// and return the structured correspondence and assurance report.
#[must_use]
pub fn check_symbolic_equivalence_report(
    prehir: &PreHirFunction,
    hir: &HirFunction,
) -> SymbolicEquivalenceReport {
    check_symbolic_equivalence_with_mode(prehir, hir, ReturnComparisonMode::LanguageValue)
}

/// Check symbolic equivalence under an explicit [`ReturnComparisonMode`],
/// returning checked input correspondence, assumptions, and assurance evidence.
#[must_use]
pub fn check_symbolic_equivalence_with_mode(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    return_mode: ReturnComparisonMode,
) -> SymbolicEquivalenceReport {
    let query = match prepare_symbolic_query(prehir, hir, return_mode) {
        Ok(query) => query,
        Err(report) => return *report,
    };

    let mut solver = Solver::new();
    let param_node_ids: Vec<_> = query
        .param_specs
        .iter()
        .map(|spec| {
            // Registers the Var's node so `get_value` can read a counterexample
            // model back for it after a `Sat` result.
            solver.register_node(spec.var.clone())
        })
        .collect();

    match &query.inequality {
        PreparedInequality::DirectPair {
            prehir_cmp,
            hir_cmp,
            ..
        } => {
            solver.assert(SymExpr::new_neq(prehir_cmp.clone(), hir_cmp.clone()));
        }
        PreparedInequality::SignedUnsignedPair { predicate, .. } => {
            solver.assert(SymExpr::new_eq(
                predicate.clone(),
                SymExpr::Const { val: 1, size: 1 },
            ));
        }
    }

    let result = match solver.check_sat() {
        Ok(result) => result,
        Err(err) => {
            tracing::debug!("lower_sym: solver error: {err}");
            return unsupported_report(
                Some(query.correspondence),
                query.assumptions,
                UnsupportedReason::Construct("solver query failed"),
                format!("solver query failed: {err}"),
            );
        }
    };

    match classify_solver_result(result) {
        SolverVerdict::Equivalent => {
            if let Some(reason) = incompatible_input_domain_reason(&query.correspondence) {
                return inconclusive_report(Some(query.correspondence), query.assumptions, reason);
            }
            let strength = if query.assumptions.is_empty() {
                EvidenceStrength::Universal
            } else {
                EvidenceStrength::Conditional
            };
            let detail = format!(
                "solver proved PreHIR and HIR return values equivalent across {} checked \
                 parameter(s) under {:?} ({:?})",
                query.correspondence.params.len(),
                query.correspondence.return_relation,
                strength
            );
            SymbolicEquivalenceReport {
                outcome: SymbolicOutcome::Equivalent,
                correspondence: Some(query.correspondence),
                rejection_reason: None,
                scope: symbolic_ast_return_scope(),
                verdict: ValidationVerdict::Equivalent,
                evidence: ValidationEvidence {
                    strength,
                    budget: ValidationBudget::default(),
                    explored_paths: 0,
                    executed_traces: 0,
                    assumptions: query.assumptions,
                    unresolved_effects: Vec::new(),
                    counterexample: None,
                    detail,
                },
            }
        }
        SolverVerdict::Counterexample => {
            validate_solver_witness_report(prehir, hir, &query, &param_node_ids, |id| {
                solver.get_value(id)
            })
        }
        SolverVerdict::Unknown => unsupported_report(
            Some(query.correspondence),
            query.assumptions,
            UnsupportedReason::Construct(
                "solver returned Unknown -- should be rare/never with no memory theory engaged",
            ),
            "solver returned Unknown",
        ),
    }
}

fn incompatible_input_domain_reason(corr: &SymbolicInputCorrespondence) -> Option<String> {
    let mismatched: Vec<String> = corr
        .params
        .iter()
        .filter(|p| !p.width_matches || !p.signedness_matches)
        .map(|p| {
            format!(
                "param {} ('{}' {}:{}b vs '{}' {}:{:}b)",
                p.index,
                p.prehir_name,
                signed_label(matches!(p.prehir_type, NirType::Int { signed: true, .. })),
                p.prehir_bits,
                p.hir_name,
                signed_label(matches!(p.hir_type, NirType::Int { signed: true, .. })),
                p.hir_bits
            )
        })
        .collect();
    if mismatched.is_empty() {
        None
    } else {
        Some(format!(
            "solver returned UNSAT on asymmetric/incompatible parameter domain(s) [{}]; \
             refusing to promote lossy narrowing or unsigned/signed conflation to equivalence",
            mismatched.join(", ")
        ))
    }
}

fn validate_solver_witness_report<F>(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    query: &PreparedSymbolicQuery,
    param_node_ids: &[SymNodeId],
    mut model_value: F,
) -> SymbolicEquivalenceReport
where
    F: FnMut(SymNodeId) -> Option<u64>,
{
    let mut referenced_vars = HashSet::new();
    let mut collect = |expr: &SymExpr| {
        if let SymExpr::Var { id, .. } = expr {
            referenced_vars.insert(*id);
        }
    };
    query.prehir_expr.visit_preorder(&mut collect);
    query.hir_expr.visit_preorder(&mut collect);

    let mut args = Vec::with_capacity(query.param_specs.len());
    let mut arg_widths = Vec::with_capacity(query.param_specs.len());
    let mut env = HashMap::with_capacity(query.param_specs.len());

    for (spec, &node_id) in query.param_specs.iter().zip(param_node_ids) {
        let val = match model_value(node_id) {
            Some(raw) => {
                if !fits_bits(raw, spec.width_bits) {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        format!(
                            "solver returned out-of-width model value 0x{raw:x} for {}-bit \
                             parameter '{}'",
                            spec.width_bits, spec.name
                        ),
                    );
                }
                raw
            }
            None => {
                if referenced_vars.contains(&spec.var_id) {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        format!(
                            "solver returned incomplete model: missing value for constrained \
                             parameter '{}'",
                            spec.name
                        ),
                    );
                }
                // Parameter does not appear in either lowered return expression:
                // explicitly complete the free input with a width-valid zero.
                0
            }
        };
        args.push(val);
        arg_widths.push(spec.width_bits);
        env.insert(spec.var_id, val);
    }

    // 1. Confirm the candidate tuple actually satisfies the configured return
    //    inequality on the lowered bitvector formulas.
    let (pre_sym_obs, hir_sym_obs, sym_diverges) = match &query.inequality {
        PreparedInequality::DirectPair {
            prehir_cmp,
            hir_cmp,
            ..
        } => {
            let pre_sym = match eval_sym_bv(prehir_cmp, &env) {
                Ok(v) => v,
                Err(reason) => {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        reason,
                    );
                }
            };
            let hir_sym = match eval_sym_bv(hir_cmp, &env) {
                Ok(v) => v,
                Err(reason) => {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        reason,
                    );
                }
            };
            (
                format!("0x{pre_sym:x}"),
                format!("0x{hir_sym:x}"),
                pre_sym != hir_sym,
            )
        }
        PreparedInequality::SignedUnsignedPair {
            prehir_expr,
            hir_expr,
            predicate,
        } => {
            let pred_val = match eval_sym_bv(predicate, &env) {
                Ok(v) => v,
                Err(reason) => {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        reason,
                    );
                }
            };
            let pre_sym = match eval_sym_bv(prehir_expr, &env) {
                Ok(v) => v,
                Err(reason) => {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        reason,
                    );
                }
            };
            let hir_sym = match eval_sym_bv(hir_expr, &env) {
                Ok(v) => v,
                Err(reason) => {
                    return inconclusive_report(
                        Some(query.correspondence.clone()),
                        query.assumptions.clone(),
                        reason,
                    );
                }
            };
            let pre_val = as_math_int(
                pre_sym,
                query.correspondence.prehir_return_bits,
                matches!(
                    query.correspondence.prehir_return_type,
                    NirType::Int { signed: true, .. }
                ),
            );
            let hir_val = as_math_int(
                hir_sym,
                query.correspondence.hir_return_bits,
                matches!(
                    query.correspondence.hir_return_type,
                    NirType::Int { signed: true, .. }
                ),
            );
            (
                pre_val.to_string(),
                hir_val.to_string(),
                pred_val != 0 && pre_val != hir_val,
            )
        }
    };

    if !sym_diverges {
        return inconclusive_report(
            Some(query.correspondence.clone()),
            query.assumptions.clone(),
            format!(
                "solver counterexample {:?} does not satisfy PreHIR != HIR (PreHIR={}, HIR={})",
                args, pre_sym_obs, hir_sym_obs
            ),
        );
    }

    // 2. Independently replay the concrete tuple against the PreHIR/HIR
    //    interpreter under the same parameter and return comparison relation.
    let eval_args: Vec<i64> = args.iter().map(|&v| v as i64).collect();
    let pre_replay = interpret_prehir(&prehir.body, &prehir.params, &prehir.locals, &eval_args);
    let hir_replay = interpret_hir(&hir.body, &hir.params, &hir.locals, &eval_args);

    match (pre_replay, hir_replay) {
        (Ok(Some(pre_ret)), Ok(Some(hir_ret))) => {
            let (pre_obs, hir_obs, replay_diverges) = compare_concrete_returns(
                pre_ret as u64,
                hir_ret as u64,
                query.correspondence.return_relation,
            );
            if !replay_diverges {
                return inconclusive_report(
                    Some(query.correspondence.clone()),
                    query.assumptions.clone(),
                    format!(
                        "concrete PreHIR/HIR replay agreed on candidate {:?} \
                         (PreHIR={pre_obs}, HIR={hir_obs})",
                        args
                    ),
                );
            }
            let inputs: Vec<(String, u64)> = query
                .param_specs
                .iter()
                .zip(&args)
                .map(|(spec, &v)| (spec.name.clone(), v))
                .collect();
            let detail = format!(
                "confirmed symbolic counterexample on {:?}: PreHIR={pre_obs}, HIR={hir_obs}",
                inputs
            );
            SymbolicEquivalenceReport {
                outcome: SymbolicOutcome::Diverged(Counterexample { args, arg_widths }),
                correspondence: Some(query.correspondence.clone()),
                rejection_reason: None,
                scope: symbolic_ast_return_scope(),
                verdict: ValidationVerdict::Counterexample,
                evidence: ValidationEvidence {
                    strength: EvidenceStrength::None,
                    budget: ValidationBudget::default(),
                    explored_paths: 0,
                    executed_traces: 0,
                    assumptions: query.assumptions.clone(),
                    unresolved_effects: Vec::new(),
                    counterexample: Some(DirCounterexample {
                        inputs,
                        foundation_observation: pre_obs,
                        candidate_observation: hir_obs,
                    }),
                    detail,
                },
            }
        }
        (Ok(None), _) | (_, Ok(None)) => inconclusive_report(
            Some(query.correspondence.clone()),
            query.assumptions.clone(),
            "concrete replay returned without a value on solver candidate",
        ),
        (Err(err), _) | (_, Err(err)) => inconclusive_report(
            Some(query.correspondence.clone()),
            query.assumptions.clone(),
            format!("concrete replay failed on solver candidate: {err}"),
        ),
    }
}

fn compare_concrete_returns(
    pre_raw: u64,
    hir_raw: u64,
    relation: ReturnObservationRelation,
) -> (String, String, bool) {
    match relation {
        ReturnObservationRelation::ExactScalar { bits, .. }
        | ReturnObservationRelation::RawBitPattern { bits, .. } => {
            let p = pre_raw & mask_bv(bits);
            let h = hir_raw & mask_bv(bits);
            (format!("0x{p:x}"), format!("0x{h:x}"), p != h)
        }
        ReturnObservationRelation::LanguageValueWidened {
            prehir_bits,
            hir_bits,
            cmp_bits,
            signed,
        } => {
            let p = widen_bv(pre_raw, prehir_bits, cmp_bits, signed);
            let h = widen_bv(hir_raw, hir_bits, cmp_bits, signed);
            (format!("0x{p:x}"), format!("0x{h:x}"), p != h)
        }
        ReturnObservationRelation::LanguageValueSignednessChecked {
            prehir_bits,
            prehir_signed,
            hir_bits,
            hir_signed,
            ..
        } => {
            let p = as_math_int(pre_raw, prehir_bits, prehir_signed);
            let h = as_math_int(hir_raw, hir_bits, hir_signed);
            (p.to_string(), h.to_string(), p != h)
        }
    }
}

fn mask_bv(bits: u32) -> u64 {
    match bits {
        0 => 0,
        1..=63 => (1u64 << bits) - 1,
        _ => u64::MAX,
    }
}

fn fits_bits(val: u64, width_bits: u32) -> bool {
    width_bits >= 64 || (val >> width_bits) == 0
}

fn sext_i128(val: u64, bits: u32) -> i128 {
    match bits {
        0 => 0,
        1..=63 => {
            let masked = val & mask_bv(bits);
            let sign_bit = 1u64 << (bits - 1);
            if masked & sign_bit == 0 {
                i128::from(masked)
            } else {
                i128::from(masked) - (1i128 << bits)
            }
        }
        _ => i128::from(val as i64),
    }
}

fn as_math_int(raw: u64, bits: u32, signed: bool) -> i128 {
    if signed {
        sext_i128(raw, bits)
    } else {
        i128::from(raw & mask_bv(bits))
    }
}

fn widen_bv(raw: u64, from_bits: u32, to_bits: u32, signed: bool) -> u64 {
    let narrow = raw & mask_bv(from_bits);
    let widened = if signed && from_bits > 0 && from_bits < to_bits {
        sext_i128(narrow, from_bits) as u64
    } else {
        narrow
    };
    widened & mask_bv(to_bits)
}

fn eval_sym_bv(expr: &SymExpr, env: &HashMap<SymNodeId, u64>) -> Result<u64, String> {
    let width = expr.get_bit_width();
    if !(1..=64).contains(&width) {
        return Err(format!(
            "unsupported {width}-bit expression during witness evaluation"
        ));
    }
    let mask = mask_bv(width);
    let value = match expr {
        SymExpr::Const { val, size } => val & mask_bv(*size),
        SymExpr::Var {
            id,
            name,
            sort: Sort::BitVector(bits),
        } => {
            let val = env.get(id).copied().ok_or_else(|| {
                format!("witness evaluation missing parameter '{name}' (id={id})")
            })?;
            val & mask_bv(*bits)
        }
        SymExpr::Add(a, b) => eval_sym_bv(a, env)?.wrapping_add(eval_sym_bv(b, env)?),
        SymExpr::Sub(a, b) => eval_sym_bv(a, env)?.wrapping_sub(eval_sym_bv(b, env)?),
        SymExpr::Mul(a, b) => eval_sym_bv(a, env)?.wrapping_mul(eval_sym_bv(b, env)?),
        SymExpr::Udiv(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r == 0 { mask } else { l / r }
        }
        SymExpr::Urem(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r == 0 { l } else { l % r }
        }
        SymExpr::And(a, b) => eval_sym_bv(a, env)? & eval_sym_bv(b, env)?,
        SymExpr::Or(a, b) => eval_sym_bv(a, env)? | eval_sym_bv(b, env)?,
        SymExpr::Xor(a, b) => eval_sym_bv(a, env)? ^ eval_sym_bv(b, env)?,
        SymExpr::Shl(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                0
            } else {
                l.wrapping_shl(r as u32)
            }
        }
        SymExpr::Lshr(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                0
            } else {
                l >> (r as u32)
            }
        }
        SymExpr::Ashr(a, b) => {
            let l = sext_i128(eval_sym_bv(a, env)?, width);
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                if l < 0 { mask } else { 0 }
            } else {
                (l >> (r as u32)) as u64
            }
        }
        SymExpr::Eq(a, b) => u64::from(eval_sym_bv(a, env)? == eval_sym_bv(b, env)?),
        SymExpr::Neq(a, b) => u64::from(eval_sym_bv(a, env)? != eval_sym_bv(b, env)?),
        SymExpr::Ult(a, b) => u64::from(eval_sym_bv(a, env)? < eval_sym_bv(b, env)?),
        SymExpr::Ule(a, b) => u64::from(eval_sym_bv(a, env)? <= eval_sym_bv(b, env)?),
        SymExpr::Slt(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) < sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Sle(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) <= sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Sgt(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) > sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Ite { cond, t, f } => {
            if eval_sym_bv(cond, env)? != 0 {
                eval_sym_bv(t, env)?
            } else {
                eval_sym_bv(f, env)?
            }
        }
        SymExpr::Extract { expr, lsb, size } => {
            let inner = eval_sym_bv(expr, env)?;
            if *lsb >= 64 {
                0
            } else {
                (inner >> *lsb) & mask_bv(*size)
            }
        }
        SymExpr::Concat(high, low) => {
            let low_w = low.get_bit_width();
            let hi_v = eval_sym_bv(high, env)?;
            let lo_v = eval_sym_bv(low, env)?;
            if low_w >= 64 {
                lo_v
            } else {
                (hi_v << low_w) | lo_v
            }
        }
        other => {
            return Err(format!(
                "unsupported symbolic operator during witness verification: {other:?}"
            ));
        }
    };
    Ok(value & mask)
}

fn checked_sym_bits(
    e: &SymExpr,
    assumptions: Vec<ValidationAssumption>,
) -> Result<u32, Box<SymbolicEquivalenceReport>> {
    match e.get_sort() {
        Sort::BitVector(bits) if (1..=64).contains(&bits) => Ok(bits),
        Sort::BitVector(bits) => Err(unsupported_err(
            None,
            assumptions,
            UnsupportedReason::UnsupportedType("symbolic tier supports 1..=64-bit integers only"),
            format!("lowered expression has unsupported {bits}-bit width"),
        )),
        other => Err(unsupported_err(
            None,
            assumptions,
            UnsupportedReason::UnsupportedType(
                "symbolic tier requires bitvector sorts on proof paths",
            ),
            format!("lowered expression has non-bitvector sort {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{SolverVerdict, classify_solver_result};

    fn int(bits: u32, signed: bool) -> fission_midend_core::ir::NirType {
        fission_midend_core::ir::NirType::Int { bits, signed }
    }

    fn prehir_returning(value: i64, bits: u32) -> fission_midend_prehir::PreHirFunction {
        use fission_midend_prehir::{PreHirExpr, PreHirStmt};
        fission_midend_prehir::PreHirFunction {
            return_type: int(bits, true),
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                value,
                int(bits, true),
            )))],
            ..Default::default()
        }
    }

    fn hir_returning(value: i64, bits: u32) -> fission_midend_core::ir::HirFunction {
        use fission_midend_core::ir::{HirExpr, HirStmt};
        fission_midend_core::ir::HirFunction {
            return_type: int(bits, true),
            body: vec![HirStmt::Return(Some(HirExpr::Const(
                value,
                int(bits, true),
            )))],
            ..Default::default()
        }
    }

    fn name(outcome: &super::SymbolicOutcome) -> &'static str {
        match outcome {
            super::SymbolicOutcome::Equivalent => "Equivalent",
            super::SymbolicOutcome::Diverged(_) => "Diverged",
            super::SymbolicOutcome::Unsupported(_) => "Unsupported",
            super::SymbolicOutcome::Inconclusive { .. } => "Inconclusive",
        }
    }

    /// A signed `-1` returned at eight bits and at thirty-two is the same
    /// value. Zero-extending the narrow side made it `255` and reported a
    /// divergence that is not one.
    #[test]
    fn a_signed_return_is_widened_by_sign_not_by_zero() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        let verdict = check_symbolic_equivalence(&prehir_returning(-1, 8), &hir_returning(-1, 32));
        assert!(
            matches!(verdict, SymbolicOutcome::Equivalent),
            "got {}",
            name(&verdict)
        );
        // The check still has teeth: a genuinely different value diverges.
        let different =
            check_symbolic_equivalence(&prehir_returning(-1, 8), &hir_returning(255, 32));
        assert!(
            matches!(different, SymbolicOutcome::Diverged(_)),
            "got {}",
            name(&different)
        );
    }
    use fission_solver::ast::SymExpr;
    use fission_solver::{SatResult, Solver};

    #[test]
    fn unknown_from_an_unencoded_operation_cannot_prove_dir_equivalence() {
        let a = SymExpr::new_float_var("a", 32);
        let b = SymExpr::new_float_var("b", 32);
        let unsupported = SymExpr::FAdd(Box::new(a), Box::new(b));
        let mut solver = Solver::new();
        solver.assert(SymExpr::Neq(
            Box::new(unsupported),
            Box::new(SymExpr::new_const(0, 32)),
        ));

        let result = solver.check_sat().expect("solver result");
        assert_eq!(result, SatResult::Unknown);
        assert_eq!(classify_solver_result(result), SolverVerdict::Unknown);
        assert_ne!(
            classify_solver_result(SatResult::Unknown),
            SolverVerdict::Equivalent
        );
    }

    #[test]
    fn lower_sym_rejects_incomplete_return_paths_and_uninitialized_local_reads() {
        use crate::lower_sym;
        use fission_midend_core::ir::NirType;
        use fission_midend_prehir::{PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirStmt};

        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let params = vec![PreHirBinding {
            name: "x".to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }];
        let param_vars = vec![SymExpr::new_var("x", 32)];

        // 1. Single guarded return with fallthrough must be rejected rather than
        // discarding `(x > 0)` and claiming constant return 42 on all inputs.
        let incomplete_body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SGt,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, i32_ty.clone())),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                42,
                i32_ty.clone(),
            )))]
            .into(),
            else_body: Vec::new().into(),
        }];
        let err = lower_sym::prehir::lower(&incomplete_body, &params, &[], &param_vars)
            .expect_err("incomplete return path should be rejected");
        assert!(err.to_string().contains("incomplete return"));

        // 2. Reading an uninitialized local must fail rather than silently seeding to 0.
        let locals = vec![PreHirBinding {
            name: "uninit".to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }];
        let uninit_body = vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "uninit".to_string(),
        )))];
        let err = lower_sym::prehir::lower(&uninit_body, &params, &locals, &param_vars)
            .expect_err("uninitialized local read should be rejected");
        assert!(err.to_string().contains("undeclared variable 'uninit'"));
    }

    #[test]
    fn signed_var_widening_cast_sign_extends_in_symbolic_equivalence() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let i64_ty = int(64, true);

        let prehir = PreHirFunction {
            return_type: i64_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: i64_ty.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }))],
            ..Default::default()
        };
        let hir = HirFunction {
            return_type: i64_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::If {
                cond: HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::SLt,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(0, i32_ty)),
                    ty: fission_midend_core::ir::NirType::Bool,
                },
                then_body: vec![HirStmt::Return(Some(HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::Or,
                    lhs: Box::new(HirExpr::Cast {
                        ty: i64_ty.clone(),
                        expr: Box::new(HirExpr::Var("x".to_string())),
                    }),
                    rhs: Box::new(HirExpr::Const(-0x1_0000_0000_i64, i64_ty.clone())),
                    ty: i64_ty.clone(),
                }))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Cast {
                    ty: i64_ty.clone(),
                    expr: Box::new(HirExpr::Var("x".to_string())),
                }))],
            }],
            ..Default::default()
        };

        let verdict = check_symbolic_equivalence(&prehir, &hir);
        assert!(
            matches!(verdict, SymbolicOutcome::Equivalent),
            "signed var widening cast should sign-extend, got {}",
            name(&verdict)
        );
    }

    #[test]
    fn symbolic_counterexample_returns_registered_model_values() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i32_ty = int(32, true);
        let prehir = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(7, i32_ty.clone())),
                        ty: fission_midend_core::ir::NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty)))],
            ..Default::default()
        };

        let SymbolicOutcome::Diverged(ce) = check_symbolic_equivalence(&prehir, &hir) else {
            panic!("expected counterexample");
        };
        assert_eq!(ce.args, vec![7]);
        assert_eq!(ce.arg_widths, vec![32]);
    }

    #[test]
    fn constrained_missing_and_invalid_models_become_inconclusive_while_free_params_zero_complete()
    {
        use super::{
            ReturnComparisonMode, SymbolicOutcome, prepare_symbolic_query,
            validate_solver_witness_report,
        };
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i32_ty = int(32, true);
        let u8_ty = fission_midend_core::ir::NirType::Int {
            bits: 8,
            signed: false,
        };
        let pre_param = |name: &str, ty: &fission_midend_core::ir::NirType| PreHirBinding {
            name: name.to_string(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        let hir_param = |name: &str, ty: &fission_midend_core::ir::NirType| NirBinding {
            name: name.to_string(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };

        // Two-parameter pair: `x` (u8, constrained at x == 7 or x == 0) and `unused` (i32, free).
        let eq_const_pair = |target: i64| {
            let prehir = PreHirFunction {
                return_type: i32_ty.clone(),
                params: vec![pre_param("x", &u8_ty), pre_param("unused", &i32_ty)],
                body: vec![
                    PreHirStmt::If {
                        cond: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Eq,
                            lhs: Box::new(PreHirExpr::Var("x".to_string())),
                            rhs: Box::new(PreHirExpr::Const(target, u8_ty.clone())),
                            ty: fission_midend_core::ir::NirType::Bool,
                        },
                        then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                            1,
                            i32_ty.clone(),
                        )))]
                        .into(),
                        else_body: Vec::new().into(),
                    },
                    PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
                ],
                ..Default::default()
            };
            let hir = HirFunction {
                return_type: i32_ty.clone(),
                params: vec![hir_param("x", &u8_ty), hir_param("unused", &i32_ty)],
                body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
                ..Default::default()
            };
            (prehir, hir)
        };

        // 1 & 2: Missing constrained model value (`x` -> None) MUST yield Inconclusive,
        // even when `x == 0` would coincidentally satisfy the inequality.
        for target in [7_i64, 0_i64] {
            let (prehir, hir) = eq_const_pair(target);
            let query =
                match prepare_symbolic_query(&prehir, &hir, ReturnComparisonMode::LanguageValue) {
                    Ok(q) => q,
                    Err(err) => panic!("query lowers, got {err:?}"),
                };
            let node_ids = [101_u32, 102_u32];
            let outcome =
                validate_solver_witness_report(&prehir, &hir, &query, &node_ids, |id| match id {
                    101 => None,
                    102 => Some(0),
                    _ => None,
                })
                .outcome;
            assert!(
                matches!(outcome, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("missing value for constrained parameter 'x'")),
                "target={target}: expected Inconclusive for missing constrained param, got {outcome:?}"
            );
        }

        // 3: Missing value for genuinely free/unconstrained `unused` completes explicitly to 0,
        // while valid `x = 7` replays as Diverged with exact parameter widths `[8, 32]`.
        let (prehir7, hir7) = eq_const_pair(7);
        let query7 =
            match prepare_symbolic_query(&prehir7, &hir7, ReturnComparisonMode::LanguageValue) {
                Ok(q) => q,
                Err(err) => panic!("query lowers, got {err:?}"),
            };
        let node_ids = [201_u32, 202_u32];
        let outcome =
            validate_solver_witness_report(&prehir7, &hir7, &query7, &node_ids, |id| match id {
                201 => Some(7),
                202 => None,
                _ => None,
            })
            .outcome;
        assert_eq!(
            outcome,
            SymbolicOutcome::Diverged(super::Counterexample {
                args: vec![7, 0],
                arg_widths: vec![8, 32],
            })
        );

        // 4: Out-of-width model value for 8-bit `x` (`0x1ff`) yields Inconclusive.
        let out_of_width =
            validate_solver_witness_report(&prehir7, &hir7, &query7, &node_ids, |id| match id {
                201 => Some(0x1ff),
                202 => Some(0),
                _ => None,
            })
            .outcome;
        assert!(
            matches!(out_of_width, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("out-of-width")),
            "expected out-of-width Inconclusive, got {out_of_width:?}"
        );

        // 5: Non-replaying model (`x = 0` when divergence is at `x == 7`) yields Inconclusive.
        let spurious =
            validate_solver_witness_report(&prehir7, &hir7, &query7, &node_ids, |id| match id {
                201 => Some(0),
                202 => Some(0),
                _ => None,
            })
            .outcome;
        assert!(
            matches!(spurious, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("does not satisfy")),
            "expected non-replaying Inconclusive, got {spurious:?}"
        );
    }

    #[test]
    fn witness_validation_preserves_width_sign_boundaries_and_rejects_unsupported_int_widths() {
        use super::{Counterexample, SymbolicOutcome, check_symbolic_equivalence};
        use crate::report::UnsupportedReason;
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding, NirType};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i8_ty = int(8, true);
        let i32_ty = int(32, true);
        let u32_ty = int(32, false);
        let u8_ty = int(8, false);
        let u64_ty = int(64, false);

        // 1. i8 signed boundary (-1 -> 0xff) preserves 8-bit width.
        let pre_i8 = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1, i8_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_i8 = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i8_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_i8, &hir_i8),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![0xff],
                arg_widths: vec![8],
            })
        );

        // 2. u64 full-width boundary (`u64::MAX`) preserves 64-bit width.
        let pre_u64 = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: u64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1_i64, u64_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_u64 = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: u64_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_u64, &hir_u64),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![u64::MAX],
                arg_widths: vec![64],
            })
        );

        // 3. Asymmetric parameter widths (PreHIR u32 vs HIR u8): divergence at x == 300
        //    must still be discovered and reported at width 32 rather than clamped to 8.
        let pre_wide = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(300, u32_ty)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_narrow = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: u8_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty)))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_wide, &hir_narrow),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![300],
                arg_widths: vec![32],
            })
        );

        // 4. Unsupported >64-bit or 0-bit integer parameters are explicitly rejected.
        for bad_bits in [0_u32, 128_u32] {
            let bad_ty = int(bad_bits, false);
            let pre_bad = PreHirFunction {
                return_type: bad_ty.clone(),
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    0,
                    bad_ty.clone(),
                )))],
                ..Default::default()
            };
            let hir_bad = HirFunction {
                return_type: bad_ty.clone(),
                body: vec![HirStmt::Return(Some(HirExpr::Const(0, bad_ty)))],
                ..Default::default()
            };
            assert!(
                matches!(
                    check_symbolic_equivalence(&pre_bad, &hir_bad),
                    SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
                ),
                "expected UnsupportedType for {bad_bits}-bit integer"
            );
        }
    }

    fn pre_bind(
        name: &str,
        ty: fission_midend_core::ir::NirType,
        origin: Option<fission_midend_core::ir::NirBindingOrigin>,
    ) -> fission_midend_prehir::PreHirBinding {
        fission_midend_prehir::PreHirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    fn hir_bind(
        name: &str,
        ty: fission_midend_core::ir::NirType,
        origin: Option<fission_midend_core::ir::NirBindingOrigin>,
    ) -> fission_midend_core::ir::NirBinding {
        fission_midend_core::ir::NirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    #[test]
    fn rejects_swapped_param_origins_mismatched_storage_and_abi_conflicts() {
        use super::{
            SymbolicOutcome, check_symbolic_equivalence_report, symbolic_ast_return_scope,
        };
        use crate::product::ValidationVerdict;
        use fission_midend_core::CallingConvention;
        use fission_midend_core::ir::{
            HirBinaryOp, HirExpr, HirFunction, HirStmt, NirBindingOrigin,
        };
        use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let base_pre_add = |p0_orig, p1_orig| PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![
                pre_bind("a", i32_ty.clone(), p0_orig),
                pre_bind("b", i32_ty.clone(), p1_orig),
            ],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("a".to_string())),
                rhs: Box::new(PreHirExpr::Var("b".to_string())),
                ty: i32_ty.clone(),
            }))],
            ..Default::default()
        };
        let base_hir_add = |p0_orig, p1_orig| HirFunction {
            return_type: i32_ty.clone(),
            params: vec![
                hir_bind("a", i32_ty.clone(), p0_orig),
                hir_bind("b", i32_ty.clone(), p1_orig),
            ],
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("a".to_string())),
                rhs: Box::new(HirExpr::Var("b".to_string())),
                ty: i32_ty.clone(),
            }))],
            ..Default::default()
        };

        // 1. Even on commutative `a + b`, swapped `ParamIndex` origins MUST NOT prove equivalent.
        let swapped = check_symbolic_equivalence_report(
            &base_pre_add(
                Some(NirBindingOrigin::ParamIndex(0)),
                Some(NirBindingOrigin::ParamIndex(1)),
            ),
            &base_hir_add(
                Some(NirBindingOrigin::ParamIndex(1)),
                Some(NirBindingOrigin::ParamIndex(0)),
            ),
        );
        assert!(matches!(
            swapped.outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));
        assert_eq!(swapped.verdict, ValidationVerdict::Inconclusive);
        assert!(!swapped.is_proven());
        assert!(swapped.correspondence.is_none());
        assert_eq!(swapped.scope, symbolic_ast_return_scope());

        // 2. Stack offset mismatch, asymmetric provenance, and invalid param origins are rejected.
        for (pre_orig, hir_orig) in [
            (
                Some(NirBindingOrigin::StackOffset(8)),
                Some(NirBindingOrigin::StackOffset(16)),
            ),
            (Some(NirBindingOrigin::ParamIndex(0)), None),
            (Some(NirBindingOrigin::Temp), Some(NirBindingOrigin::Temp)),
            (
                Some(NirBindingOrigin::ReturnScaffold),
                Some(NirBindingOrigin::ReturnScaffold),
            ),
        ] {
            let report = check_symbolic_equivalence_report(
                &base_pre_add(pre_orig, Some(NirBindingOrigin::ParamIndex(1))),
                &base_hir_add(hir_orig, Some(NirBindingOrigin::ParamIndex(1))),
            );
            assert!(
                matches!(report.outcome, SymbolicOutcome::Inconclusive { .. }),
                "expected Inconclusive for ({pre_orig:?}, {hir_orig:?}), got {:?}",
                report.outcome
            );
            assert!(!report.is_proven());
        }

        // 3. Conflicting implicit `param_N` hints without explicit origins are rejected.
        let mut pre_hint = base_pre_add(None, None);
        let mut hir_hint = base_hir_add(None, None);
        pre_hint.params[0].name = "param_1".to_string();
        pre_hint.params[1].name = "param_2".to_string();
        pre_hint.body = vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "param_1".to_string(),
        )))];
        hir_hint.params[0].name = "param_2".to_string();
        hir_hint.params[1].name = "param_1".to_string();
        hir_hint.body = vec![HirStmt::Return(Some(HirExpr::Var("param_2".to_string())))];
        let hint_report = check_symbolic_equivalence_report(&pre_hint, &hir_hint);
        assert!(matches!(
            hint_report.outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 4. Different parameter counts are rejected as Inconclusive.
        let mut one_param = base_hir_add(Some(NirBindingOrigin::ParamIndex(0)), None);
        one_param.params.pop();
        one_param.body = vec![HirStmt::Return(Some(HirExpr::Var("a".to_string())))];
        let arity_report = check_symbolic_equivalence_report(
            &base_pre_add(
                Some(NirBindingOrigin::ParamIndex(0)),
                Some(NirBindingOrigin::ParamIndex(1)),
            ),
            &one_param,
        );
        assert!(matches!(
            arity_report.outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 5. ABI mismatches (`calling_convention`, `is_64bit`, `is_big_endian`,
        //    `suppress_entry_register_params`, `has_unknown_arity_prototype`,
        //    `variadic_fixed_arity`, `int_param_offsets`) are rejected.
        let valid_pre = base_pre_add(
            Some(NirBindingOrigin::ParamIndex(0)),
            Some(NirBindingOrigin::ParamIndex(1)),
        );
        let base_hir = base_hir_add(
            Some(NirBindingOrigin::ParamIndex(0)),
            Some(NirBindingOrigin::ParamIndex(1)),
        );
        let mut cc_mismatch = base_hir.clone();
        cc_mismatch.calling_convention = CallingConvention::SystemVAmd64;
        let mut bitness_mismatch = base_hir.clone();
        bitness_mismatch.is_64bit = false;
        let mut endian_mismatch = base_hir.clone();
        endian_mismatch.is_big_endian = true;
        let mut unknown_arity = base_hir.clone();
        unknown_arity.has_unknown_arity_prototype = true;
        let mut variadic = base_hir.clone();
        variadic.variadic_fixed_arity = Some(1);
        let mut suppress_reg = base_hir.clone();
        suppress_reg.suppress_entry_register_params = true;
        let mut pre_offsets = valid_pre.clone();
        pre_offsets.int_param_offsets = vec![0x08, 0x10];
        let mut hir_offsets = base_hir.clone();
        hir_offsets.int_param_offsets = vec![0x38, 0x30];

        for (pre, hir, label) in [
            (&valid_pre, &cc_mismatch, "calling_convention"),
            (&valid_pre, &bitness_mismatch, "is_64bit"),
            (&valid_pre, &endian_mismatch, "is_big_endian"),
            (&valid_pre, &unknown_arity, "has_unknown_arity_prototype"),
            (&valid_pre, &variadic, "variadic_fixed_arity"),
            (&valid_pre, &suppress_reg, "suppress_entry_register_params"),
            (&pre_offsets, &hir_offsets, "int_param_offsets"),
        ] {
            let report = check_symbolic_equivalence_report(pre, hir);
            assert!(
                matches!(report.outcome, SymbolicOutcome::Inconclusive { .. }),
                "{label}: expected Inconclusive, got {:?}",
                report.outcome
            );
        }
    }

    #[test]
    fn rejects_duplicate_and_colliding_bindings_and_undeclared_live_ins() {
        use super::{SymbolicOutcome, check_symbolic_equivalence_report};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBindingOrigin};
        use fission_midend_prehir::{PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let o0 = Some(NirBindingOrigin::ParamIndex(0));
        let o1 = Some(NirBindingOrigin::ParamIndex(1));

        // 1. Duplicate parameter names.
        let pre_dup_param = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![
                pre_bind("x", i32_ty.clone(), o0),
                pre_bind("x", i32_ty.clone(), o1),
            ],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))],
            ..Default::default()
        };
        let hir_ok_two = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![
                hir_bind("x", i32_ty.clone(), o0),
                hir_bind("y", i32_ty.clone(), o1),
            ],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
            ..Default::default()
        };
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_dup_param, &hir_ok_two).outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 2. Duplicate parameter origin on same side.
        let mut pre_dup_orig = pre_dup_param.clone();
        pre_dup_orig.params[1].name = "y".to_string();
        pre_dup_orig.params[1].origin = o0;
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_dup_orig, &hir_ok_two).outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 3. Empty parameter name.
        let mut pre_empty = pre_dup_param.clone();
        pre_empty.params[0].name = String::new();
        pre_empty.params[1].name = "y".to_string();
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_empty, &hir_ok_two).outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 4. Local colliding with parameter name.
        let pre_one = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![pre_bind("x", i32_ty.clone(), o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))],
            ..Default::default()
        };
        let mut hir_shadow = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![hir_bind("x", i32_ty.clone(), o0)],
            locals: vec![hir_bind("x", i32_ty.clone(), None)],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
            ..Default::default()
        };
        hir_shadow.locals[0].initializer = Some(HirExpr::Const(0, i32_ty.clone()));
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_one, &hir_shadow).outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 5. Duplicate local names.
        let mut hir_dup_local = hir_shadow.clone();
        hir_dup_local.locals = vec![
            hir_bind("tmp", i32_ty.clone(), None),
            hir_bind("tmp", i32_ty.clone(), None),
        ];
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_one, &hir_dup_local).outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        // 6. Undeclared live-in variable read in body must fail rather than proving equivalent.
        let pre_livein = PreHirFunction {
            return_type: i32_ty.clone(),
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Var(
                "global_or_uninit_reg".to_string(),
            )))],
            ..Default::default()
        };
        let hir_livein = HirFunction {
            return_type: i32_ty.clone(),
            body: vec![HirStmt::Return(Some(HirExpr::Var(
                "global_or_uninit_reg".to_string(),
            )))],
            ..Default::default()
        };
        let livein_report = check_symbolic_equivalence_report(&pre_livein, &hir_livein);
        assert!(!matches!(
            livein_report.outcome,
            SymbolicOutcome::Equivalent
        ));
        assert!(!livein_report.is_proven());
    }

    #[test]
    fn rejects_non_scalar_and_wide_types_without_silent_clamping() {
        use super::{SymbolicOutcome, check_symbolic_equivalence_report};
        use crate::product::ValidationVerdict;
        use crate::report::UnsupportedReason;
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBindingOrigin, NirType};
        use fission_midend_prehir::{PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let o0 = Some(NirBindingOrigin::ParamIndex(0));
        let unsupported_types = [
            NirType::Float { bits: 32 },
            NirType::Float { bits: 64 },
            NirType::Aggregate {
                size: 16,
                fields: Vec::new(),
            },
            NirType::Ptr(Box::new(i32_ty.clone())),
            NirType::Unknown,
            int(0, false),
            int(128, false),
        ];

        for bad_ty in unsupported_types {
            // 1. Parameter + return type.
            let pre_param = PreHirFunction {
                return_type: bad_ty.clone(),
                params: vec![pre_bind("x", bad_ty.clone(), o0)],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Var("x".to_string())))],
                ..Default::default()
            };
            let hir_param = HirFunction {
                return_type: bad_ty.clone(),
                params: vec![hir_bind("x", bad_ty.clone(), o0)],
                body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
                ..Default::default()
            };
            let param_report = check_symbolic_equivalence_report(&pre_param, &hir_param);
            assert!(
                matches!(
                    param_report.outcome,
                    SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
                ),
                "param/return {bad_ty:?}: expected UnsupportedType, got {:?}",
                param_report.outcome
            );
            assert_eq!(param_report.verdict, ValidationVerdict::Unsupported);

            // 2. Body cast from valid i32 parameter into unsupported/wide type.
            let pre_cast = PreHirFunction {
                return_type: i32_ty.clone(),
                params: vec![pre_bind("x", i32_ty.clone(), o0)],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                    ty: i32_ty.clone(),
                    expr: Box::new(PreHirExpr::Cast {
                        ty: bad_ty.clone(),
                        expr: Box::new(PreHirExpr::Var("x".to_string())),
                    }),
                }))],
                ..Default::default()
            };
            let hir_cast = HirFunction {
                return_type: i32_ty.clone(),
                params: vec![hir_bind("x", i32_ty.clone(), o0)],
                body: vec![HirStmt::Return(Some(HirExpr::Cast {
                    ty: i32_ty.clone(),
                    expr: Box::new(HirExpr::Cast {
                        ty: bad_ty.clone(),
                        expr: Box::new(HirExpr::Var("x".to_string())),
                    }),
                }))],
                ..Default::default()
            };
            let cast_report = check_symbolic_equivalence_report(&pre_cast, &hir_cast);
            assert!(
                matches!(
                    cast_report.outcome,
                    SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
                ),
                "cast {bad_ty:?}: expected UnsupportedType, got {:?}",
                cast_report.outcome
            );

            // 3. Initialized local variable with unsupported type.
            let mut bad_local = pre_bind("loc", bad_ty.clone(), None);
            bad_local.initializer = Some(PreHirExpr::Const(0, bad_ty.clone()));
            let pre_local = PreHirFunction {
                return_type: i32_ty.clone(),
                locals: vec![bad_local],
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    0,
                    i32_ty.clone(),
                )))],
                ..Default::default()
            };
            let mut bad_hir_local = hir_bind("loc", bad_ty.clone(), None);
            bad_hir_local.initializer = Some(HirExpr::Const(0, bad_ty.clone()));
            let hir_local = HirFunction {
                return_type: i32_ty.clone(),
                locals: vec![bad_hir_local],
                body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
                ..Default::default()
            };
            let local_report = check_symbolic_equivalence_report(&pre_local, &hir_local);
            assert!(
                matches!(
                    local_report.outcome,
                    SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
                ),
                "local {bad_ty:?}: expected UnsupportedType, got {:?}",
                local_report.outcome
            );
        }
    }

    #[test]
    fn asymmetric_param_widths_and_signedness_catch_divergence_and_refuse_lossy_equivalence() {
        use super::{SymbolicOutcome, check_symbolic_equivalence_report};
        use crate::product::ValidationVerdict;
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBindingOrigin};
        use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let u32_ty = int(32, false);
        let u8_ty = int(8, false);
        let i64_ty = int(64, true);
        let o0 = Some(NirBindingOrigin::ParamIndex(0));

        // 1. PreHIR u32 returns `(x & 0xff)` cast to u8 while HIR u8 returns `x`:
        //    the solver finds UNSAT because the narrower HIR parameter aliases low 8 bits,
        //    but the correspondence gate MUST refuse equivalence over an asymmetric domain!
        let pre_masked = PreHirFunction {
            return_type: u8_ty.clone(),
            params: vec![pre_bind("x", u32_ty.clone(), o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: u8_ty.clone(),
                expr: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("x".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0xff, u32_ty.clone())),
                    ty: u32_ty.clone(),
                }),
            }))],
            ..Default::default()
        };
        let hir_narrow = HirFunction {
            return_type: u8_ty.clone(),
            params: vec![hir_bind("x", u8_ty.clone(), o0)],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
            ..Default::default()
        };
        let masked_report = check_symbolic_equivalence_report(&pre_masked, &hir_narrow);
        assert!(
            matches!(masked_report.outcome, SymbolicOutcome::Inconclusive { .. }),
            "asymmetric parameter width must not receive Equivalent: {:?}",
            masked_report.outcome
        );
        assert_eq!(masked_report.verdict, ValidationVerdict::Inconclusive);
        assert!(!masked_report.is_proven());
        assert!(
            masked_report
                .rejection_reason
                .as_deref()
                .is_some_and(|r| r.contains("asymmetric/incompatible parameter")),
            "unexpected rejection reason: {:?}",
            masked_report.rejection_reason
        );

        // 2. Same-width but differing parameter signedness (i32 vs u32):
        //    bit-identical `x + 1` -> refused as Inconclusive.
        let pre_signed_add = PreHirFunction {
            return_type: u32_ty.clone(),
            params: vec![pre_bind("x", i32_ty.clone(), o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: u32_ty.clone(),
                expr: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("x".to_string())),
                    rhs: Box::new(PreHirExpr::Const(1, i32_ty.clone())),
                    ty: i32_ty.clone(),
                }),
            }))],
            ..Default::default()
        };
        let hir_unsigned_add = HirFunction {
            return_type: u32_ty.clone(),
            params: vec![hir_bind("x", u32_ty.clone(), o0)],
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: fission_midend_core::ir::HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("x".to_string())),
                rhs: Box::new(HirExpr::Const(1, u32_ty.clone())),
                ty: u32_ty.clone(),
            }))],
            ..Default::default()
        };
        let signedness_report =
            check_symbolic_equivalence_report(&pre_signed_add, &hir_unsigned_add);
        assert!(matches!(
            signedness_report.outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));
        assert!(!signedness_report.is_proven());

        // 3. Same-width signed/unsigned divergence on `(i64)x` MUST still find counterexample!
        let pre_widen = PreHirFunction {
            return_type: i64_ty.clone(),
            params: vec![pre_bind("x", i32_ty, o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: i64_ty.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }))],
            ..Default::default()
        };
        let hir_widen = HirFunction {
            return_type: i64_ty.clone(),
            params: vec![hir_bind("x", u32_ty, o0)],
            body: vec![HirStmt::Return(Some(HirExpr::Cast {
                ty: i64_ty,
                expr: Box::new(HirExpr::Var("x".to_string())),
            }))],
            ..Default::default()
        };
        let widen_report = check_symbolic_equivalence_report(&pre_widen, &hir_widen);
        let SymbolicOutcome::Diverged(cx) = widen_report.outcome else {
            panic!(
                "expected Diverged for signed vs unsigned widening, got {:?}",
                widen_report.outcome
            );
        };
        assert_eq!(cx.arg_widths, vec![32]);
        assert!(cx.args[0] >= 0x8000_0000);
    }

    #[test]
    fn distinguishes_return_comparison_modes_and_signed_unsigned_returns() {
        use super::{
            ReturnComparisonMode, ReturnObservationRelation, SymbolicOutcome,
            check_symbolic_equivalence_report, check_symbolic_equivalence_with_mode,
        };
        use crate::product::{AssuranceBand, EvidenceStrength, ValidationVerdict};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBindingOrigin};
        use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirFunction, PreHirStmt};

        let i8_ty = int(8, true);
        let u8_ty = int(8, false);
        let u32_ty = int(32, false);
        let i64_ty = int(64, true);
        let u64_ty = int(64, false);
        let o0 = Some(NirBindingOrigin::ParamIndex(0));

        // 1. i8(-1) vs u8(255): identical raw 0xff bits, but mathematical integer -1 != 255.
        let pre_i8_neg1 = PreHirFunction {
            return_type: i8_ty.clone(),
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                -1,
                i8_ty.clone(),
            )))],
            ..Default::default()
        };
        let hir_u8_255 = HirFunction {
            return_type: u8_ty.clone(),
            body: vec![HirStmt::Return(Some(HirExpr::Const(255, u8_ty.clone())))],
            ..Default::default()
        };

        let lang_diff = check_symbolic_equivalence_report(&pre_i8_neg1, &hir_u8_255);
        assert!(
            matches!(lang_diff.outcome, SymbolicOutcome::Diverged(_)),
            "i8(-1) and u8(255) must diverge under LanguageValue, got {:?}",
            lang_diff.outcome
        );
        assert_eq!(lang_diff.verdict, ValidationVerdict::Counterexample);

        let exact_diff = check_symbolic_equivalence_with_mode(
            &pre_i8_neg1,
            &hir_u8_255,
            ReturnComparisonMode::ExactScalar,
        );
        assert!(matches!(
            exact_diff.outcome,
            SymbolicOutcome::Inconclusive { .. }
        ));

        let bitpattern_eq = check_symbolic_equivalence_with_mode(
            &pre_i8_neg1,
            &hir_u8_255,
            ReturnComparisonMode::BitPattern,
        );
        assert_eq!(bitpattern_eq.outcome, SymbolicOutcome::Equivalent);
        assert_eq!(
            bitpattern_eq.evidence.strength,
            EvidenceStrength::Conditional
        );
        assert!(!bitpattern_eq.is_unconditionally_proven());
        assert!(matches!(
            bitpattern_eq
                .correspondence
                .as_ref()
                .expect("correspondence")
                .return_relation,
            ReturnObservationRelation::RawBitPattern { bits: 8, .. }
        ));

        // 2. 64-bit boundary: i64(-1) vs u64(u64::MAX) MUST diverge under LanguageValue.
        let pre_i64_neg1 = PreHirFunction {
            return_type: i64_ty.clone(),
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(-1, i64_ty)))],
            ..Default::default()
        };
        let hir_u64_max = HirFunction {
            return_type: u64_ty.clone(),
            body: vec![HirStmt::Return(Some(HirExpr::Const(-1_i64, u64_ty)))],
            ..Default::default()
        };
        assert!(matches!(
            check_symbolic_equivalence_report(&pre_i64_neg1, &hir_u64_max).outcome,
            SymbolicOutcome::Diverged(_)
        ));

        // 3. Non-negative overlap `(i8)(x & 0x7f)` vs `(u32)(x & 0x7f)` with matched `u8` param
        //    proves conditionally equivalent (records signed/unsigned mathematical value assumption).
        let pre_nonneg = PreHirFunction {
            return_type: i8_ty.clone(),
            params: vec![pre_bind("x", u8_ty.clone(), o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: i8_ty.clone(),
                expr: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::And,
                    lhs: Box::new(PreHirExpr::Var("x".to_string())),
                    rhs: Box::new(PreHirExpr::Const(0x7f, u8_ty.clone())),
                    ty: u8_ty.clone(),
                }),
            }))],
            ..Default::default()
        };
        let hir_nonneg = HirFunction {
            return_type: u32_ty.clone(),
            params: vec![hir_bind("x", u8_ty.clone(), o0)],
            body: vec![HirStmt::Return(Some(HirExpr::Cast {
                ty: u32_ty.clone(),
                expr: Box::new(HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::And,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(0x7f, u8_ty)),
                    ty: int(8, false),
                }),
            }))],
            ..Default::default()
        };
        let nonneg_report = check_symbolic_equivalence_report(&pre_nonneg, &hir_nonneg);
        assert_eq!(nonneg_report.outcome, SymbolicOutcome::Equivalent);
        assert_eq!(nonneg_report.band(), AssuranceBand::ConditionallyProven);
        assert!(!nonneg_report.is_unconditionally_proven());

        // 4. Changed output widths `i8(-1)` vs `i32(-1)`:
        //    LanguageValue -> Conditional Equivalence, ExactScalar & BitPattern -> Inconclusive.
        let widened_lang =
            check_symbolic_equivalence_report(&prehir_returning(-1, 8), &hir_returning(-1, 32));
        assert_eq!(widened_lang.outcome, SymbolicOutcome::Equivalent);
        assert_eq!(widened_lang.band(), AssuranceBand::ConditionallyProven);
        assert!(matches!(
            widened_lang
                .correspondence
                .as_ref()
                .expect("correspondence")
                .return_relation,
            ReturnObservationRelation::LanguageValueWidened {
                prehir_bits: 8,
                hir_bits: 32,
                cmp_bits: 32,
                signed: true,
            }
        ));
        for mode in [
            ReturnComparisonMode::ExactScalar,
            ReturnComparisonMode::BitPattern,
        ] {
            let r = check_symbolic_equivalence_with_mode(
                &prehir_returning(-1, 8),
                &hir_returning(-1, 32),
                mode,
            );
            assert!(
                matches!(r.outcome, SymbolicOutcome::Inconclusive { .. }),
                "{mode:?}: width mismatch must be rejected, got {:?}",
                r.outcome
            );
        }
    }

    #[test]
    fn exhaustive_4bit_enumeration_crosschecks_symbolic_equivalence_and_assurance_report() {
        use super::{
            ParamIdentityBasis, SymbolicOutcome, as_math_int, check_symbolic_equivalence_report,
            symbolic_ast_return_scope,
        };
        use crate::eval::{interpret_hir, interpret_prehir};
        use crate::product::{
            AssumptionKind, AssuranceBand, AssuranceClassification, EvidenceStrength,
            FoundationIdentity, ValidationVerdict,
        };
        use crate::report::UnsupportedReason;
        use fission_midend_core::ir::{
            HirBinaryOp, HirExpr, HirFunction, HirStmt, NirBindingOrigin,
        };
        use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirFunction, PreHirStmt};

        let i4_ty = int(4, true);
        let u4_ty = int(4, false);
        let o0 = Some(NirBindingOrigin::ParamIndex(0));
        let o1 = Some(NirBindingOrigin::ParamIndex(1));
        let foundation = FoundationIdentity {
            model_version: 1,
            function_address: 0x401000,
            block_count: 1,
            op_count: 1,
            digest: 0x1234,
        };

        // Case A: Exact 4-bit two-arg function `(a + b) ^ 3` vs `(b ^ 0) + a`? No, `(a + b) - 1` vs `(b + a) + (-1)`.
        // Verify all 16 * 16 = 256 4-bit inputs agree exhaustively AND symbolically with Universal Proven band.
        let pre_i4 = PreHirFunction {
            return_type: i4_ty.clone(),
            params: vec![
                pre_bind("a", i4_ty.clone(), o0),
                pre_bind("b", i4_ty.clone(), o1),
            ],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs: Box::new(PreHirExpr::Var("a".to_string())),
                    rhs: Box::new(PreHirExpr::Var("b".to_string())),
                    ty: i4_ty.clone(),
                }),
                rhs: Box::new(PreHirExpr::Const(1, i4_ty.clone())),
                ty: i4_ty.clone(),
            }))],
            ..Default::default()
        };
        let hir_i4 = HirFunction {
            return_type: i4_ty.clone(),
            params: vec![
                hir_bind("a", i4_ty.clone(), o0),
                hir_bind("b", i4_ty.clone(), o1),
            ],
            body: vec![HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("b".to_string())),
                    rhs: Box::new(HirExpr::Var("a".to_string())),
                    ty: i4_ty.clone(),
                }),
                rhs: Box::new(HirExpr::Const(-1, i4_ty.clone())),
                ty: i4_ty.clone(),
            }))],
            ..Default::default()
        };

        let mut exhaustive_diffs = Vec::new();
        for a in 0_i64..16 {
            for b in 0_i64..16 {
                let p = interpret_prehir(&pre_i4.body, &pre_i4.params, &pre_i4.locals, &[a, b])
                    .expect("prehir eval")
                    .expect("value");
                let h = interpret_hir(&hir_i4.body, &hir_i4.params, &hir_i4.locals, &[a, b])
                    .expect("hir eval")
                    .expect("value");
                if as_math_int(p as u64, 4, true) != as_math_int(h as u64, 4, true) {
                    exhaustive_diffs.push((a as u64, b as u64));
                }
            }
        }
        assert!(exhaustive_diffs.is_empty());

        let proven_report = check_symbolic_equivalence_report(&pre_i4, &hir_i4);
        assert_eq!(proven_report.outcome, SymbolicOutcome::Equivalent);
        assert_eq!(proven_report.band(), AssuranceBand::Proven);
        assert_eq!(
            proven_report.classification(),
            AssuranceClassification::UniversallyProven
        );
        assert!(proven_report.is_unconditionally_proven());
        assert_eq!(proven_report.scope, symbolic_ast_return_scope());
        let corr = proven_report.correspondence.as_ref().expect("corr");
        assert_eq!(
            corr.params[0].basis,
            ParamIdentityBasis::StorageOrigin(NirBindingOrigin::ParamIndex(0))
        );
        assert_eq!(
            proven_report
                .to_assurance_report(foundation)
                .expect("assurance contract")
                .band(),
            AssuranceBand::Proven
        );

        // Case B: Positional-only (`origin: None`) equivalent pair becomes ConditionallyProven.
        let mut pre_pos = pre_i4.clone();
        let mut hir_pos = hir_i4.clone();
        for p in &mut pre_pos.params {
            p.origin = None;
        }
        for p in &mut hir_pos.params {
            p.origin = None;
        }
        let cond_report = check_symbolic_equivalence_report(&pre_pos, &hir_pos);
        assert_eq!(cond_report.outcome, SymbolicOutcome::Equivalent);
        assert_eq!(cond_report.band(), AssuranceBand::ConditionallyProven);
        assert!(!cond_report.is_unconditionally_proven());
        assert!(
            cond_report
                .evidence
                .assumptions
                .iter()
                .any(|a| a.kind == AssumptionKind::CallingConvention)
        );
        assert_eq!(
            cond_report
                .to_assurance_report(foundation)
                .expect("assurance contract")
                .band(),
            AssuranceBand::ConditionallyProven
        );

        // Case C: Divergent 4-bit pair returning `i4` (prehir) vs `u4` (hir):
        // matched bit pattern for non-negative values (0..=7), diverges on 8..=15.
        let pre_div_i4 = PreHirFunction {
            return_type: i4_ty.clone(),
            params: vec![pre_bind("x", u4_ty.clone(), o0)],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: i4_ty,
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }))],
            ..Default::default()
        };
        let hir_div_u4 = HirFunction {
            return_type: u4_ty.clone(),
            params: vec![hir_bind("x", u4_ty.clone(), o0)],
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))],
            ..Default::default()
        };

        let mut div_set = Vec::new();
        for x in 0_i64..16 {
            let p = interpret_prehir(&pre_div_i4.body, &pre_div_i4.params, &[], &[x])
                .expect("pre eval")
                .expect("val");
            let h = interpret_hir(&hir_div_u4.body, &hir_div_u4.params, &[], &[x])
                .expect("hir eval")
                .expect("val");
            if as_math_int(p as u64, 4, true) != as_math_int(h as u64, 4, false) {
                div_set.push(x as u64);
            }
        }
        assert_eq!(div_set, (8_u64..16).collect::<Vec<_>>());

        let div_report = check_symbolic_equivalence_report(&pre_div_i4, &hir_div_u4);
        let SymbolicOutcome::Diverged(cx) = &div_report.outcome else {
            panic!("expected Diverged, got {:?}", div_report.outcome);
        };
        assert_eq!(cx.arg_widths, vec![4]);
        assert!(div_set.contains(&cx.args[0]));
        assert_eq!(div_report.verdict, ValidationVerdict::Counterexample);
        assert_eq!(div_report.evidence.strength, EvidenceStrength::None);
        assert!(
            div_report
                .to_assurance_report(foundation)
                .expect("counterexample assurance")
                .is_proven()
                == false
        );

        // Case D: Inconclusive and Unsupported reports also satisfy `AssuranceReport` invariants.
        let mut mismatch_hir = hir_i4;
        mismatch_hir.params.pop();
        mismatch_hir.body = vec![HirStmt::Return(Some(HirExpr::Var("a".to_string())))];
        let inc_report = check_symbolic_equivalence_report(&pre_i4, &mismatch_hir);
        assert_eq!(inc_report.verdict, ValidationVerdict::Inconclusive);
        assert_eq!(
            inc_report
                .to_assurance_report(foundation)
                .expect("inconclusive assurance")
                .verdict(),
            ValidationVerdict::Inconclusive
        );

        let unsup_ty = fission_midend_core::ir::NirType::Float { bits: 32 };
        let pre_unsup = PreHirFunction {
            return_type: unsup_ty.clone(),
            ..Default::default()
        };
        let hir_unsup = HirFunction {
            return_type: unsup_ty,
            ..Default::default()
        };
        let unsup_report = check_symbolic_equivalence_report(&pre_unsup, &hir_unsup);
        assert!(matches!(
            unsup_report.outcome,
            SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
        ));
        assert_eq!(
            unsup_report
                .to_assurance_report(foundation)
                .expect("unsupported assurance")
                .verdict(),
            ValidationVerdict::Unsupported
        );
    }
}
