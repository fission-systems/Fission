//! Typed, version-scoped declarations and checked effect contracts for ISA userops (`CALLOTHER`).
//!
//! This module gives user-defined SLEIGH operations explicit identities, operand/output width
//! constraints, conservative side-effect lattices, and separated provenance/availability levels so
//! optimization and verification consumers never treat unverified or ambiguous names as pure
//! no-ops.

use crate::pcode::{PcodeOp, PcodeOpcode, Varnode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

/// Immutable language + specification version identifying the table a `CallOther` index belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IntrinsicScope {
    pub language_id: String,
    pub spec_version: String,
}

impl IntrinsicScope {
    #[must_use]
    pub fn new(language_id: impl Into<String>, spec_version: impl Into<String>) -> Self {
        Self {
            language_id: language_id.into(),
            spec_version: spec_version.into(),
        }
    }
}

/// Canonical identity of a declared SLEIGH userop (`CALLOTHER` target).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntrinsicIdentity {
    pub scope: IntrinsicScope,
    pub userop_id: u32,
    pub name: String,
}

/// High-level scalar/value kind carried by a userop operand or return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntrinsicValueKind {
    BitVector,
    SignedInt,
    UnsignedInt,
    Float,
    Bool,
    Pointer,
    OpaqueBits,
}

/// Width constraint (in bytes, matching [`Varnode::size`]) for an intrinsic operand or output.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntrinsicWidthConstraint {
    ExactBytes(u32),
    OneOfBytes(Vec<u32>),
    /// Width must match the resolved byte width of input `index` (excluding the userop ID slot).
    MatchesOperand(usize),
    RangeBytes {
        min: u32,
        max: u32,
    },
}

impl IntrinsicWidthConstraint {
    /// Validate that this constraint is well formed for an operation taking `input_count` operands.
    pub fn validate(&self, input_count: usize, context: &str) -> Result<(), ContractViolation> {
        match self {
            Self::ExactBytes(0) => Err(ContractViolation::InvalidSignature {
                reason: format!("{context}: ExactBytes(0) is invalid"),
            }),
            Self::ExactBytes(_) => Ok(()),
            Self::OneOfBytes(choices) => {
                if choices.is_empty() || choices.contains(&0) {
                    return Err(ContractViolation::InvalidSignature {
                        reason: format!("{context}: OneOfBytes must be non-empty and non-zero"),
                    });
                }
                Ok(())
            }
            Self::MatchesOperand(idx) => {
                if *idx >= input_count {
                    return Err(ContractViolation::InvalidSignature {
                        reason: format!(
                            "{context}: MatchesOperand({idx}) out of bounds for {input_count} inputs"
                        ),
                    });
                }
                Ok(())
            }
            Self::RangeBytes { min, max } => {
                if *min == 0 || min > max {
                    return Err(ContractViolation::InvalidSignature {
                        reason: format!(
                            "{context}: invalid RangeBytes {{ min: {min}, max: {max} }}"
                        ),
                    });
                }
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn matches(&self, actual_bytes: u32, resolved_input_widths: &[u32]) -> bool {
        if actual_bytes == 0 {
            return false;
        }
        match self {
            Self::ExactBytes(expected) => actual_bytes == *expected,
            Self::OneOfBytes(choices) => choices.contains(&actual_bytes),
            Self::MatchesOperand(idx) => resolved_input_widths
                .get(*idx)
                .is_some_and(|expected| actual_bytes == *expected),
            Self::RangeBytes { min, max } => actual_bytes >= *min && actual_bytes <= *max,
        }
    }
}

/// Typed declaration for a single userop input or output slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperandDecl {
    pub name: String,
    pub kind: IntrinsicValueKind,
    pub width: IntrinsicWidthConstraint,
}

/// Signature of a userop (excluding slot 0, which carries the SLEIGH userop ID constant).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntrinsicSignature {
    pub inputs: Vec<OperandDecl>,
    pub output: Option<OperandDecl>,
}

impl IntrinsicSignature {
    pub fn validate(&self) -> Result<(), ContractViolation> {
        let input_count = self.inputs.len();
        for (idx, input) in self.inputs.iter().enumerate() {
            if let IntrinsicWidthConstraint::MatchesOperand(ref_idx) = input.width
                && ref_idx >= idx
            {
                return Err(ContractViolation::InvalidSignature {
                    reason: format!(
                        "input {idx} ({}) cannot reference later/self operand {ref_idx}",
                        input.name
                    ),
                });
            }
            input
                .width
                .validate(input_count, &format!("input '{}'", input.name))?;
        }
        if let Some(out) = &self.output {
            out.width
                .validate(input_count, &format!("output '{}'", out.name))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntrinsicMemoryEffect {
    None,
    ReadsGlobal,
    WritesGlobal,
    ReadsAndWritesGlobal,
    Unknown,
}

impl IntrinsicMemoryEffect {
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        use IntrinsicMemoryEffect::*;
        match (self, other) {
            (Unknown, _) | (_, Unknown) => Unknown,
            (ReadsAndWritesGlobal, _) | (_, ReadsAndWritesGlobal) => ReadsAndWritesGlobal,
            (ReadsGlobal, WritesGlobal) | (WritesGlobal, ReadsGlobal) => ReadsAndWritesGlobal,
            (None, x) | (x, None) => x,
            (ReadsGlobal, ReadsGlobal) => ReadsGlobal,
            (WritesGlobal, WritesGlobal) => WritesGlobal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntrinsicRegisterEffect {
    OutputOnly,
    ReadsFlags,
    WritesFlags,
    ReadsAndWritesFlags,
    ClobbersArchitecturalState,
    Unknown,
}

impl IntrinsicRegisterEffect {
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        use IntrinsicRegisterEffect::*;
        match (self, other) {
            (Unknown, _) | (_, Unknown) => Unknown,
            (ClobbersArchitecturalState, _) | (_, ClobbersArchitecturalState) => {
                ClobbersArchitecturalState
            }
            (ReadsAndWritesFlags, _) | (_, ReadsAndWritesFlags) => ReadsAndWritesFlags,
            (ReadsFlags, WritesFlags) | (WritesFlags, ReadsFlags) => ReadsAndWritesFlags,
            (OutputOnly, x) | (x, OutputOnly) => x,
            (ReadsFlags, ReadsFlags) => ReadsFlags,
            (WritesFlags, WritesFlags) => WritesFlags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntrinsicControlEffect {
    TotalNoTrap,
    MayFault,
    MayTrapOrExit,
    NonReturning,
    Unknown,
}

impl IntrinsicControlEffect {
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        use IntrinsicControlEffect::*;
        match (self, other) {
            (Unknown, _) | (_, Unknown) => Unknown,
            (NonReturning, _) | (_, NonReturning) => NonReturning,
            (MayTrapOrExit, _) | (_, MayTrapOrExit) => MayTrapOrExit,
            (MayFault, _) | (_, MayFault) => MayFault,
            (TotalNoTrap, TotalNoTrap) => TotalNoTrap,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntrinsicEnvironmentEffect {
    Deterministic,
    ReadsHardwareState,
    ExternalSideEffect,
    Nondeterministic,
    Unknown,
}

impl IntrinsicEnvironmentEffect {
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        use IntrinsicEnvironmentEffect::*;
        match (self, other) {
            (Unknown, _) | (_, Unknown) => Unknown,
            (Nondeterministic, _) | (_, Nondeterministic) => Nondeterministic,
            (ExternalSideEffect, _) | (_, ExternalSideEffect) => ExternalSideEffect,
            (ReadsHardwareState, _) | (_, ReadsHardwareState) => ReadsHardwareState,
            (Deterministic, Deterministic) => Deterministic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IntrinsicOrderingEffect {
    None,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
    DeviceBarrier,
    Unknown,
}

impl IntrinsicOrderingEffect {
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        use IntrinsicOrderingEffect::*;
        match (self, other) {
            (Unknown, _) | (_, Unknown) => Unknown,
            (DeviceBarrier, _) | (_, DeviceBarrier) => DeviceBarrier,
            (SeqCst, _) | (_, SeqCst) => SeqCst,
            (AcqRel, _) | (_, AcqRel) => AcqRel,
            (Acquire, Release) | (Release, Acquire) => AcqRel,
            (None, x) | (x, None) => x,
            (Acquire, Acquire) => Acquire,
            (Release, Release) => Release,
        }
    }
}

/// Conservative effect product describing how a userop interacts with architectural and
/// external state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntrinsicEffects {
    pub memory: IntrinsicMemoryEffect,
    pub registers: IntrinsicRegisterEffect,
    pub control: IntrinsicControlEffect,
    pub environment: IntrinsicEnvironmentEffect,
    pub ordering: IntrinsicOrderingEffect,
}

impl IntrinsicEffects {
    /// Worst-case conservative barrier for undeclared, mismatched, or unverified operations.
    #[must_use]
    pub const fn opaque_barrier() -> Self {
        Self {
            memory: IntrinsicMemoryEffect::Unknown,
            registers: IntrinsicRegisterEffect::Unknown,
            control: IntrinsicControlEffect::Unknown,
            environment: IntrinsicEnvironmentEffect::Unknown,
            ordering: IntrinsicOrderingEffect::Unknown,
        }
    }

    /// Total, deterministic, side-effect-free computation whose value depends solely on its inputs.
    #[must_use]
    pub const fn pure_total() -> Self {
        Self {
            memory: IntrinsicMemoryEffect::None,
            registers: IntrinsicRegisterEffect::OutputOnly,
            control: IntrinsicControlEffect::TotalNoTrap,
            environment: IntrinsicEnvironmentEffect::Deterministic,
            ordering: IntrinsicOrderingEffect::None,
        }
    }

    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        Self {
            memory: self.memory.join(other.memory),
            registers: self.registers.join(other.registers),
            control: self.control.join(other.control),
            environment: self.environment.join(other.environment),
            ordering: self.ordering.join(other.ordering),
        }
    }

    #[must_use]
    pub fn is_pure_value_effect(&self) -> bool {
        self.memory == IntrinsicMemoryEffect::None
            && self.registers == IntrinsicRegisterEffect::OutputOnly
            && self.environment == IntrinsicEnvironmentEffect::Deterministic
            && self.ordering == IntrinsicOrderingEffect::None
    }

    #[must_use]
    pub fn is_total_nontrapping(&self) -> bool {
        self.control == IntrinsicControlEffect::TotalNoTrap
    }

    #[must_use]
    pub fn reads_memory(&self) -> bool {
        !matches!(self.memory, IntrinsicMemoryEffect::None)
    }

    #[must_use]
    pub fn writes_memory(&self) -> bool {
        !matches!(
            self.memory,
            IntrinsicMemoryEffect::None | IntrinsicMemoryEffect::ReadsGlobal
        )
    }

    #[must_use]
    pub fn may_trap(&self) -> bool {
        self.control != IntrinsicControlEffect::TotalNoTrap
    }

    #[must_use]
    pub fn is_ordering_barrier(&self) -> bool {
        self.ordering != IntrinsicOrderingEffect::None
    }

    #[must_use]
    pub fn is_opaque_barrier(&self) -> bool {
        *self == Self::opaque_barrier()
    }
}

/// How an intrinsic contract's semantics were established.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticProvenance {
    Declared,
    ExternallyDocumented { reference: String },
    Inferred { source: String },
    VerifiedImplementation { backend: String, witness: String },
}

/// Whether a backend provides an executable/semantic implementation for the intrinsic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImplementationAvailability {
    Unimplemented,
    DeclaredOnly,
    AvailableUnverified { backend: String },
    Verified { backend: String },
}

/// Backend-reported implementation capabilities used to verify or downgrade declarations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendIntrinsicCapabilities {
    capabilities: BTreeMap<(String, String), ImplementationAvailability>,
}

impl BackendIntrinsicCapabilities {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        language_family_or_id: impl Into<String>,
        name: impl Into<String>,
        availability: ImplementationAvailability,
    ) {
        self.capabilities
            .insert((language_family_or_id.into(), name.into()), availability);
    }

    #[must_use]
    pub fn availability_for(&self, language_id: &str, name: &str) -> ImplementationAvailability {
        if let Some(exact) = self
            .capabilities
            .get(&(language_id.to_string(), name.to_string()))
        {
            return exact.clone();
        }
        let family = language_family(language_id);
        if let Some(by_family) = self
            .capabilities
            .get(&(family.to_string(), name.to_string()))
        {
            return by_family.clone();
        }
        ImplementationAvailability::DeclaredOnly
    }

    #[must_use]
    pub fn supports_verified(&self, language_id: &str, name: &str) -> Option<&str> {
        match self
            .capabilities
            .get(&(language_id.to_string(), name.to_string()))
            .or_else(|| {
                self.capabilities
                    .get(&(language_family(language_id).to_string(), name.to_string()))
            }) {
            Some(ImplementationAvailability::Verified { backend }) => Some(backend.as_str()),
            _ => None,
        }
    }
}

/// Complete typed contract for a single version-scoped userop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntrinsicDeclaration {
    pub identity: IntrinsicIdentity,
    pub signature: IntrinsicSignature,
    pub effects: IntrinsicEffects,
    pub provenance: SemanticProvenance,
    pub availability: ImplementationAvailability,
}

impl IntrinsicDeclaration {
    /// Check internal consistency of signature, effects, provenance, and backend availability.
    pub fn validate(&self) -> Result<(), ContractViolation> {
        if self.identity.scope.language_id.trim().is_empty()
            || self.identity.scope.spec_version.trim().is_empty()
            || self.identity.name.trim().is_empty()
        {
            return Err(ContractViolation::InvalidSignature {
                reason: "intrinsic identity scope and name must be non-empty".to_string(),
            });
        }
        self.signature.validate()?;

        // Purity requires justified semantics and cannot claim a total pure function with no result.
        if self.effects.is_pure_value_effect()
            && self.effects.is_total_nontrapping()
            && self.signature.output.is_none()
        {
            return Err(ContractViolation::InvalidEffectAnnotation {
                reason: format!(
                    "userop '{}' declares pure total semantics without an output value",
                    self.identity.name
                ),
            });
        }

        // Verified provenance and Verified availability must agree: missing backend support cannot
        // silently claim proof-grade verification.
        match (&self.provenance, &self.availability) {
            (
                SemanticProvenance::VerifiedImplementation { backend: pb, .. },
                ImplementationAvailability::Verified { backend: ab },
            ) => {
                if pb != ab {
                    return Err(ContractViolation::InvalidEffectAnnotation {
                        reason: format!(
                            "verified provenance backend '{pb}' disagrees with availability '{ab}'"
                        ),
                    });
                }
                if self.effects.is_opaque_barrier() {
                    return Err(ContractViolation::InvalidEffectAnnotation {
                        reason: "verified implementation cannot have completely Unknown effects"
                            .to_string(),
                    });
                }
            }
            (SemanticProvenance::VerifiedImplementation { .. }, _) => {
                return Err(ContractViolation::InvalidEffectAnnotation {
                    reason: format!(
                        "userop '{}' cannot claim VerifiedImplementation without Verified backend availability",
                        self.identity.name
                    ),
                });
            }
            (_, ImplementationAvailability::Verified { .. }) => {
                return Err(ContractViolation::InvalidEffectAnnotation {
                    reason: format!(
                        "userop '{}' cannot claim Verified backend availability without VerifiedImplementation provenance",
                        self.identity.name
                    ),
                });
            }
            _ => {}
        }

        Ok(())
    }

    #[must_use]
    pub fn is_proof_eligible(&self) -> bool {
        matches!(
            (&self.provenance, &self.availability),
            (
                SemanticProvenance::VerifiedImplementation { .. },
                ImplementationAvailability::Verified { .. }
            )
        ) && !self.effects.is_opaque_barrier()
    }
}

/// Why a userop declaration or `CallOther` check failed or was degraded to an opaque barrier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContractViolation {
    VersionSkew {
        expected: String,
        found: String,
    },
    LanguageMismatch {
        expected: String,
        found: String,
    },
    UnresolvedUserop {
        userop_id: u32,
        name: Option<String>,
    },
    AmbiguousDeclaration {
        name: String,
    },
    NameMismatch {
        userop_id: u32,
        expected: String,
        found: String,
    },
    NonConstantUseropIndex,
    ArityMismatch {
        name: String,
        expected: usize,
        found: usize,
    },
    MissingOutput {
        name: String,
    },
    UnexpectedOutput {
        name: String,
    },
    WidthMismatch {
        name: String,
        slot: String,
        expected: IntrinsicWidthConstraint,
        found_bytes: u32,
    },
    InvalidSignature {
        reason: String,
    },
    InvalidEffectAnnotation {
        reason: String,
    },
    MissingBackendImplementation {
        name: String,
    },
    UnsupportedForProof {
        name: String,
    },
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionSkew { expected, found } => {
                write!(
                    f,
                    "intrinsic spec version skew: expected '{expected}', found '{found}'"
                )
            }
            Self::LanguageMismatch { expected, found } => {
                write!(
                    f,
                    "intrinsic language mismatch: expected '{expected}', found '{found}'"
                )
            }
            Self::UnresolvedUserop { userop_id, name } => match name {
                Some(n) => write!(f, "unresolved/opaque userop #{userop_id} ('{n}')"),
                None => write!(f, "unresolved/opaque userop #{userop_id}"),
            },
            Self::AmbiguousDeclaration { name } => {
                write!(f, "ambiguous intrinsic declaration for '{name}'")
            }
            Self::NameMismatch {
                userop_id,
                expected,
                found,
            } => write!(
                f,
                "userop #{userop_id} name mismatch: expected '{expected}', found '{found}'"
            ),
            Self::NonConstantUseropIndex => {
                write!(
                    f,
                    "CallOther slot 0 is missing or not a constant userop index"
                )
            }
            Self::ArityMismatch {
                name,
                expected,
                found,
            } => write!(
                f,
                "userop '{name}' operand arity mismatch: expected {expected}, found {found}"
            ),
            Self::MissingOutput { name } => write!(f, "userop '{name}' requires an output varnode"),
            Self::UnexpectedOutput { name } => {
                write!(f, "userop '{name}' forbids an output varnode")
            }
            Self::WidthMismatch {
                name,
                slot,
                expected,
                found_bytes,
            } => write!(
                f,
                "userop '{name}' slot '{slot}' width mismatch: expected {expected:?}, got {found_bytes}B"
            ),
            Self::InvalidSignature { reason } => write!(f, "invalid intrinsic signature: {reason}"),
            Self::InvalidEffectAnnotation { reason } => {
                write!(f, "invalid intrinsic effect annotation: {reason}")
            }
            Self::MissingBackendImplementation { name } => {
                write!(f, "userop '{name}' has no verified backend implementation")
            }
            Self::UnsupportedForProof { name } => {
                write!(f, "userop '{name}' is unsupported for proof")
            }
        }
    }
}

impl std::error::Error for ContractViolation {}

/// Verified contract judgment produced when a `CallOther` matches a registered declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedIntrinsicCall {
    pub declaration: IntrinsicDeclaration,
    pub effective_effects: IntrinsicEffects,
    pub proof_eligible: bool,
    pub eliminable_when_unused: bool,
}

/// Outcome of querying an [`IntrinsicRegistry`] for a `PcodeOp::CallOther`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedIntrinsicOutcome {
    Resolved(CheckedIntrinsicCall),
    OpaqueBarrier {
        userop_id: Option<u32>,
        name: Option<String>,
        reason: ContractViolation,
        effects: IntrinsicEffects,
    },
}

/// Scoped, immutable-at-query-time table mapping userop IDs/names to checked contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntrinsicRegistry {
    scope: IntrinsicScope,
    userop_names: BTreeMap<u32, String>,
    by_id: BTreeMap<u32, IntrinsicDeclaration>,
    by_name: HashMap<String, u32>,
    ambiguous_names: HashMap<String, ContractViolation>,
}

impl IntrinsicRegistry {
    #[must_use]
    pub fn new(scope: IntrinsicScope, userop_names: BTreeMap<u32, String>) -> Self {
        Self {
            scope,
            userop_names,
            by_id: BTreeMap::new(),
            by_name: HashMap::new(),
            ambiguous_names: HashMap::new(),
        }
    }

    #[must_use]
    pub fn scope(&self) -> &IntrinsicScope {
        &self.scope
    }

    #[must_use]
    pub fn userop_name(&self, userop_id: u32) -> Option<&str> {
        self.userop_names.get(&userop_id).map(String::as_str)
    }

    #[must_use]
    pub fn declaration(&self, userop_id: u32) -> Option<&IntrinsicDeclaration> {
        self.by_id.get(&userop_id)
    }

    #[must_use]
    pub fn declarations(&self) -> &BTreeMap<u32, IntrinsicDeclaration> {
        &self.by_id
    }

    pub fn register(&mut self, decl: IntrinsicDeclaration) -> Result<(), ContractViolation> {
        decl.validate()?;
        if decl.identity.scope.language_id != self.scope.language_id {
            return Err(ContractViolation::LanguageMismatch {
                expected: self.scope.language_id.clone(),
                found: decl.identity.scope.language_id,
            });
        }
        if decl.identity.scope.spec_version != self.scope.spec_version {
            return Err(ContractViolation::VersionSkew {
                expected: self.scope.spec_version.clone(),
                found: decl.identity.scope.spec_version,
            });
        }
        let id = decl.identity.userop_id;
        if let Some(table_name) = self.userop_names.get(&id)
            && table_name != &decl.identity.name
        {
            return Err(ContractViolation::NameMismatch {
                userop_id: id,
                expected: table_name.clone(),
                found: decl.identity.name,
            });
        }
        if let Some(existing) = self.by_id.get(&id)
            && existing != &decl
        {
            return Err(ContractViolation::AmbiguousDeclaration {
                name: decl.identity.name,
            });
        }
        if let Some(&prev_id) = self.by_name.get(&decl.identity.name)
            && prev_id != id
        {
            self.ambiguous_names.insert(
                decl.identity.name.clone(),
                ContractViolation::AmbiguousDeclaration {
                    name: decl.identity.name.clone(),
                },
            );
            return Err(ContractViolation::AmbiguousDeclaration {
                name: decl.identity.name,
            });
        }
        self.by_name.insert(decl.identity.name.clone(), id);
        self.by_id.insert(id, decl);
        Ok(())
    }

    /// Re-check every declaration against actual backend capabilities.
    ///
    /// Absence of a verified backend implementation can never upgrade assurance: any declaration
    /// claiming `VerifiedImplementation` without matching `Verified` backend support is downgraded
    /// to `DeclaredOnly` + `Declared` provenance and recorded as unverified.
    pub fn validate_against_backend(
        &mut self,
        capabilities: &BackendIntrinsicCapabilities,
    ) -> Vec<ContractViolation> {
        let mut issues = Vec::new();
        for decl in self.by_id.values_mut() {
            let backend_avail =
                capabilities.availability_for(&self.scope.language_id, &decl.identity.name);
            let claims_verified = matches!(
                (&decl.provenance, &decl.availability),
                (
                    SemanticProvenance::VerifiedImplementation { .. },
                    ImplementationAvailability::Verified { .. }
                )
            );
            match (&decl.availability, &backend_avail) {
                (
                    ImplementationAvailability::Verified { backend: claimed },
                    ImplementationAvailability::Verified { backend: actual },
                ) if claimed == actual => {}
                _ if claims_verified
                    || matches!(backend_avail, ImplementationAvailability::Unimplemented) =>
                {
                    if matches!(backend_avail, ImplementationAvailability::Unimplemented) {
                        issues.push(ContractViolation::MissingBackendImplementation {
                            name: decl.identity.name.clone(),
                        });
                    } else {
                        issues.push(ContractViolation::UnsupportedForProof {
                            name: decl.identity.name.clone(),
                        });
                    }
                    decl.availability = backend_avail;
                    if !matches!(
                        decl.availability,
                        ImplementationAvailability::Verified { .. }
                    ) && matches!(
                        decl.provenance,
                        SemanticProvenance::VerifiedImplementation { .. }
                    ) {
                        decl.provenance = SemanticProvenance::Declared;
                    }
                }
                _ => {}
            }
        }
        issues
    }

    /// Check a lifted [`PcodeOp::CallOther`] against this scoped contract table.
    ///
    /// Returns [`CheckedIntrinsicOutcome::OpaqueBarrier`] on any scope mismatch, version skew,
    /// unresolved/ambiguous operation, or arity/width violation.
    #[must_use]
    pub fn check_pcode_op(
        &self,
        expected_scope: &IntrinsicScope,
        op: &PcodeOp,
    ) -> CheckedIntrinsicOutcome {
        if expected_scope.language_id != self.scope.language_id {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: extract_userop_id(op),
                name: None,
                reason: ContractViolation::LanguageMismatch {
                    expected: expected_scope.language_id.clone(),
                    found: self.scope.language_id.clone(),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }
        if expected_scope.spec_version != self.scope.spec_version {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: extract_userop_id(op),
                name: None,
                reason: ContractViolation::VersionSkew {
                    expected: expected_scope.spec_version.clone(),
                    found: self.scope.spec_version.clone(),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }

        if op.opcode != PcodeOpcode::CallOther {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: None,
                name: None,
                reason: ContractViolation::InvalidSignature {
                    reason: format!("expected CallOther opcode, got {:?}", op.opcode),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }

        let Some(userop_id) = extract_userop_id(op) else {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: None,
                name: None,
                reason: ContractViolation::NonConstantUseropIndex,
                effects: IntrinsicEffects::opaque_barrier(),
            };
        };

        let table_name = self.userop_names.get(&userop_id).cloned();
        if let Some(name) = &table_name
            && let Some(amb) = self.ambiguous_names.get(name)
        {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: Some(userop_id),
                name: Some(name.clone()),
                reason: amb.clone(),
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }

        let Some(decl) = self.by_id.get(&userop_id) else {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: Some(userop_id),
                name: table_name,
                reason: ContractViolation::UnresolvedUserop {
                    userop_id,
                    name: self.userop_names.get(&userop_id).cloned(),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        };

        if let Some(name) = &table_name
            && name != &decl.identity.name
        {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: Some(userop_id),
                name: Some(name.clone()),
                reason: ContractViolation::NameMismatch {
                    userop_id,
                    expected: decl.identity.name.clone(),
                    found: name.clone(),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }

        let actual_inputs = &op.inputs[1..];
        let expected_inputs = &decl.signature.inputs;
        if actual_inputs.len() != expected_inputs.len() {
            return CheckedIntrinsicOutcome::OpaqueBarrier {
                userop_id: Some(userop_id),
                name: Some(decl.identity.name.clone()),
                reason: ContractViolation::ArityMismatch {
                    name: decl.identity.name.clone(),
                    expected: expected_inputs.len(),
                    found: actual_inputs.len(),
                },
                effects: IntrinsicEffects::opaque_barrier(),
            };
        }

        let mut resolved_widths = Vec::with_capacity(actual_inputs.len());
        for (input_vn, input_decl) in actual_inputs.iter().zip(expected_inputs.iter()) {
            if !input_decl.width.matches(input_vn.size, &resolved_widths) {
                return CheckedIntrinsicOutcome::OpaqueBarrier {
                    userop_id: Some(userop_id),
                    name: Some(decl.identity.name.clone()),
                    reason: ContractViolation::WidthMismatch {
                        name: decl.identity.name.clone(),
                        slot: input_decl.name.clone(),
                        expected: input_decl.width.clone(),
                        found_bytes: input_vn.size,
                    },
                    effects: IntrinsicEffects::opaque_barrier(),
                };
            }
            resolved_widths.push(input_vn.size);
        }

        match (&decl.signature.output, &op.output) {
            (Some(out_decl), Some(out_vn)) => {
                if !out_decl.width.matches(out_vn.size, &resolved_widths) {
                    return CheckedIntrinsicOutcome::OpaqueBarrier {
                        userop_id: Some(userop_id),
                        name: Some(decl.identity.name.clone()),
                        reason: ContractViolation::WidthMismatch {
                            name: decl.identity.name.clone(),
                            slot: out_decl.name.clone(),
                            expected: out_decl.width.clone(),
                            found_bytes: out_vn.size,
                        },
                        effects: IntrinsicEffects::opaque_barrier(),
                    };
                }
            }
            (Some(_), None) => {
                return CheckedIntrinsicOutcome::OpaqueBarrier {
                    userop_id: Some(userop_id),
                    name: Some(decl.identity.name.clone()),
                    reason: ContractViolation::MissingOutput {
                        name: decl.identity.name.clone(),
                    },
                    effects: IntrinsicEffects::opaque_barrier(),
                };
            }
            (None, Some(_)) => {
                return CheckedIntrinsicOutcome::OpaqueBarrier {
                    userop_id: Some(userop_id),
                    name: Some(decl.identity.name.clone()),
                    reason: ContractViolation::UnexpectedOutput {
                        name: decl.identity.name.clone(),
                    },
                    effects: IntrinsicEffects::opaque_barrier(),
                };
            }
            (None, None) => {}
        }

        let proof_eligible = decl.is_proof_eligible();
        let eliminable_when_unused = proof_eligible
            && decl.effects.is_pure_value_effect()
            && decl.effects.is_total_nontrapping();
        let effective_effects =
            if matches!(decl.availability, ImplementationAvailability::Unimplemented) {
                decl.effects.join(IntrinsicEffects::opaque_barrier())
            } else {
                decl.effects
            };

        CheckedIntrinsicOutcome::Resolved(CheckedIntrinsicCall {
            declaration: decl.clone(),
            effective_effects,
            proof_eligible,
            eliminable_when_unused,
        })
    }
}

fn extract_userop_id(op: &PcodeOp) -> Option<u32> {
    let slot0: &Varnode = op.inputs.first()?;
    if !slot0.is_constant {
        return None;
    }
    u32::try_from(slot0.offset).ok()
}

fn language_family(language_id: &str) -> &str {
    language_id.split(':').next().unwrap_or(language_id)
}

fn promote_with_backend(
    language_id: &str,
    name: &str,
    backend: &BackendIntrinsicCapabilities,
    documented_ref: &str,
) -> (SemanticProvenance, ImplementationAvailability) {
    let avail = backend.availability_for(language_id, name);
    match &avail {
        ImplementationAvailability::Verified { backend: b } => (
            SemanticProvenance::VerifiedImplementation {
                backend: b.clone(),
                witness: documented_ref.to_string(),
            },
            avail,
        ),
        _ => (
            SemanticProvenance::ExternallyDocumented {
                reference: documented_ref.to_string(),
            },
            avail,
        ),
    }
}

/// Build the version-scoped built-in declaration registry for a compiled SLEIGH userop table.
///
/// Any userop in `userops` outside the typed prototype subset stays unmentioned and therefore
/// resolves cleanly to [`CheckedIntrinsicOutcome::OpaqueBarrier`].
#[must_use]
pub fn build_default_intrinsic_registry(
    scope: IntrinsicScope,
    userops: &BTreeMap<u32, String>,
    backend: &BackendIntrinsicCapabilities,
) -> IntrinsicRegistry {
    let mut registry = IntrinsicRegistry::new(scope.clone(), userops.clone());
    let family = language_family(&scope.language_id);

    for (&id, name) in userops {
        let maybe_decl = match (family, name.as_str()) {
            ("x86", "crc32") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "Intel SDM Vol. 2 CRC32 (Castagnoli 0x1EDC6F41)",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: vec![
                            OperandDecl {
                                name: "acc".to_string(),
                                kind: IntrinsicValueKind::BitVector,
                                width: IntrinsicWidthConstraint::OneOfBytes(vec![4, 8]),
                            },
                            OperandDecl {
                                name: "data".to_string(),
                                kind: IntrinsicValueKind::BitVector,
                                width: IntrinsicWidthConstraint::OneOfBytes(vec![1, 2, 4, 8]),
                            },
                        ],
                        output: Some(OperandDecl {
                            name: "crc".to_string(),
                            kind: IntrinsicValueKind::BitVector,
                            width: IntrinsicWidthConstraint::MatchesOperand(0),
                        }),
                    },
                    effects: IntrinsicEffects::pure_total(),
                    provenance,
                    availability,
                })
            }
            ("x86", "rdtsc") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "Intel SDM Vol. 2 RDTSC",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: Vec::new(),
                        output: Some(OperandDecl {
                            name: "tsc".to_string(),
                            kind: IntrinsicValueKind::UnsignedInt,
                            width: IntrinsicWidthConstraint::ExactBytes(8),
                        }),
                    },
                    effects: IntrinsicEffects {
                        memory: IntrinsicMemoryEffect::None,
                        registers: IntrinsicRegisterEffect::OutputOnly,
                        control: IntrinsicControlEffect::MayFault,
                        environment: IntrinsicEnvironmentEffect::ReadsHardwareState,
                        ordering: IntrinsicOrderingEffect::None,
                    },
                    provenance,
                    availability,
                })
            }
            ("x86", "LOCK") | ("x86", "UNLOCK") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "Intel SDM Vol. 2 LOCK/UNLOCK bus serialization",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: Vec::new(),
                        output: None,
                    },
                    effects: IntrinsicEffects {
                        memory: IntrinsicMemoryEffect::None,
                        registers: IntrinsicRegisterEffect::OutputOnly,
                        control: IntrinsicControlEffect::TotalNoTrap,
                        environment: IntrinsicEnvironmentEffect::Deterministic,
                        ordering: IntrinsicOrderingEffect::SeqCst,
                    },
                    provenance,
                    availability,
                })
            }
            ("x86", "syscall") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "x86 SYSCALL supervisor transition",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: Vec::new(),
                        output: None,
                    },
                    effects: IntrinsicEffects {
                        memory: IntrinsicMemoryEffect::ReadsAndWritesGlobal,
                        registers: IntrinsicRegisterEffect::ClobbersArchitecturalState,
                        control: IntrinsicControlEffect::MayTrapOrExit,
                        environment: IntrinsicEnvironmentEffect::ExternalSideEffect,
                        ordering: IntrinsicOrderingEffect::SeqCst,
                    },
                    provenance,
                    availability,
                })
            }
            ("AARCH64", "crc32b") | ("AARCH64", "crc32w") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "ARMv8-A CRC32B/CRC32W (IEEE 0xEDB88320)",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: vec![
                            OperandDecl {
                                name: "acc".to_string(),
                                kind: IntrinsicValueKind::BitVector,
                                width: IntrinsicWidthConstraint::ExactBytes(4),
                            },
                            OperandDecl {
                                name: "data".to_string(),
                                kind: IntrinsicValueKind::BitVector,
                                width: IntrinsicWidthConstraint::ExactBytes(4),
                            },
                            OperandDecl {
                                name: "poly".to_string(),
                                kind: IntrinsicValueKind::BitVector,
                                width: IntrinsicWidthConstraint::ExactBytes(4),
                            },
                        ],
                        output: Some(OperandDecl {
                            name: "crc".to_string(),
                            kind: IntrinsicValueKind::BitVector,
                            width: IntrinsicWidthConstraint::ExactBytes(4),
                        }),
                    },
                    effects: IntrinsicEffects::pure_total(),
                    provenance,
                    availability,
                })
            }
            ("AARCH64", "DataMemoryBarrier") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "ARMv8-A DMB DataMemoryBarrier",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: vec![
                            OperandDecl {
                                name: "domain".to_string(),
                                kind: IntrinsicValueKind::UnsignedInt,
                                width: IntrinsicWidthConstraint::RangeBytes { min: 1, max: 4 },
                            },
                            OperandDecl {
                                name: "types".to_string(),
                                kind: IntrinsicValueKind::UnsignedInt,
                                width: IntrinsicWidthConstraint::RangeBytes { min: 1, max: 4 },
                            },
                        ],
                        output: None,
                    },
                    effects: IntrinsicEffects {
                        memory: IntrinsicMemoryEffect::None,
                        registers: IntrinsicRegisterEffect::OutputOnly,
                        control: IntrinsicControlEffect::TotalNoTrap,
                        environment: IntrinsicEnvironmentEffect::Deterministic,
                        ordering: IntrinsicOrderingEffect::SeqCst,
                    },
                    provenance,
                    availability,
                })
            }
            ("AARCH64", "CallSupervisor") => {
                let (provenance, availability) = promote_with_backend(
                    &scope.language_id,
                    name,
                    backend,
                    "ARMv8-A SVC supervisor exception",
                );
                Some(IntrinsicDeclaration {
                    identity: IntrinsicIdentity {
                        scope: scope.clone(),
                        userop_id: id,
                        name: name.clone(),
                    },
                    signature: IntrinsicSignature {
                        inputs: vec![OperandDecl {
                            name: "imm16".to_string(),
                            kind: IntrinsicValueKind::UnsignedInt,
                            width: IntrinsicWidthConstraint::ExactBytes(2),
                        }],
                        output: None,
                    },
                    effects: IntrinsicEffects {
                        memory: IntrinsicMemoryEffect::ReadsAndWritesGlobal,
                        registers: IntrinsicRegisterEffect::ClobbersArchitecturalState,
                        control: IntrinsicControlEffect::MayTrapOrExit,
                        environment: IntrinsicEnvironmentEffect::ExternalSideEffect,
                        ordering: IntrinsicOrderingEffect::SeqCst,
                    },
                    provenance,
                    availability,
                })
            }
            _ => None,
        };

        if let Some(decl) = maybe_decl {
            let _ = registry.register(decl);
        }
    }

    registry
}
