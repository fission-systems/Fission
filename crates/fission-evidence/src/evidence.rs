//! One claim, and everything needed to weigh it.

use crate::digest::ContentDigest;
use crate::location::{CodeLocation, FunctionKey};
use crate::runtime::{RunId, RuntimeCodeLocation, RuntimeLocation, UnattributedReason};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a claim is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Entity {
    Code(CodeLocation),
    RuntimeCode(RuntimeCodeLocation),
    Function(FunctionKey),
}

/// The relation claimed between entity and value.
///
/// Both predicates are **existential**: "control went from here to there". No
/// predicate here says the targets listed are all there are, so no set of these
/// claims can close a target set or prune an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    IndirectCallTarget,
    IndirectJumpTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Value {
    Code(CodeLocation),
    RuntimeCode(RuntimeCodeLocation),
    /// Seen, but not attributable to any code.
    Address {
        at: RuntimeLocation,
        reason: UnattributedReason,
    },
}

/// What kind of process produced a claim. Kinds are never merged: a thousand
/// observations do not become declared metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    DeclaredMetadata,
    StaticInference,
    RuntimeObservation,
    UserAssertion,
    SymbolicInference,
    Hypothesis,
}

/// Where a claim holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Scope {
    /// Every execution of the image.
    Image,
    /// One execution context of one run.
    Run {
        run: RunId,
        process: u32,
        thread: u32,
        epoch: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Producer {
    pub name: String,
    pub version: String,
}

/// A stored artifact a claim was read from, and where in it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Artifact schema, for example `fission-runtime-trace-v1`.
    pub schema: String,
    pub digest: ContentDigest,
    /// Record number inside the artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Dependency {
    Evidence { id: EvidenceId },
    Artifact(ArtifactRef),
}

/// Content identity of a claim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EvidenceId(pub ContentDigest);

impl std::fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Whether a claim is still put forward. Withdrawal is recorded beside the
/// claim; the claim itself is never edited or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Lifecycle {
    Active,
    Withdrawn { reason: String },
}

/// What checking has established. Orthogonal to source, scope and lifecycle:
/// an observation can be unchecked, and a hypothesis can be refuted. No
/// numeric confidence stands in for any of these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    #[default]
    Unchecked,
    ObservedAgreement,
    ReplayValidated,
    ProvenUnderAssumptions,
    Refuted,
    Unknown,
    Unsupported,
}

/// The immutable part of an evidence record. Two claims that agree on every
/// field are the same claim and have the same [`EvidenceId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub entity: Entity,
    pub predicate: Predicate,
    pub value: Value,
    pub source: SourceKind,
    pub scope: Scope,
    pub producer: Producer,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dependencies: BTreeSet<Dependency>,
    /// Conditions the claim rests on, in plain words.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub assumptions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceError {
    #[error("a runtime observation holds in a run, not for every execution of an image")]
    UnscopedObservation,
    #[error("the claim's runtime code belongs to a run other than the claim's scope")]
    ScopeMismatch,
    #[error("a claim needs a producer name and version")]
    MissingProducer,
}

impl Evidence {
    pub fn id(&self) -> EvidenceId {
        EvidenceId(ContentDigest::of_canonical(
            "fission-evidence-claim-v1",
            self,
        ))
    }

    pub fn validate(&self) -> Result<(), EvidenceError> {
        if self.producer.name.is_empty() || self.producer.version.is_empty() {
            return Err(EvidenceError::MissingProducer);
        }
        if self.source == SourceKind::RuntimeObservation && self.scope == Scope::Image {
            return Err(EvidenceError::UnscopedObservation);
        }
        let entity_run = match &self.entity {
            Entity::RuntimeCode(code) => Some(&code.at.run),
            Entity::Code(_) | Entity::Function(_) => None,
        };
        let value_run = match &self.value {
            Value::RuntimeCode(code) => Some(&code.at.run),
            Value::Address { at, .. } => Some(&at.run),
            Value::Code(_) => None,
        };
        for named in [entity_run, value_run].into_iter().flatten() {
            if !matches!(&self.scope, Scope::Run { run, .. } if run == named) {
                return Err(EvidenceError::ScopeMismatch);
            }
        }
        Ok(())
    }
}
