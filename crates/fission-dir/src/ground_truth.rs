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

use fission_emulator::pcode::page_map::PageFault;
use fission_loader::loader::LoadedBinary;
use fission_midend_core::ir::{HirFunction, NirBindingOrigin, NirType};
use fission_midend_prehir::PreHirFunction;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::call_sites::MachineFlow;
use crate::emu_driver::{
    CallOutcome, CallStub, EmulatorHarness, EscapedFramePointer, FrameExclusionAudit,
    FrameExclusionJustification, FrameExclusionSpec, MemoryAccessAudit, ObservedCall, StubbedEnd,
};
use crate::eval::{
    CallEvent, Run as InterpRun, UndefinedRead, World, hir_addressed_locals, hir_free_names,
    hir_mentioned_names, normalize, prehir_addressed_locals, run_hir, run_prehir, width_of,
};
use crate::initial_state::{
    InitialState, InitialStateIdentity, InitialStatePolicy, RunPlan, StateAdmissibility,
};
use crate::product::{
    AssumptionKind, AssuranceContractError, AssuranceReport, CallObservation, DirCounterexample,
    EvidenceStrength, FoundationIdentity, ObservationScope, UnresolvedEffect, UnresolvedEffectKind,
    ValidationAssumption, ValidationBudget, ValidationEvidence, ValidationVerdict,
};
use crate::report::{Divergence, UnsupportedReason, VerifyOutcome};
use fission_decompiler::PrintedGlobal;

/// Divergent runs a function is checked until.
const MAX_REPORTED_DIVERGENCES: usize = 3;

/// Maximum representative non-divergent witnesses retained in a [`GroundTruthReport`].
const MAX_SAMPLE_WITNESSES: usize = 8;

/// Maximum synthetic unmapped addresses sampled per interpreter run.
const MAX_INTERP_SYNTHETIC_SAMPLES: usize = 16;

/// Size of the byte window checked around an escaped frame pointer returned or
/// stored externally by a function whose frame exclusion was unjustified.
const ESCAPED_FRAME_CHECK_WINDOW: u64 = 64;

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

/// How external memory effects are compared in concrete ground-truth checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryComparisonKind {
    /// Final written byte states agree across all addresses outside the audited
    /// private stack frame exclusion (does not enforce intermediate write order).
    FinalStateOutsideExcludedFrame,
}

/// How stubbed callee invocations are compared in concrete ground-truth checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallComparisonKind {
    /// Ordered call sequence, instruction sites, and argument widths/values agree.
    OrderedTrace,
}

/// Explicit declaration of what one concrete verification run checks and what
/// effects lie outside its oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcreteObservationScope {
    pub return_bits: u32,
    pub return_value_checked: bool,
    pub exit_kind_checked: bool,
    pub callee_saved_checked: bool,
    pub memory_comparison: MemoryComparisonKind,
    pub call_comparison: CallComparisonKind,
    pub traps_and_faults_checked: bool,
    pub unsupported_effects: Vec<String>,
}

impl ConcreteObservationScope {
    #[must_use]
    pub fn for_function(return_bits: u32, return_value_checked: bool) -> Self {
        Self {
            return_bits,
            return_value_checked,
            exit_kind_checked: true,
            callee_saved_checked: true,
            memory_comparison: MemoryComparisonKind::FinalStateOutsideExcludedFrame,
            call_comparison: CallComparisonKind::OrderedTrace,
            traps_and_faults_checked: true,
            unsupported_effects: vec![
                "floating-point and vector registers outside modeled integer ABI slots".to_string(),
                "host syscalls, I/O, randomness, clocks, and concurrency/interleaving".to_string(),
                "intermediate store ordering inside identically-ending non-overlapping writes"
                    .to_string(),
            ],
        }
    }

    #[must_use]
    pub fn to_product_scope(&self) -> ObservationScope {
        ObservationScope {
            return_value: self.return_value_checked,
            exit_kind: self.exit_kind_checked,
            live_out_locations: Vec::new(),
            memory_ranges: Vec::new(),
            calls: CallObservation::TypedEffects,
            traps: self.traps_and_faults_checked,
        }
    }
}

/// How one side of a concrete run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationKind {
    Returned,
    NoReturnCall(u64),
    HitBudget,
    ProcessExited,
    LeftDecodedCode(u64),
    Fault(String),
    Trap(String),
    UndefinedRead(String),
    Unsupported(String),
}

impl TerminationKind {
    #[must_use]
    pub fn is_trap_or_fault(&self) -> bool {
        matches!(
            self,
            Self::HitBudget | Self::ProcessExited | Self::Fault(_) | Self::Trap(_)
        )
    }
}

/// Role of a [`ConcreteWitness`] within [`GroundTruthReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessKind {
    AgreedSample,
    Divergence,
    Suspect,
    Unknown,
    AsymmetricFault,
    SkippedHarnessError,
    SkippedInterpreterError,
}

/// Summary of call-stub behavior active during a concrete run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubSummary {
    pub calls_intercepted: usize,
    pub call_sites: Vec<u64>,
    pub clobbers_enabled: bool,
}

/// Detailed witness for an individual concrete execution, preserving exact
/// initial state, admissibility classification, stub summary, and audits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcreteWitness {
    pub kind: WitnessKind,
    pub initial_state: InitialStateIdentity,
    pub args: Vec<i64>,
    pub stub_summary: StubSummary,
    pub frame_audit: Option<FrameExclusionAudit>,
    pub memory_audit: Option<MemoryAccessAudit>,
    pub machine_termination: TerminationKind,
    pub hir_termination: TerminationKind,
    pub prehir_termination: Option<TerminationKind>,
    pub machine_return: Option<i64>,
    pub hir_return: Option<i64>,
    pub prehir_return: Option<i64>,
    pub detail: Option<String>,
}

/// Complete denominator accounting across all planned concrete runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GroundTruthDenominators {
    pub planned_runs: usize,
    pub attempted_runs: usize,
    pub checked_runs: usize,
    pub agreed_runs: usize,
    pub diverged_runs: usize,
    pub suspect_runs: usize,
    pub native_admissible_runs: usize,
    pub synthetic_model_runs: usize,
    pub unknown_runs: usize,
    pub skipped_interpreter_errors: usize,
    pub skipped_emulator_errors: usize,
    pub asymmetric_fault_runs: usize,
    pub unjustified_frame_runs: usize,
    pub prehir_skipped_runs: usize,
}

/// Explicit, non-overstated input case for native-admissible verification.
#[derive(Debug, Clone, Copy)]
pub struct AdmissibleCase<'a> {
    pub args: &'a [u64],
    pub initial_memory: &'a [(u64, u8)],
}

/// Audited concrete verification report preserving denominators, policy, scope,
/// assumptions, unresolved effects, and native-vs-synthetic witnesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundTruthReport {
    pub outcome: VerifyOutcome,
    pub policy: InitialStatePolicy,
    pub observation_scope: ConcreteObservationScope,
    pub denominators: GroundTruthDenominators,
    pub assumptions: Vec<ValidationAssumption>,
    pub unresolved_effects: Vec<UnresolvedEffect>,
    pub witnesses: Vec<ConcreteWitness>,
}

impl GroundTruthReport {
    #[must_use]
    pub fn native_admissible_divergences(&self) -> Vec<&ConcreteWitness> {
        self.witnesses
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    WitnessKind::Divergence | WitnessKind::AsymmetricFault
                ) && w.initial_state.admissibility == StateAdmissibility::NativeAdmissible
            })
            .collect()
    }

    #[must_use]
    pub fn synthetic_divergences(&self) -> Vec<&ConcreteWitness> {
        self.witnesses
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    WitnessKind::Divergence | WitnessKind::AsymmetricFault
                ) && w.initial_state.admissibility == StateAdmissibility::SyntheticModel
            })
            .collect()
    }

    #[must_use]
    pub fn native_admissible_agreements(&self) -> usize {
        if !matches!(self.outcome, VerifyOutcome::Equivalent { .. }) {
            return 0;
        }
        self.denominators.native_admissible_runs
    }

    #[must_use]
    pub fn synthetic_agreements(&self) -> usize {
        if !matches!(self.outcome, VerifyOutcome::Equivalent { .. }) {
            return 0;
        }
        self.denominators.synthetic_model_runs
    }

    /// Project this concrete report into the canonical DIR [`AssuranceReport`]
    /// contract. Sampled concrete passes NEVER claim `Universal` or
    /// `Conditional` proof; they project strictly to `EvidenceStrength::Traces`
    /// (`AssuranceBand::TraceValidated`).
    pub fn to_assurance_report(
        &self,
        foundation: FoundationIdentity,
    ) -> Result<AssuranceReport, AssuranceContractError> {
        let scope = self.observation_scope.to_product_scope();
        let budget = ValidationBudget {
            max_traces: Some(self.denominators.planned_runs as u64),
            ..ValidationBudget::default()
        };
        match &self.outcome {
            VerifyOutcome::Equivalent { checked } => {
                let mut unresolved = self.unresolved_effects.clone();
                unresolved.push(UnresolvedEffect {
                    kind: UnresolvedEffectKind::PathBudget,
                    description: format!(
                        "concrete sampling covers {} checked trace(s) ({} native-admissible, \
                         {} synthetic-model, {} unknown, {} skipped-interp, {} skipped-emu) \
                         and is not a universal equivalence proof",
                        checked,
                        self.denominators.native_admissible_runs,
                        self.denominators.synthetic_model_runs,
                        self.denominators.unknown_runs,
                        self.denominators.skipped_interpreter_errors,
                        self.denominators.skipped_emulator_errors,
                    ),
                });
                AssuranceReport::new(
                    foundation,
                    scope,
                    ValidationVerdict::Equivalent,
                    ValidationEvidence {
                        strength: EvidenceStrength::Traces,
                        budget,
                        explored_paths: 0,
                        executed_traces: *checked as u64,
                        assumptions: self.assumptions.clone(),
                        unresolved_effects: unresolved,
                        counterexample: None,
                        detail: format!(
                            "agreed on {checked}/{} planned concrete runs ({} native-admissible, \
                             {} synthetic-model)",
                            self.denominators.planned_runs,
                            self.denominators.native_admissible_runs,
                            self.denominators.synthetic_model_runs,
                        ),
                    },
                )
            }
            VerifyOutcome::Diverged(divergences) => {
                let first = &divergences[0];
                let counterexample = DirCounterexample {
                    inputs: first
                        .args
                        .iter()
                        .enumerate()
                        .map(|(i, val)| (format!("arg_{i}"), *val as u64))
                        .collect(),
                    foundation_observation: format!(
                        "emulator_ret={:?}, detail={}",
                        first.emulator_result,
                        first.detail.as_deref().unwrap_or("none"),
                    ),
                    candidate_observation: format!(
                        "hir_ret={:?}, prehir_ret={:?}",
                        first.hir_result, first.prehir_result,
                    ),
                };
                AssuranceReport::new(
                    foundation,
                    scope,
                    ValidationVerdict::Counterexample,
                    ValidationEvidence {
                        strength: EvidenceStrength::None,
                        budget,
                        explored_paths: 0,
                        executed_traces: self.denominators.checked_runs as u64,
                        assumptions: self.assumptions.clone(),
                        unresolved_effects: self.unresolved_effects.clone(),
                        counterexample: Some(counterexample),
                        detail: first
                            .detail
                            .clone()
                            .unwrap_or_else(|| "concrete divergence observed".to_string()),
                    },
                )
            }
            VerifyOutcome::Suspect(suspects) => AssuranceReport::new(
                foundation,
                scope,
                ValidationVerdict::Inconclusive,
                ValidationEvidence {
                    strength: EvidenceStrength::None,
                    budget,
                    explored_paths: 0,
                    executed_traces: self.denominators.checked_runs as u64,
                    assumptions: self.assumptions.clone(),
                    unresolved_effects: self.unresolved_effects.clone(),
                    counterexample: None,
                    detail: suspects
                        .first()
                        .and_then(|s| s.detail.clone())
                        .unwrap_or_else(|| "recovered callee prototype mismatch".to_string()),
                },
            ),
            VerifyOutcome::Inconclusive { reason } => AssuranceReport::new(
                foundation,
                scope,
                ValidationVerdict::Inconclusive,
                ValidationEvidence {
                    strength: EvidenceStrength::None,
                    budget,
                    explored_paths: 0,
                    executed_traces: self.denominators.checked_runs as u64,
                    assumptions: self.assumptions.clone(),
                    unresolved_effects: self.unresolved_effects.clone(),
                    counterexample: None,
                    detail: reason.clone(),
                },
            ),
            VerifyOutcome::Unsupported(reason) => AssuranceReport::new(
                foundation,
                scope,
                ValidationVerdict::Unsupported,
                ValidationEvidence {
                    strength: EvidenceStrength::None,
                    budget,
                    explored_paths: 0,
                    executed_traces: self.denominators.checked_runs as u64,
                    assumptions: self.assumptions.clone(),
                    unresolved_effects: self.unresolved_effects.clone(),
                    counterexample: None,
                    detail: reason.to_string(),
                },
            ),
        }
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

/// Run [`check_ground_truth_report`] and return only its [`VerifyOutcome`].
pub fn check_ground_truth_with(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    plan: &RunPlan,
    grounding: &Grounding,
) -> VerifyOutcome {
    check_ground_truth_report(harness, address, prehir, hir, plan, grounding).outcome
}

/// Audited version of [`check_ground_truth_with`]: preserves initial-state
/// policy, exact observation scope, full run denominators, and native-vs-
/// synthetic witness separation.
pub fn check_ground_truth_report(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    plan: &RunPlan,
    grounding: &Grounding,
) -> GroundTruthReport {
    run_ground_truth_suite(
        harness,
        address,
        prehir,
        hir,
        grounding,
        InitialStatePolicy::synthetic_default(),
        RunSuiteKind::SyntheticPlan(plan),
    )
}

/// Check one explicit [`AdmissibleCase`] under strict native memory and
/// permission enforcement (`lazy_memory = None`, no synthetic call stubs).
pub fn check_admissible_case(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    case: &AdmissibleCase<'_>,
    grounding: &Grounding,
) -> GroundTruthReport {
    run_ground_truth_suite(
        harness,
        address,
        prehir,
        hir,
        grounding,
        InitialStatePolicy::native_admissible(),
        RunSuiteKind::NativeCase(case),
    )
}

enum RunSuiteKind<'a> {
    SyntheticPlan(&'a RunPlan),
    NativeCase(&'a AdmissibleCase<'a>),
}

fn push_witness_bounded(
    witnesses: &mut Vec<ConcreteWitness>,
    non_error_samples: &mut usize,
    witness: ConcreteWitness,
) {
    match witness.kind {
        WitnessKind::Divergence | WitnessKind::Suspect | WitnessKind::AsymmetricFault => {
            witnesses.push(witness);
        }
        _ => {
            if *non_error_samples < MAX_SAMPLE_WITNESSES {
                *non_error_samples += 1;
                witnesses.push(witness);
            }
        }
    }
}

fn is_interp_runtime_trap(msg: &str) -> bool {
    msg.contains("division by zero")
        || msg.contains("modulo by zero")
        || msg.contains("did not finish within")
        || msg.contains("more than ")
        || msg.contains("load from unmapped address")
}

fn classify_interp_err(err: &anyhow::Error) -> TerminationKind {
    if let Some(undef) = err.downcast_ref::<UndefinedRead>() {
        return TerminationKind::UndefinedRead(undef.to_string());
    }
    let msg = err.to_string();
    if is_interp_runtime_trap(&msg) {
        TerminationKind::Trap(msg)
    } else {
        TerminationKind::Unsupported(msg)
    }
}

fn classify_machine_err(err: &anyhow::Error, strict_native: bool) -> (TerminationKind, bool) {
    if let Some(fault) = err.downcast_ref::<PageFault>() {
        // In strict native mode every mapping/permission fault is a genuine
        // fault event; in synthetic mode a PageFault reflects the emulator
        // page map's unmappable boundary (such as the top page) unless the
        // tree also trapped.
        return (TerminationKind::Fault(fault.to_string()), strict_native);
    }
    let msg = err.to_string();
    let is_arith_or_fault = msg.contains("divide")
        || msg.contains("division")
        || msg.contains("fault")
        || msg.contains("p-code budget");
    (
        TerminationKind::Fault(msg),
        strict_native || is_arith_or_fault,
    )
}

fn build_report_assumptions(
    policy: &InitialStatePolicy,
    grounding: &Grounding,
) -> Vec<ValidationAssumption> {
    let mut assumptions = vec![ValidationAssumption {
        kind: AssumptionKind::CallingConvention,
        description: "Integer argument slots and return register follow target ABI calling \
                      convention; floating-point/vector ABI functions are refused."
            .to_string(),
    }];
    if policy.lazy_memory_enabled {
        assumptions.push(ValidationAssumption {
            kind: AssumptionKind::Memory,
            description: "Unmapped memory reads/writes use deterministic `InitialState` lazy \
                          pages (`SyntheticModel` unless accesses stay strictly inside mapped \
                          image/stack/scratch)."
                .to_string(),
        });
    } else {
        assumptions.push(ValidationAssumption {
            kind: AssumptionKind::Memory,
            description: "Strict `PageMap` permissions enforced (`lazy_memory = None`); only \
                          mapped image, stack, and explicit scratch buffers are accessible."
                .to_string(),
        });
    }
    if policy.stubbed_calls_enabled
        && grounding
            .flow
            .as_ref()
            .is_some_and(|f| !f.calls.is_empty() || !f.tail_jumps.is_empty())
    {
        assumptions.push(ValidationAssumption {
            kind: AssumptionKind::ExternalCall,
            description: "Direct calls and tail jumps are intercepted by deterministic `CallStub` \
                          models rather than executing callee machine code."
                .to_string(),
        });
    }
    assumptions
}

fn run_ground_truth_suite(
    harness: &mut EmulatorHarness,
    address: u64,
    prehir: &PreHirFunction,
    hir: &HirFunction,
    grounding: &Grounding,
    policy: InitialStatePolicy,
    suite: RunSuiteKind<'_>,
) -> GroundTruthReport {
    let planned_runs = match suite {
        RunSuiteKind::SyntheticPlan(plan) => plan.runs,
        RunSuiteKind::NativeCase(_) => 1,
    };
    let return_bits = width_of(&hir.return_type).clamp(1, 64);
    let declares_value = !matches!(hir.return_type, NirType::Unknown)
        || !matches!(prehir.return_type, NirType::Unknown);
    let observation_scope = ConcreteObservationScope::for_function(return_bits, declares_value);
    let assumptions = build_report_assumptions(&policy, grounding);

    let early_unsupported = |reason: UnsupportedReason, extra_effect: Option<UnresolvedEffect>| {
        let mut unresolved = Vec::new();
        if let Some(effect) = extra_effect {
            unresolved.push(effect);
        }
        GroundTruthReport {
            outcome: VerifyOutcome::Unsupported(reason),
            policy: policy.clone(),
            observation_scope: observation_scope.clone(),
            denominators: GroundTruthDenominators {
                planned_runs,
                ..GroundTruthDenominators::default()
            },
            assumptions: assumptions.clone(),
            unresolved_effects: unresolved,
            witnesses: Vec::new(),
        }
    };

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
        return early_unsupported(
            UnsupportedReason::Construct("floating-point ABI in ground-truth"),
            Some(UnresolvedEffect {
                kind: UnresolvedEffectKind::UnsupportedOpcode,
                description: "floating-point parameter or return ABI is not modeled".to_string(),
            }),
        );
    }

    if let Some(name) = hir_mentioned_names(hir)
        .into_iter()
        .find(|name| grounding.ambiguous_globals.contains(name))
    {
        return early_unsupported(
            UnsupportedReason::Harness(format!(
                "global '{name}' names several objects; the text does not say which"
            )),
            Some(UnresolvedEffect {
                kind: UnresolvedEffectKind::MemoryAlias,
                description: format!("global '{name}' resolved ambiguously across symbols"),
            }),
        );
    }

    let mut free = hir_free_names(hir);
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

    let hir_local_offsets = match frame_offsets(
        &hir_addressed_locals(&hir.body, &hir.locals),
        &grounding.frame_offsets,
    ) {
        Ok(offsets) => offsets,
        Err(reason) => {
            return early_unsupported(
                UnsupportedReason::Interpreter(format!("hir: {reason}")),
                None,
            );
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
    let no_placed: HashMap<u64, u8> = HashMap::new();
    let norm = |v: i64| normalize(mask_to_width(v as u64, return_bits), &hir.return_type);

    let mut divergences = Vec::new();
    let mut suspects = Vec::new();
    let mut denominators = GroundTruthDenominators {
        planned_runs,
        ..GroundTruthDenominators::default()
    };
    let mut unresolved_effects = Vec::new();
    let mut witnesses = Vec::new();
    let mut non_error_samples = 0usize;
    let mut first_interpreter_error: Option<String> = None;
    let mut first_unknown: Option<String> = None;
    let mut first_emulator_error: Option<String> = None;

    let strict_native = matches!(suite, RunSuiteKind::NativeCase(_));

    for run_idx in 0..planned_runs {
        if divergences.len() >= MAX_REPORTED_DIVERGENCES {
            break;
        }
        denominators.attempted_runs += 1;

        let (raw_slots, seed_opt, run_opt, state_opt, image_slice) = match suite {
            RunSuiteKind::SyntheticPlan(plan) => {
                let state = InitialState::new(plan.seed, run_idx as u32);
                let raw: Vec<u64> = (0..slot_count)
                    .map(|slot| match slots.iter().position(|&s| s == slot) {
                        Some(i) => state.argument_word(slot, &hir.params[i].ty, slot_bits),
                        None => state.argument_word(slot, &NirType::Unknown, slot_bits),
                    })
                    .collect();
                (
                    raw,
                    Some(plan.seed),
                    Some(run_idx as u32),
                    Some(state),
                    &[][..],
                )
            }
            RunSuiteKind::NativeCase(case) => {
                let mut raw = vec![0u64; slot_count.max(case.args.len())];
                for (i, &slot) in slots.iter().enumerate() {
                    if let Some(&arg) = case.args.get(i) {
                        raw[slot] = arg;
                    }
                }
                if case.args.len() > slots.len() {
                    for (i, &arg) in case.args.iter().enumerate() {
                        raw[i] = arg;
                    }
                }
                (raw, None, Some(0), None, case.initial_memory)
            }
        };

        let args: Vec<i64> = slots
            .iter()
            .zip(&hir.params)
            .map(|(&slot, p)| normalize(raw_slots.get(slot).copied().unwrap_or(0) as i64, &p.ty))
            .collect();
        let prehir_args: Vec<i64> = prehir_slots
            .iter()
            .zip(&prehir.params)
            .map(|(&slot, p)| normalize(raw_slots.get(slot).copied().unwrap_or(0) as i64, &p.ty))
            .collect();

        let setup_res = match &state_opt {
            Some(state) => harness.begin_run(address, &raw_slots, state),
            None => harness.begin_native_run(address, &raw_slots),
        };
        if let Err(err) = setup_res {
            let msg = format!("call setup failed: {err}");
            first_emulator_error.get_or_insert_with(|| msg.clone());
            denominators.skipped_emulator_errors += 1;
            push_witness_bounded(
                &mut witnesses,
                &mut non_error_samples,
                ConcreteWitness {
                    kind: WitnessKind::SkippedHarnessError,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility: StateAdmissibility::Unknown,
                        admissibility_reasons: vec![msg.clone()],
                    },
                    args,
                    stub_summary: StubSummary {
                        calls_intercepted: 0,
                        call_sites: Vec::new(),
                        clobbers_enabled: false,
                    },
                    frame_audit: None,
                    memory_audit: None,
                    machine_termination: TerminationKind::Fault(msg.clone()),
                    hir_termination: TerminationKind::Unsupported("not executed".to_string()),
                    prehir_termination: None,
                    machine_return: None,
                    hir_return: None,
                    prehir_return: None,
                    detail: Some(msg),
                },
            );
            continue;
        }

        let bound = harness.registers(&free);
        let below_entry = match harness.frame_below_entry() {
            Ok(range) => range,
            Err(err) => {
                first_emulator_error.get_or_insert_with(|| format!("call setup failed: {err}"));
                denominators.skipped_emulator_errors += 1;
                continue;
            }
        };
        let (cfa, frame) = match (harness.cfa(), harness.frame_range()) {
            (Ok(cfa), Ok(frame)) => (cfa, frame),
            (Err(err), _) | (_, Err(err)) => {
                first_emulator_error.get_or_insert_with(|| format!("call setup failed: {err}"));
                denominators.skipped_emulator_errors += 1;
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

        let interp_synthetic_reads = RefCell::new(Vec::<u64>::new());
        let initial_mem_map: HashMap<u64, u8> = image_slice.iter().copied().collect();
        let (prehir_r, hir_r) = {
            let backing = |addr: u64| -> Option<u8> {
                if let Some(byte) = harness.baseline_byte(addr) {
                    return Some(byte);
                }
                if let Some(&byte) = initial_mem_map.get(&addr) {
                    return Some(byte);
                }
                if let Some(state) = &state_opt {
                    let mut log = interp_synthetic_reads.borrow_mut();
                    if log.len() < MAX_INTERP_SYNTHETIC_SAMPLES && !log.contains(&addr) {
                        log.push(addr);
                    }
                    Some(state.memory_byte(addr))
                } else {
                    None
                }
            };
            let call_return_fn = state_opt.as_ref().map(|s| move |n: usize| s.call_result(n));
            let call_effects_state = state_opt.as_ref();
            let call_effects = call_effects_state.map(|state| CallEffects {
                state,
                objects: &objects,
                frame: &frame,
                tls: tls.as_ref(),
                grounding,
            });
            let call_writes_fn = call_effects
                .as_ref()
                .map(|eff| move |n: usize, c_args: &[u64]| eff.writes(n, c_args));
            let call_return_ref: Option<&dyn Fn(usize) -> u64> =
                match (&grounding.flow, &call_return_fn) {
                    (Some(_), Some(f)) => Some(f),
                    _ => None,
                };
            let call_writes_ref: Option<&dyn Fn(usize, &[u64]) -> Vec<(u64, u8)>> = call_writes_fn
                .as_ref()
                .map(|f| f as &dyn Fn(usize, &[u64]) -> Vec<(u64, u8)>);
            let world = World {
                image: image_slice,
                bound: &bound,
                backing: Some(&backing),
                globals: Some(&grounding.globals),
                global_types: Some(&grounding.global_types),
                call_return: call_return_ref,
                no_return_sites: Some(&no_return_sites),
                local_addresses: Some(&hir_locals),
                call_writes: call_writes_ref,
                undefined_frame: Some(&below_entry),
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
        let interp_used_synthetic = !interp_synthetic_reads.borrow().is_empty();
        if hir_r.is_ok() && prehir_r.is_err() {
            denominators.prehir_skipped_runs += 1;
        }

        let hir_term = match &hir_r {
            Ok(run) if run.ended_in_no_return => TerminationKind::NoReturnCall(
                run.calls.last().and_then(|c| c.site).unwrap_or_default(),
            ),
            Ok(_) => TerminationKind::Returned,
            Err(err) => classify_interp_err(err),
        };
        let prehir_term = Some(match &prehir_r {
            Ok(run) if run.ended_in_no_return => TerminationKind::NoReturnCall(
                run.calls.last().and_then(|c| c.site).unwrap_or_default(),
            ),
            Ok(_) => TerminationKind::Returned,
            Err(err) => classify_interp_err(err),
        });

        // Unsupported interpreter constructs cannot meaningfully compare against
        // machine execution; record denominator & sample cleanly.
        if let TerminationKind::Unsupported(msg) = &hir_term {
            let detail_msg = format!("hir: {msg}");
            first_interpreter_error.get_or_insert_with(|| detail_msg.clone());
            denominators.skipped_interpreter_errors += 1;
            push_witness_bounded(
                &mut witnesses,
                &mut non_error_samples,
                ConcreteWitness {
                    kind: WitnessKind::SkippedInterpreterError,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility: StateAdmissibility::Unknown,
                        admissibility_reasons: vec![format!("hir unsupported: {msg}")],
                    },
                    args,
                    stub_summary: StubSummary {
                        calls_intercepted: 0,
                        call_sites: Vec::new(),
                        clobbers_enabled: false,
                    },
                    frame_audit: None,
                    memory_audit: None,
                    machine_termination: TerminationKind::Unsupported(
                        "skipped before run".to_string(),
                    ),
                    hir_termination: hir_term,
                    prehir_termination: prehir_term,
                    machine_return: None,
                    hir_return: None,
                    prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                    detail: Some(detail_msg),
                },
            );
            continue;
        }

        // Prepare call stub and execute the real emulator even when HIR trapped
        // or hit UndefinedRead so asymmetric traps/faults are caught.
        let stub_result_state = state_opt;
        let zero_stub = |_: usize| 0u64;
        let state_stub_ret = |n: usize| stub_result_state.map_or(0, |s| s.call_result(n));
        let state_stub_clobber =
            |n: usize, index: usize| stub_result_state.map_or(0, |s| s.call_clobber(n, index));
        let state_call_effects = state_opt.as_ref().map(|state| CallEffects {
            state,
            objects: &objects,
            frame: &frame,
            tls: tls.as_ref(),
            grounding,
        });
        let stub_writes = |n: usize, c_args: &[u64]| -> Vec<(u64, u8)> {
            state_call_effects
                .as_ref()
                .map_or_else(Vec::new, |eff| eff.writes(n, c_args))
        };
        let stub = if state_opt.is_some() {
            CallStub {
                result: &state_stub_ret,
                clobber: Some(&state_stub_clobber),
                writes: &stub_writes,
            }
        } else {
            CallStub::returning(&zero_stub)
        };

        let machine = match harness.run_prepared(flow, &stub) {
            Ok(run) => run,
            Err(err) => {
                let (m_term, treat_as_fault) = classify_machine_err(&err, strict_native);
                if treat_as_fault {
                    if hir_term.is_trap_or_fault() {
                        denominators.unknown_runs += 1;
                        let msg = format!(
                            "both sides faulted/trapped: hir={hir_term:?}, machine={m_term:?}"
                        );
                        first_unknown.get_or_insert_with(|| msg.clone());
                        push_witness_bounded(
                            &mut witnesses,
                            &mut non_error_samples,
                            ConcreteWitness {
                                kind: WitnessKind::Unknown,
                                initial_state: InitialStateIdentity {
                                    seed: seed_opt,
                                    run: run_opt,
                                    pointer_bits: slot_bits,
                                    raw_slots,
                                    admissibility: StateAdmissibility::Unknown,
                                    admissibility_reasons: vec![msg.clone()],
                                },
                                args,
                                stub_summary: StubSummary {
                                    calls_intercepted: 0,
                                    call_sites: Vec::new(),
                                    clobbers_enabled: state_opt.is_some(),
                                },
                                frame_audit: None,
                                memory_audit: None,
                                machine_termination: m_term,
                                hir_termination: hir_term,
                                prehir_termination: prehir_term,
                                machine_return: None,
                                hir_return: None,
                                prehir_return: None,
                                detail: Some(msg),
                            },
                        );
                        continue;
                    }
                    denominators.checked_runs += 1;
                    denominators.asymmetric_fault_runs += 1;
                    let detail = format!(
                        "asymmetric fault: machine terminated with {m_term:?} while hir \
                         terminated with {hir_term:?}"
                    );
                    let hir_ret = hir_r.as_ref().ok().and_then(|r| r.ret).map(norm);
                    let prehir_ret = prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm);
                    divergences.push(Divergence {
                        run: run_opt,
                        args: args.clone(),
                        prehir_result: prehir_ret,
                        hir_result: hir_ret,
                        emulator_result: None,
                        detail: Some(detail.clone()),
                    });
                    witnesses.push(ConcreteWitness {
                        kind: WitnessKind::AsymmetricFault,
                        initial_state: InitialStateIdentity {
                            seed: seed_opt,
                            run: run_opt,
                            pointer_bits: slot_bits,
                            raw_slots,
                            admissibility: if strict_native && !interp_used_synthetic {
                                StateAdmissibility::NativeAdmissible
                            } else {
                                StateAdmissibility::SyntheticModel
                            },
                            admissibility_reasons: vec![detail.clone()],
                        },
                        args,
                        stub_summary: StubSummary {
                            calls_intercepted: 0,
                            call_sites: Vec::new(),
                            clobbers_enabled: state_opt.is_some(),
                        },
                        frame_audit: None,
                        memory_audit: None,
                        machine_termination: m_term,
                        hir_termination: hir_term,
                        prehir_termination: prehir_term,
                        machine_return: None,
                        hir_return: hir_ret,
                        prehir_return: prehir_ret,
                        detail: Some(detail),
                    });
                    continue;
                }
                tracing::debug!("emulator run {run_idx} failed: {err}");
                let msg = format!("call failed: {err}");
                first_emulator_error.get_or_insert_with(|| msg.clone());
                denominators.skipped_emulator_errors += 1;
                push_witness_bounded(
                    &mut witnesses,
                    &mut non_error_samples,
                    ConcreteWitness {
                        kind: WitnessKind::SkippedHarnessError,
                        initial_state: InitialStateIdentity {
                            seed: seed_opt,
                            run: run_opt,
                            pointer_bits: slot_bits,
                            raw_slots,
                            admissibility: StateAdmissibility::Unknown,
                            admissibility_reasons: vec![msg.clone()],
                        },
                        args,
                        stub_summary: StubSummary {
                            calls_intercepted: 0,
                            call_sites: Vec::new(),
                            clobbers_enabled: state_opt.is_some(),
                        },
                        frame_audit: None,
                        memory_audit: None,
                        machine_termination: m_term,
                        hir_termination: hir_term,
                        prehir_termination: prehir_term,
                        machine_return: None,
                        hir_return: hir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                        prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                        detail: Some(msg),
                    },
                );
                continue;
            }
        };

        let stub_summary = StubSummary {
            calls_intercepted: machine.calls.len(),
            call_sites: machine.calls.iter().map(|c| c.site).collect(),
            clobbers_enabled: state_opt.is_some(),
        };

        let (machine_term, machine_ret, ended_in_noreturn) = match &machine.end {
            StubbedEnd::Returned(raw) => (TerminationKind::Returned, Some(*raw), false),
            StubbedEnd::NoReturnCall(site) => (TerminationKind::NoReturnCall(*site), None, true),
            StubbedEnd::LeftDecodedCode(pc) => {
                let msg = format!("machine ran {pc:#x}, outside the decoded function");
                first_emulator_error.get_or_insert_with(|| msg.clone());
                denominators.skipped_emulator_errors += 1;
                push_witness_bounded(
                    &mut witnesses,
                    &mut non_error_samples,
                    ConcreteWitness {
                        kind: WitnessKind::SkippedHarnessError,
                        initial_state: InitialStateIdentity {
                            seed: seed_opt,
                            run: run_opt,
                            pointer_bits: slot_bits,
                            raw_slots,
                            admissibility: StateAdmissibility::Unknown,
                            admissibility_reasons: vec![msg.clone()],
                        },
                        args,
                        stub_summary,
                        frame_audit: Some(machine.frame_audit),
                        memory_audit: Some(machine.memory_audit),
                        machine_termination: TerminationKind::LeftDecodedCode(*pc),
                        hir_termination: hir_term,
                        prehir_termination: prehir_term,
                        machine_return: None,
                        hir_return: hir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                        prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                        detail: Some(msg),
                    },
                );
                continue;
            }
            StubbedEnd::Other(CallOutcome::HitBudget) => (TerminationKind::HitBudget, None, false),
            StubbedEnd::Other(CallOutcome::ProcessExited) => {
                if let Some(sig) = machine.exit_signal {
                    (TerminationKind::Fault(format!("signal {sig}")), None, false)
                } else {
                    (TerminationKind::ProcessExited, None, false)
                }
            }
            StubbedEnd::Other(other) => (TerminationKind::Fault(format!("{other:?}")), None, false),
        };

        // Under synthetic `lazy_memory`, `SIGSEGV` (`signal 11`) occurs when a
        // synthetic pointer lands in the emulator's unmappable boundary page
        // (e.g. `u64::MAX`), whereas strict native mode treats every memory
        // fault as a first-class asymmetric fault unless HIR also faulted.
        let synthetic_segfault_skip =
            !strict_native && machine.exit_signal == Some(11) && !hir_term.is_trap_or_fault();
        if synthetic_segfault_skip {
            let msg =
                format!("synthetic unmappable page signal (SIGSEGV) during run {run_idx}; skipped");
            first_emulator_error.get_or_insert_with(|| msg.clone());
            denominators.skipped_emulator_errors += 1;
            push_witness_bounded(
                &mut witnesses,
                &mut non_error_samples,
                ConcreteWitness {
                    kind: WitnessKind::SkippedHarnessError,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility: StateAdmissibility::Unknown,
                        admissibility_reasons: vec![msg.clone()],
                    },
                    args,
                    stub_summary,
                    frame_audit: Some(machine.frame_audit),
                    memory_audit: Some(machine.memory_audit),
                    machine_termination: machine_term,
                    hir_termination: hir_term,
                    prehir_termination: prehir_term,
                    machine_return: None,
                    hir_return: hir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                    prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                    detail: Some(msg),
                },
            );
            continue;
        }

        // Handle non-normal machine termination (`HitBudget`, `ProcessExited`, `Fault`).
        if machine_term.is_trap_or_fault() {
            if hir_term.is_trap_or_fault() {
                denominators.unknown_runs += 1;
                let msg = format!(
                    "both sides trapped/terminated abnormally: machine={machine_term:?}, hir={hir_term:?}"
                );
                first_unknown.get_or_insert_with(|| msg.clone());
                push_witness_bounded(
                    &mut witnesses,
                    &mut non_error_samples,
                    ConcreteWitness {
                        kind: WitnessKind::Unknown,
                        initial_state: InitialStateIdentity {
                            seed: seed_opt,
                            run: run_opt,
                            pointer_bits: slot_bits,
                            raw_slots,
                            admissibility: StateAdmissibility::Unknown,
                            admissibility_reasons: vec![msg.clone()],
                        },
                        args,
                        stub_summary,
                        frame_audit: Some(machine.frame_audit),
                        memory_audit: Some(machine.memory_audit),
                        machine_termination: machine_term,
                        hir_termination: hir_term,
                        prehir_termination: prehir_term,
                        machine_return: None,
                        hir_return: None,
                        prehir_return: None,
                        detail: Some(msg),
                    },
                );
                continue;
            }
            // Machine trapped/hit-budget/exited while HIR finished normally (or read undefined):
            // explicit asymmetric termination divergence!
            denominators.checked_runs += 1;
            denominators.asymmetric_fault_runs += 1;
            let hir_ret = hir_r.as_ref().ok().and_then(|r| r.ret).map(norm);
            let prehir_ret = prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm);
            let detail = format!(
                "asymmetric termination: machine stopped with {machine_term:?} while hir \
                 finished with {hir_term:?}"
            );
            divergences.push(Divergence {
                run: run_opt,
                args: args.clone(),
                prehir_result: prehir_ret,
                hir_result: hir_ret,
                emulator_result: None,
                detail: Some(detail.clone()),
            });
            let (admissibility, reasons) = classify_run_admissibility(
                strict_native,
                &machine.calls,
                &machine.memory_audit,
                &machine.frame_audit,
                interp_used_synthetic,
                false,
            );
            witnesses.push(ConcreteWitness {
                kind: WitnessKind::AsymmetricFault,
                initial_state: InitialStateIdentity {
                    seed: seed_opt,
                    run: run_opt,
                    pointer_bits: slot_bits,
                    raw_slots,
                    admissibility,
                    admissibility_reasons: reasons,
                },
                args,
                stub_summary,
                frame_audit: Some(machine.frame_audit),
                memory_audit: Some(machine.memory_audit),
                machine_termination: machine_term,
                hir_termination: hir_term,
                prehir_termination: prehir_term,
                machine_return: None,
                hir_return: hir_ret,
                prehir_return: prehir_ret,
                detail: Some(detail),
            });
            continue;
        }

        // HIR trapped while machine returned cleanly -> asymmetric trap divergence!
        if let TerminationKind::Trap(msg) = &hir_term {
            denominators.checked_runs += 1;
            denominators.asymmetric_fault_runs += 1;
            let emulator_val = machine_ret.map(|raw| norm(raw as i64));
            let detail = format!("asymmetric trap: hir trapped ({msg}) while machine returned");
            divergences.push(Divergence {
                run: run_opt,
                args: args.clone(),
                prehir_result: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                hir_result: None,
                emulator_result: emulator_val,
                detail: Some(detail.clone()),
            });
            let (admissibility, reasons) = classify_run_admissibility(
                strict_native,
                &machine.calls,
                &machine.memory_audit,
                &machine.frame_audit,
                interp_used_synthetic,
                false,
            );
            witnesses.push(ConcreteWitness {
                kind: WitnessKind::AsymmetricFault,
                initial_state: InitialStateIdentity {
                    seed: seed_opt,
                    run: run_opt,
                    pointer_bits: slot_bits,
                    raw_slots,
                    admissibility,
                    admissibility_reasons: reasons,
                },
                args,
                stub_summary,
                frame_audit: Some(machine.frame_audit),
                memory_audit: Some(machine.memory_audit),
                machine_termination: machine_term,
                hir_termination: hir_term,
                prehir_termination: prehir_term,
                machine_return: emulator_val,
                hir_return: None,
                prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                detail: Some(detail),
            });
            continue;
        }

        let tree_undefined = hir_r
            .as_ref()
            .err()
            .and_then(|err| err.downcast_ref::<UndefinedRead>())
            .cloned();
        match (&tree_undefined, machine.undefined_frame_read) {
            (Some(read), Some(at)) => {
                denominators.unknown_runs += 1;
                let msg = format!(
                    "hir: {read}; the machine read frame byte {at:#x}, which nothing wrote"
                );
                first_unknown.get_or_insert_with(|| msg.clone());
                push_witness_bounded(
                    &mut witnesses,
                    &mut non_error_samples,
                    ConcreteWitness {
                        kind: WitnessKind::Unknown,
                        initial_state: InitialStateIdentity {
                            seed: seed_opt,
                            run: run_opt,
                            pointer_bits: slot_bits,
                            raw_slots,
                            admissibility: StateAdmissibility::Unknown,
                            admissibility_reasons: vec![msg.clone()],
                        },
                        args,
                        stub_summary,
                        frame_audit: Some(machine.frame_audit),
                        memory_audit: Some(machine.memory_audit),
                        machine_termination: machine_term,
                        hir_termination: hir_term,
                        prehir_termination: prehir_term,
                        machine_return: machine_ret.map(|raw| norm(raw as i64)),
                        hir_return: None,
                        prehir_return: prehir_r.as_ref().ok().and_then(|r| r.ret).map(norm),
                        detail: Some(msg),
                    },
                );
                continue;
            }
            (Some(read), None) => {
                denominators.checked_runs += 1;
                let emulator_val = machine_ret.map(|raw| norm(raw as i64));
                let detail = format!("hir: {read}; the machine read only what the run defined");
                divergences.push(Divergence {
                    run: run_opt,
                    args: args.clone(),
                    prehir_result: None,
                    hir_result: None,
                    emulator_result: emulator_val,
                    detail: Some(detail.clone()),
                });
                let (admissibility, reasons) = classify_run_admissibility(
                    strict_native,
                    &machine.calls,
                    &machine.memory_audit,
                    &machine.frame_audit,
                    interp_used_synthetic,
                    false,
                );
                witnesses.push(ConcreteWitness {
                    kind: WitnessKind::Divergence,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility,
                        admissibility_reasons: reasons,
                    },
                    args,
                    stub_summary,
                    frame_audit: Some(machine.frame_audit),
                    memory_audit: Some(machine.memory_audit),
                    machine_termination: machine_term,
                    hir_termination: hir_term,
                    prehir_termination: prehir_term,
                    machine_return: emulator_val,
                    hir_return: None,
                    prehir_return: None,
                    detail: Some(detail),
                });
                continue;
            }
            (None, _) => {}
        }

        let hir_run = hir_r.as_ref().expect("handled interpreter errors above");
        let prehir_run = prehir_r.as_ref().ok();
        let emulator_val = machine_ret.map(|raw| norm(raw as i64));
        let prehir_val = prehir_run.and_then(|run| run.ret).map(norm);
        let hir_val = hir_run.ret.map(norm);

        let mut detail = None;
        let mut suspect: Option<String> = None;
        let escaped_ranges = escaped_pointer_ranges(&machine.frame_audit.escaped_frame_pointers);

        let sides = std::iter::once(("hir", hir_run)).chain(prehir_run.map(|run| ("prehir", run)));
        for (side, run) in sides {
            if detail.is_some() {
                break;
            }
            let obs_tree = observable_tree_writes(&machine.frame_audit.spec, run, &escaped_ranges);
            let obs_machine =
                observable_machine_writes(&machine.writes, &machine.all_writes, &escaped_ranges);
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
                        format!(
                            "{side}: the machine stopped at a call that does not return; the tree went on"
                        )
                    } else {
                        format!(
                            "{side}: the tree stopped at a call that does not return; the machine went on"
                        )
                    }
                })
            })
            .or_else(|| {
                compare_writes(side, &obs_tree, &obs_machine, &no_placed, |a| {
                    harness
                        .baseline_byte(a)
                        .or_else(|| initial_mem_map.get(&a).copied())
                        .or_else(|| state_opt.map(|s| s.memory_byte(a)))
                })
            });
        }

        let returns_value = declares_value || hir_val.is_some() || prehir_val.is_some();
        let return_differs = !ended_in_noreturn
            && returns_value
            && (hir_val != emulator_val || prehir_run.is_some() && prehir_val != emulator_val);

        if return_differs || detail.is_some() {
            denominators.checked_runs += 1;
            denominators.diverged_runs += 1;
            divergences.push(Divergence {
                run: run_opt,
                args: args.clone(),
                prehir_result: prehir_val,
                hir_result: hir_val,
                emulator_result: emulator_val,
                detail: detail.clone(),
            });
            let (admissibility, reasons) = classify_run_admissibility(
                strict_native,
                &machine.calls,
                &machine.memory_audit,
                &machine.frame_audit,
                interp_used_synthetic,
                false,
            );
            witnesses.push(ConcreteWitness {
                kind: WitnessKind::Divergence,
                initial_state: InitialStateIdentity {
                    seed: seed_opt,
                    run: run_opt,
                    pointer_bits: slot_bits,
                    raw_slots,
                    admissibility,
                    admissibility_reasons: reasons,
                },
                args,
                stub_summary,
                frame_audit: Some(machine.frame_audit),
                memory_audit: Some(machine.memory_audit),
                machine_termination: machine_term,
                hir_termination: hir_term,
                prehir_termination: prehir_term,
                machine_return: emulator_val,
                hir_return: hir_val,
                prehir_return: prehir_val,
                detail,
            });
            continue;
        }

        if let Some(suspect_msg) = suspect {
            denominators.checked_runs += 1;
            denominators.suspect_runs += 1;
            if suspects.len() < MAX_REPORTED_DIVERGENCES {
                suspects.push(Divergence {
                    run: run_opt,
                    args: args.clone(),
                    prehir_result: prehir_val,
                    hir_result: hir_val,
                    emulator_result: emulator_val,
                    detail: Some(suspect_msg.clone()),
                });
            }
            push_witness_bounded(
                &mut witnesses,
                &mut non_error_samples,
                ConcreteWitness {
                    kind: WitnessKind::Suspect,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility: StateAdmissibility::SyntheticModel,
                        admissibility_reasons: vec![suspect_msg.clone()],
                    },
                    args,
                    stub_summary,
                    frame_audit: Some(machine.frame_audit),
                    memory_audit: Some(machine.memory_audit),
                    machine_termination: machine_term,
                    hir_termination: hir_term,
                    prehir_termination: prehir_term,
                    machine_return: emulator_val,
                    hir_return: hir_val,
                    prehir_return: prehir_val,
                    detail: Some(suspect_msg),
                },
            );
            continue;
        }

        // Frame exclusion must be justified before counting a positive pass as agreed!
        if let FrameExclusionJustification::Unjustified { reasons } =
            &machine.frame_audit.justification
        {
            denominators.unknown_runs += 1;
            denominators.unjustified_frame_runs += 1;
            let msg = format!("unjustified frame exclusion: {}", reasons.join("; "));
            first_unknown.get_or_insert_with(|| msg.clone());
            push_witness_bounded(
                &mut witnesses,
                &mut non_error_samples,
                ConcreteWitness {
                    kind: WitnessKind::Unknown,
                    initial_state: InitialStateIdentity {
                        seed: seed_opt,
                        run: run_opt,
                        pointer_bits: slot_bits,
                        raw_slots,
                        admissibility: StateAdmissibility::Unknown,
                        admissibility_reasons: reasons.clone(),
                    },
                    args,
                    stub_summary,
                    frame_audit: Some(machine.frame_audit),
                    memory_audit: Some(machine.memory_audit),
                    machine_termination: machine_term,
                    hir_termination: hir_term,
                    prehir_termination: prehir_term,
                    machine_return: emulator_val,
                    hir_return: hir_val,
                    prehir_return: prehir_val,
                    detail: Some(msg),
                },
            );
            continue;
        }

        let (admissibility, reasons) = classify_run_admissibility(
            strict_native,
            &machine.calls,
            &machine.memory_audit,
            &machine.frame_audit,
            interp_used_synthetic,
            machine.undefined_frame_read.is_some(),
        );
        denominators.checked_runs += 1;
        denominators.agreed_runs += 1;
        match admissibility {
            StateAdmissibility::NativeAdmissible => denominators.native_admissible_runs += 1,
            StateAdmissibility::SyntheticModel => denominators.synthetic_model_runs += 1,
            StateAdmissibility::Unknown => {}
        }
        push_witness_bounded(
            &mut witnesses,
            &mut non_error_samples,
            ConcreteWitness {
                kind: WitnessKind::AgreedSample,
                initial_state: InitialStateIdentity {
                    seed: seed_opt,
                    run: run_opt,
                    pointer_bits: slot_bits,
                    raw_slots,
                    admissibility,
                    admissibility_reasons: reasons,
                },
                args,
                stub_summary,
                frame_audit: Some(machine.frame_audit),
                memory_audit: Some(machine.memory_audit),
                machine_termination: machine_term,
                hir_termination: hir_term,
                prehir_termination: prehir_term,
                machine_return: emulator_val,
                hir_return: hir_val,
                prehir_return: prehir_val,
                detail: None,
            },
        );
    }

    if denominators.skipped_emulator_errors > 0 {
        unresolved_effects.push(UnresolvedEffect {
            kind: UnresolvedEffectKind::Environment,
            description: format!(
                "{} of {} planned runs hit harness/mapping limits{}",
                denominators.skipped_emulator_errors,
                denominators.planned_runs,
                first_emulator_error
                    .as_deref()
                    .map(|e| format!(" (first: {e})"))
                    .unwrap_or_default()
            ),
        });
    }
    if denominators.skipped_interpreter_errors > 0 {
        unresolved_effects.push(UnresolvedEffect {
            kind: UnresolvedEffectKind::Other,
            description: format!(
                "{} of {} planned runs hit unsupported interpreter constructs{}",
                denominators.skipped_interpreter_errors,
                denominators.planned_runs,
                first_interpreter_error
                    .as_deref()
                    .map(|e| format!(" (first: {e})"))
                    .unwrap_or_default()
            ),
        });
    }
    if denominators.prehir_skipped_runs > 0 {
        unresolved_effects.push(UnresolvedEffect {
            kind: UnresolvedEffectKind::Other,
            description: format!(
                "PreHIR could not evaluate {} of {} runs (HIR still checked against machine)",
                denominators.prehir_skipped_runs, denominators.planned_runs
            ),
        });
    }
    if denominators.unjustified_frame_runs > 0 {
        unresolved_effects.push(UnresolvedEffect {
            kind: UnresolvedEffectKind::MemoryAlias,
            description: format!(
                "{} run(s) had unjustified private-frame exclusion (escape, SP delta, or boundary overlap)",
                denominators.unjustified_frame_runs
            ),
        });
    }

    let outcome = if !divergences.is_empty() {
        VerifyOutcome::Diverged(divergences)
    } else if !suspects.is_empty() {
        VerifyOutcome::Suspect(suspects)
    } else if denominators.checked_runs == 0 && denominators.unknown_runs > 0 {
        VerifyOutcome::Inconclusive {
            reason: format!(
                "both sides read state nothing defined, or frame exclusion was unjustified, in {} runs; first: {}",
                denominators.unknown_runs,
                first_unknown.unwrap_or_default()
            ),
        }
    } else if denominators.checked_runs == 0 {
        if denominators.skipped_emulator_errors > 0 {
            VerifyOutcome::Unsupported(UnsupportedReason::Harness(
                first_emulator_error.unwrap_or_default(),
            ))
        } else {
            VerifyOutcome::Unsupported(UnsupportedReason::Interpreter(
                first_interpreter_error.unwrap_or_else(|| "no runs".to_string()),
            ))
        }
    } else {
        VerifyOutcome::Equivalent {
            checked: denominators.checked_runs,
        }
    };

    GroundTruthReport {
        outcome,
        policy,
        observation_scope,
        denominators,
        assumptions,
        unresolved_effects,
        witnesses,
    }
}

fn classify_run_admissibility(
    strict_native: bool,
    calls: &[ObservedCall],
    memory_audit: &MemoryAccessAudit,
    frame_audit: &FrameExclusionAudit,
    interp_used_synthetic: bool,
    machine_undefined_read: bool,
) -> (StateAdmissibility, Vec<String>) {
    let mut synthetic_reasons = Vec::new();
    if memory_audit.used_synthetic_memory() {
        synthetic_reasons.push(format!(
            "emulator accessed {} synthetic/unmapped page(s)",
            memory_audit.total_synthetic_pages
        ));
    }
    if interp_used_synthetic {
        synthetic_reasons
            .push("interpreter read bytes from synthetic InitialState lazy memory".to_string());
    }
    if !calls.is_empty() {
        synthetic_reasons.push(format!(
            "{} call(s) intercepted via deterministic CallStub",
            calls.len()
        ));
    }
    if !frame_audit.clobbered_callee_saved.is_empty() {
        synthetic_reasons.push(format!(
            "callee-saved registers modified across call: {}",
            frame_audit.clobbered_callee_saved.join(", ")
        ));
    }
    if machine_undefined_read {
        synthetic_reasons.push("machine read undefined private frame bytes".to_string());
    }

    if !synthetic_reasons.is_empty() {
        return (StateAdmissibility::SyntheticModel, synthetic_reasons);
    }
    let mut notes = vec![if strict_native {
        "strict native run: mapped pages only, no lazy memory, no call stubs, balanced SP/ABI"
            .to_string()
    } else {
        "run accessed mapped image/stack only with no stubbed calls or lazy unmapped pages"
            .to_string()
    }];
    if let FrameExclusionJustification::EscapedToCallee { count } = frame_audit.justification {
        notes.push(format!("{count} private-frame pointer(s) passed to callee"));
    }
    (StateAdmissibility::NativeAdmissible, notes)
}

fn escaped_pointer_ranges(escaped: &[EscapedFramePointer]) -> Vec<std::ops::Range<u64>> {
    escaped
        .iter()
        .filter_map(|esc| match esc {
            EscapedFramePointer::Returned(ptr)
            | EscapedFramePointer::StoredExternally { target: ptr, .. } => {
                Some(*ptr..ptr.saturating_add(ESCAPED_FRAME_CHECK_WINDOW))
            }
            EscapedFramePointer::PassedToCall { .. } => None,
        })
        .collect()
}

fn observable_tree_writes(
    spec: &FrameExclusionSpec,
    run: &InterpRun,
    escaped_ranges: &[std::ops::Range<u64>],
) -> BTreeMap<u64, u8> {
    run.writes
        .iter()
        .filter(|(addr, _)| {
            !spec.is_excluded(**addr) || escaped_ranges.iter().any(|r| r.contains(addr))
        })
        .map(|(addr, val)| (*addr, *val))
        .collect()
}

fn observable_machine_writes(
    machine_writes: &BTreeMap<u64, u8>,
    machine_all_writes: &BTreeMap<u64, u8>,
    escaped_ranges: &[std::ops::Range<u64>],
) -> BTreeMap<u64, u8> {
    if escaped_ranges.is_empty() {
        return machine_writes.clone();
    }
    let mut out = machine_writes.clone();
    for (&addr, &val) in machine_all_writes {
        if escaped_ranges.iter().any(|r| r.contains(&addr)) {
            out.insert(addr, val);
        }
    }
    out
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

    #[test]
    fn ground_truth_report_projects_to_trace_validated_never_proven() {
        let foundation = FoundationIdentity {
            model_version: 1,
            function_address: 0x1000,
            block_count: 1,
            op_count: 2,
            digest: 0x1234,
        };
        let report = GroundTruthReport {
            outcome: VerifyOutcome::Equivalent { checked: 16 },
            policy: InitialStatePolicy::synthetic_default(),
            observation_scope: ConcreteObservationScope::for_function(32, true),
            denominators: GroundTruthDenominators {
                planned_runs: 20,
                attempted_runs: 20,
                checked_runs: 16,
                agreed_runs: 16,
                synthetic_model_runs: 12,
                native_admissible_runs: 4,
                skipped_emulator_errors: 4,
                ..GroundTruthDenominators::default()
            },
            assumptions: build_report_assumptions(
                &InitialStatePolicy::synthetic_default(),
                &Grounding::default(),
            ),
            unresolved_effects: Vec::new(),
            witnesses: Vec::new(),
        };
        let assurance = report
            .to_assurance_report(foundation)
            .expect("assurance projection");
        assert!(!assurance.is_proven());
        assert!(!assurance.is_unconditionally_proven());
        assert!(assurance.is_sampled_only());
        assert!(assurance.is_trace_only());
        assert_eq!(
            assurance.band(),
            crate::product::AssuranceBand::TraceValidated
        );
        assert_eq!(report.native_admissible_agreements(), 4);
        assert_eq!(report.synthetic_agreements(), 12);
    }
}
