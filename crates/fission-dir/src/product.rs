//! Public contracts for the P-code-rooted DIR product.
//!
//! DIR is a sibling reconstruction path to HIR.  A [`DirArtifact`] therefore
//! names the immutable P-code foundation it was checked against and carries
//! explicit evidence about what was observed.  PreHIR or HIR may seed a
//! candidate, but neither is the semantic oracle.

use std::hash::{Hash, Hasher};

use fission_pcode::{PcodeFunction, PcodeValidationError, ValidatedPcodeFunction, Varnode};

/// Version of the canonical byte stream used for [`FoundationIdentity::digest`].
pub const FOUNDATION_IDENTITY_VERSION: u32 = 1;

/// Stable identity of the exact validated P-code function used as the oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FoundationIdentity {
    pub model_version: u32,
    pub function_address: u64,
    pub block_count: u32,
    pub op_count: u32,
    pub digest: u64,
}

/// Immutable, shape-validated P-code admitted to the DIR pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcodeFoundation {
    identity: FoundationIdentity,
    pcode: ValidatedPcodeFunction,
}

impl PcodeFoundation {
    pub fn new(function_address: u64, pcode: PcodeFunction) -> Result<Self, PcodeValidationError> {
        Ok(Self::from_validated(
            function_address,
            pcode.into_validated()?,
        ))
    }

    #[must_use]
    pub fn from_validated(function_address: u64, pcode: ValidatedPcodeFunction) -> Self {
        let identity = foundation_identity(function_address, pcode.as_raw());
        Self { identity, pcode }
    }

    #[must_use]
    pub fn identity(&self) -> FoundationIdentity {
        self.identity
    }

    #[must_use]
    pub fn pcode(&self) -> &ValidatedPcodeFunction {
        &self.pcode
    }

    #[must_use]
    pub fn into_pcode(self) -> ValidatedPcodeFunction {
        self.pcode
    }
}

/// How a readable reconstruction candidate was produced.
///
/// Only `PcodeNative` is the intended steady-state DIR route.  The two seeded
/// variants make transitional reuse auditable rather than silently turning HIR
/// or PreHIR into the oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSource {
    PcodeNative,
    PreHirSeeded,
    HirSeeded,
    External { producer: String },
}

/// Executable meaning carried by a reconstruction candidate.
///
/// Text alone is deliberately insufficient for validation: a verifier must
/// consume the same typed candidate semantics that produced the pseudocode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSemantics {
    PcodeNativeRegion(PcodeNativeRegion),
    /// Transitional candidates may still be displayed, but cannot be admitted
    /// by the P-code-native verifier.
    Unmodeled,
}

/// A contiguous operation slice in one validated P-code basic block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcodeRegionSelection {
    pub block_index: u32,
    pub first_op: usize,
    pub op_count: usize,
    pub output: ObservedLocation,
}

/// One live-in value consumed by a P-code-native region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcodeNativeInput {
    pub name: String,
    pub location: ObservedLocation,
}

/// Pure expression reconstructed from a supported P-code region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcodeNativeExpr {
    Input {
        index: usize,
        bits: u32,
    },
    Constant {
        value: u64,
        bits: u32,
    },
    Binary {
        op: PcodeNativeBinaryOp,
        left: Box<PcodeNativeExpr>,
        right: Box<PcodeNativeExpr>,
        bits: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcodeNativeBinaryOp {
    Add,
    Subtract,
    BitAnd,
    BitOr,
    BitXor,
    Equal,
    NotEqual,
    UnsignedLess,
    UnsignedLessEqual,
}

/// Executable candidate for one side-effect-free P-code region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcodeNativeRegion {
    pub selection: PcodeRegionSelection,
    /// Address spaces asserted by the caller to be register/unique-style
    /// value spaces rather than memory.
    pub value_space_ids: Vec<u64>,
    pub inputs: Vec<PcodeNativeInput>,
    pub expression: PcodeNativeExpr,
}

/// A readable reconstruction proposed for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirCandidate {
    pub candidate_id: String,
    pub source: CandidateSource,
    pub pseudocode: String,
    pub semantics: CandidateSemantics,
}

/// A typed register, temporary, or other non-memory location visible at exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservedLocation {
    pub space_id: u64,
    pub offset: u64,
    pub size: u32,
}

/// A memory interval included in the observational-equivalence contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObservedMemoryRange {
    pub space_id: u64,
    pub start: u64,
    pub length: u64,
}

/// How calls participate in the observable behavior of a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallObservation {
    Ignored,
    OrderOnly,
    TypedEffects,
}

/// Explicit boundary of what equivalence means for one DIR artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationScope {
    pub return_value: bool,
    pub exit_kind: bool,
    pub live_out_locations: Vec<ObservedLocation>,
    pub memory_ranges: Vec<ObservedMemoryRange>,
    pub calls: CallObservation,
    pub traps: bool,
}

impl Default for ObservationScope {
    fn default() -> Self {
        Self {
            return_value: true,
            exit_kind: true,
            live_out_locations: Vec::new(),
            memory_ranges: Vec::new(),
            calls: CallObservation::TypedEffects,
            traps: true,
        }
    }
}

impl ObservationScope {
    /// Observation contract supported by the first P-code-native region
    /// verifier: one live-out value and no implicit control or memory effects.
    #[must_use]
    pub fn location_only(location: ObservedLocation) -> Self {
        Self {
            return_value: false,
            exit_kind: false,
            live_out_locations: vec![location],
            memory_ranges: Vec::new(),
            calls: CallObservation::Ignored,
            traps: false,
        }
    }
}

/// A condition under which verification evidence is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationAssumption {
    pub kind: AssumptionKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssumptionKind {
    InputRange,
    Memory,
    ExternalCall,
    CallingConvention,
    Environment,
    LoopBound,
    Other,
}

/// Resource bounds applied by concrete/symbolic validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidationBudget {
    pub max_paths: Option<u64>,
    pub max_traces: Option<u64>,
    pub max_loop_unroll: Option<u32>,
    pub solver_timeout_ms: Option<u64>,
}

/// An effect that prevented the verifier from making a stronger statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedEffect {
    pub kind: UnresolvedEffectKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedEffectKind {
    ExternalCall,
    IndirectControlFlow,
    MemoryAlias,
    UnsupportedOpcode,
    Environment,
    SolverTimeout,
    PathBudget,
    Other,
}

/// One input/state witness showing an observable difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirCounterexample {
    pub inputs: Vec<(String, u64)>,
    pub foundation_observation: String,
    pub candidate_observation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationVerdict {
    Equivalent,
    Counterexample,
    Inconclusive,
    Unsupported,
}

/// Strength of positive evidence.  This is separate from the verdict so an
/// `Equivalent` result cannot accidentally erase path/trace bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStrength {
    Universal,
    Conditional,
    BoundedPaths,
    Traces,
    None,
}

/// Evidence and limitations emitted by a verifier backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationEvidence {
    pub strength: EvidenceStrength,
    pub budget: ValidationBudget,
    pub explored_paths: u64,
    pub executed_traces: u64,
    pub assumptions: Vec<ValidationAssumption>,
    pub unresolved_effects: Vec<UnresolvedEffect>,
    pub counterexample: Option<DirCounterexample>,
    pub detail: String,
}

impl ValidationEvidence {
    #[must_use]
    pub fn none(detail: impl Into<String>) -> Self {
        Self {
            strength: EvidenceStrength::None,
            budget: ValidationBudget::default(),
            explored_paths: 0,
            executed_traces: 0,
            assumptions: Vec::new(),
            unresolved_effects: Vec::new(),
            counterexample: None,
            detail: detail.into(),
        }
    }
}

/// User-facing assurance band derived from verdict, evidence, and limitations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceBand {
    Proven,
    ConditionallyProven,
    PathValidated,
    TraceValidated,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssuranceContractError {
    #[error("equivalent verdict requires positive evidence")]
    EquivalentWithoutPositiveEvidence,
    #[error("non-equivalent verdict cannot carry positive equivalence evidence")]
    PositiveEvidenceWithoutEquivalence,
    #[error("counterexample verdict requires a counterexample witness")]
    MissingCounterexample,
    #[error("only a counterexample verdict may carry a counterexample witness")]
    UnexpectedCounterexample,
    #[error("bounded-path evidence requires at least one explored path")]
    EmptyPathEvidence,
    #[error("trace evidence requires at least one executed trace")]
    EmptyTraceEvidence,
}

/// Complete validation statement for one candidate/foundation pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssuranceReport {
    foundation: FoundationIdentity,
    scope: ObservationScope,
    verdict: ValidationVerdict,
    evidence: ValidationEvidence,
}

impl AssuranceReport {
    pub fn new(
        foundation: FoundationIdentity,
        scope: ObservationScope,
        verdict: ValidationVerdict,
        evidence: ValidationEvidence,
    ) -> Result<Self, AssuranceContractError> {
        match (verdict, evidence.strength) {
            (ValidationVerdict::Equivalent, EvidenceStrength::None) => {
                return Err(AssuranceContractError::EquivalentWithoutPositiveEvidence);
            }
            (ValidationVerdict::Equivalent, _) | (_, EvidenceStrength::None) => {}
            (_, _) => return Err(AssuranceContractError::PositiveEvidenceWithoutEquivalence),
        }

        match (verdict, evidence.counterexample.is_some()) {
            (ValidationVerdict::Counterexample, false) => {
                return Err(AssuranceContractError::MissingCounterexample);
            }
            (ValidationVerdict::Counterexample, true) | (_, false) => {}
            (_, true) => return Err(AssuranceContractError::UnexpectedCounterexample),
        }

        if evidence.strength == EvidenceStrength::BoundedPaths && evidence.explored_paths == 0 {
            return Err(AssuranceContractError::EmptyPathEvidence);
        }
        if evidence.strength == EvidenceStrength::Traces && evidence.executed_traces == 0 {
            return Err(AssuranceContractError::EmptyTraceEvidence);
        }

        Ok(Self {
            foundation,
            scope,
            verdict,
            evidence,
        })
    }

    #[must_use]
    pub fn foundation(&self) -> FoundationIdentity {
        self.foundation
    }

    #[must_use]
    pub fn scope(&self) -> &ObservationScope {
        &self.scope
    }

    #[must_use]
    pub fn verdict(&self) -> ValidationVerdict {
        self.verdict
    }

    #[must_use]
    pub fn evidence(&self) -> &ValidationEvidence {
        &self.evidence
    }

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
            EvidenceStrength::Universal | EvidenceStrength::Conditional => {
                AssuranceBand::ConditionallyProven
            }
            EvidenceStrength::BoundedPaths => AssuranceBand::PathValidated,
            EvidenceStrength::Traces => AssuranceBand::TraceValidated,
            EvidenceStrength::None => AssuranceBand::Unverified,
        }
    }
}

/// The output unit of the independent DIR product path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirArtifact {
    pub candidate: DirCandidate,
    pub assurance: AssuranceReport,
}

impl DirArtifact {
    /// Admit a candidate before any equivalence backend has run.
    ///
    /// This deliberately returns `Unverified`; candidate generation and
    /// validation are separate operations.
    #[must_use]
    pub fn unverified(
        foundation: &PcodeFoundation,
        candidate: DirCandidate,
        scope: ObservationScope,
    ) -> Self {
        Self {
            candidate,
            assurance: AssuranceReport {
                foundation: foundation.identity(),
                scope,
                verdict: ValidationVerdict::Inconclusive,
                evidence: ValidationEvidence::none("no DIR validation backend has run"),
            },
        }
    }
}

/// Candidate-generation half of the independent DIR path.
///
/// Implementations receive only the semantic foundation.  Supplying an HIR or
/// PreHIR seed is an explicit implementation choice reflected in each
/// candidate's [`CandidateSource`], not an implicit pipeline dependency.
pub trait DirReconstructor {
    type Error;

    fn reconstruct(&self, foundation: &PcodeFoundation) -> Result<Vec<DirCandidate>, Self::Error>;
}

/// Concrete/symbolic equivalence half of the independent DIR path.
pub trait DirVerifier {
    fn verify(
        &self,
        foundation: &PcodeFoundation,
        candidate: &DirCandidate,
        scope: &ObservationScope,
        budget: ValidationBudget,
    ) -> (ValidationVerdict, ValidationEvidence);
}

/// Orchestrates reconstruction and validation without routing through HIR.
pub struct DirPipeline<R, V> {
    reconstructor: R,
    verifier: V,
}

impl<R, V> DirPipeline<R, V> {
    #[must_use]
    pub fn new(reconstructor: R, verifier: V) -> Self {
        Self {
            reconstructor,
            verifier,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirPipelineError<E> {
    Reconstruction(E),
    InvalidEvidence {
        candidate_id: String,
        error: AssuranceContractError,
    },
}

impl<R, V> DirPipeline<R, V>
where
    R: DirReconstructor,
    V: DirVerifier,
{
    /// Reconstruct and validate every proposed candidate against the same
    /// immutable foundation. Candidate ordering is preserved so selection can
    /// be deterministic in a later policy layer.
    pub fn run(
        &self,
        foundation: &PcodeFoundation,
        scope: &ObservationScope,
        budget: ValidationBudget,
    ) -> Result<Vec<DirArtifact>, DirPipelineError<R::Error>> {
        let candidates = self
            .reconstructor
            .reconstruct(foundation)
            .map_err(DirPipelineError::Reconstruction)?;
        let mut artifacts = Vec::with_capacity(candidates.len());

        for candidate in candidates {
            let (verdict, evidence) = self.verifier.verify(foundation, &candidate, scope, budget);
            let assurance =
                AssuranceReport::new(foundation.identity(), scope.clone(), verdict, evidence)
                    .map_err(|error| DirPipelineError::InvalidEvidence {
                        candidate_id: candidate.candidate_id.clone(),
                        error,
                    })?;
            artifacts.push(DirArtifact {
                candidate,
                assurance,
            });
        }

        Ok(artifacts)
    }
}

fn foundation_identity(function_address: u64, pcode: &PcodeFunction) -> FoundationIdentity {
    let mut hasher = Fnv1a64::default();
    FOUNDATION_IDENTITY_VERSION.hash(&mut hasher);
    function_address.hash(&mut hasher);
    pcode.blocks.len().hash(&mut hasher);

    let mut op_count = 0_u32;
    for block in &pcode.blocks {
        block.index.hash(&mut hasher);
        block.start_address.hash(&mut hasher);
        block.successors.hash(&mut hasher);
        block.ops.len().hash(&mut hasher);
        op_count = op_count.saturating_add(block.ops.len() as u32);

        for op in &block.ops {
            op.seq_num.hash(&mut hasher);
            op.opcode.hash(&mut hasher);
            op.address.hash(&mut hasher);
            hash_varnode(op.output.as_ref(), &mut hasher);
            op.inputs.len().hash(&mut hasher);
            for input in &op.inputs {
                hash_varnode(Some(input), &mut hasher);
            }
            op.asm_mnemonic.hash(&mut hasher);
        }
    }

    FoundationIdentity {
        model_version: FOUNDATION_IDENTITY_VERSION,
        function_address,
        block_count: pcode.blocks.len() as u32,
        op_count,
        digest: hasher.finish(),
    }
}

fn hash_varnode(varnode: Option<&Varnode>, hasher: &mut Fnv1a64) {
    match varnode {
        Some(varnode) => {
            1_u8.hash(hasher);
            varnode.space_id.hash(hasher);
            varnode.offset.hash(hasher);
            varnode.size.hash(hasher);
            varnode.is_constant.hash(hasher);
            varnode.constant_val.hash(hasher);
        }
        None => 0_u8.hash(hasher),
    }
}

/// Fixed FNV-1a avoids the implementation-defined/randomized state of
/// `DefaultHasher`; the stream itself is versioned above.
struct Fnv1a64(u64);

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl Hasher for Fnv1a64 {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use fission_pcode::{PcodeBasicBlock, PcodeOp, PcodeOpcode, Varnode};

    use super::*;

    fn valid_function(constant: i64) -> PcodeFunction {
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0,
                        size: 8,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(constant, 8)],
                    asm_mnemonic: Some("mov".to_string()),
                }],
            }],
        }
    }

    #[test]
    fn foundation_rejects_invalid_pcode_shape() {
        let mut function = valid_function(1);
        function.blocks[0].ops[0].output = None;
        assert!(PcodeFoundation::new(0x1000, function).is_err());
    }

    #[test]
    fn foundation_identity_is_deterministic_and_content_sensitive() {
        let left = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let same = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let changed = PcodeFoundation::new(0x1000, valid_function(2)).unwrap();

        assert_eq!(left.identity(), same.identity());
        assert_ne!(left.identity().digest, changed.identity().digest);
    }

    #[test]
    fn assurance_band_never_promotes_an_unverified_candidate() {
        let foundation = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let artifact = DirArtifact::unverified(
            &foundation,
            DirCandidate {
                candidate_id: "candidate-0".to_string(),
                source: CandidateSource::PcodeNative,
                pseudocode: "return 1;".to_string(),
                semantics: CandidateSemantics::Unmodeled,
            },
            ObservationScope::default(),
        );

        assert_eq!(artifact.assurance.band(), AssuranceBand::Unverified);
    }

    #[test]
    fn assurance_bands_preserve_evidence_strength() {
        let foundation = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let evidence =
            |strength, explored_paths, executed_traces, assumptions| ValidationEvidence {
                strength,
                budget: ValidationBudget::default(),
                explored_paths,
                executed_traces,
                assumptions,
                unresolved_effects: Vec::new(),
                counterexample: None,
                detail: String::new(),
            };
        let report = |evidence| {
            AssuranceReport::new(
                foundation.identity(),
                ObservationScope::default(),
                ValidationVerdict::Equivalent,
                evidence,
            )
            .unwrap()
        };

        let proven = report(evidence(EvidenceStrength::Universal, 0, 0, Vec::new()));
        assert_eq!(proven.band(), AssuranceBand::Proven);

        let conditional = report(evidence(
            EvidenceStrength::Universal,
            0,
            0,
            vec![ValidationAssumption {
                kind: AssumptionKind::InputRange,
                description: "len <= 4096".to_string(),
            }],
        ));
        assert_eq!(conditional.band(), AssuranceBand::ConditionallyProven);

        let paths = report(evidence(EvidenceStrength::BoundedPaths, 8, 0, Vec::new()));
        assert_eq!(paths.band(), AssuranceBand::PathValidated);

        let traces = report(evidence(EvidenceStrength::Traces, 0, 4, Vec::new()));
        assert_eq!(traces.band(), AssuranceBand::TraceValidated);
    }

    #[test]
    fn assurance_contract_rejects_contradictory_backend_results() {
        let foundation = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let result = AssuranceReport::new(
            foundation.identity(),
            ObservationScope::default(),
            ValidationVerdict::Equivalent,
            ValidationEvidence::none("backend forgot evidence"),
        );
        assert_eq!(
            result,
            Err(AssuranceContractError::EquivalentWithoutPositiveEvidence)
        );

        let result = AssuranceReport::new(
            foundation.identity(),
            ObservationScope::default(),
            ValidationVerdict::Counterexample,
            ValidationEvidence {
                strength: EvidenceStrength::Universal,
                budget: ValidationBudget::default(),
                explored_paths: 0,
                executed_traces: 0,
                assumptions: Vec::new(),
                unresolved_effects: Vec::new(),
                counterexample: None,
                detail: String::new(),
            },
        );
        assert_eq!(
            result,
            Err(AssuranceContractError::PositiveEvidenceWithoutEquivalence)
        );
    }

    #[test]
    fn pipeline_validates_candidates_against_the_pcode_foundation() {
        struct Reconstructor;
        impl DirReconstructor for Reconstructor {
            type Error = ();

            fn reconstruct(
                &self,
                _foundation: &PcodeFoundation,
            ) -> Result<Vec<DirCandidate>, Self::Error> {
                Ok(vec![DirCandidate {
                    candidate_id: "native-0".to_string(),
                    source: CandidateSource::PcodeNative,
                    pseudocode: "return 1;".to_string(),
                    semantics: CandidateSemantics::Unmodeled,
                }])
            }
        }

        struct Verifier;
        impl DirVerifier for Verifier {
            fn verify(
                &self,
                foundation: &PcodeFoundation,
                _candidate: &DirCandidate,
                _scope: &ObservationScope,
                _budget: ValidationBudget,
            ) -> (ValidationVerdict, ValidationEvidence) {
                assert_eq!(foundation.identity().function_address, 0x1000);
                (
                    ValidationVerdict::Equivalent,
                    ValidationEvidence {
                        strength: EvidenceStrength::Traces,
                        budget: ValidationBudget::default(),
                        explored_paths: 0,
                        executed_traces: 4,
                        assumptions: Vec::new(),
                        unresolved_effects: Vec::new(),
                        counterexample: None,
                        detail: "four concrete traces agreed".to_string(),
                    },
                )
            }
        }

        let foundation = PcodeFoundation::new(0x1000, valid_function(1)).unwrap();
        let artifacts = DirPipeline::new(Reconstructor, Verifier)
            .run(
                &foundation,
                &ObservationScope::default(),
                ValidationBudget::default(),
            )
            .unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].assurance.foundation(), foundation.identity());
        assert_eq!(artifacts[0].assurance.band(), AssuranceBand::TraceValidated);
    }
}
