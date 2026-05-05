//! Score → [`Confidence`] mapping and category gates (FP reduction).

use crate::detector::Confidence;

use super::model::{IdentityEvidence, IdentityKind};

#[must_use]
pub(super) fn confidence_from_score(score: u32) -> Confidence {
    match score {
        0 | 1 | 2 | 3 => Confidence::Low,
        4 | 5 => Confidence::Medium,
        _ => Confidence::High,
    }
}

/// Distinct [`IdentitySource`] values among evidence rows (same source repeated does not lift High alone).
#[must_use]
pub(super) fn distinct_evidence_sources(evidence: &[IdentityEvidence]) -> usize {
    let mut keys = std::collections::BTreeSet::new();
    for e in evidence {
        keys.insert(e.source);
    }
    keys.len()
}

#[must_use]
pub(super) fn gate_high_for_kind(kind: IdentityKind, score: u32, distinct_sources: usize) -> Confidence {
    let c = confidence_from_score(score);
    if c != Confidence::High {
        return c;
    }
    let needs_sources = match kind {
        IdentityKind::Packer | IdentityKind::Protector => 2,
        IdentityKind::Compiler | IdentityKind::Language | IdentityKind::Runtime | IdentityKind::Linker => {
            2
        }
        _ => 2,
    };
    if distinct_sources < needs_sources {
        return Confidence::Medium;
    }
    if matches!(kind, IdentityKind::Packer | IdentityKind::Protector) && score < 7 {
        return Confidence::Medium;
    }
    c
}
