//! Score → [`Confidence`] mapping and category gates (FP reduction).

use crate::detector::Confidence;

use fission_core::evidence_policy::IdentityEvidenceThresholds;

use super::model::{IdentityEvidence, IdentityKind};

const THRESHOLDS: IdentityEvidenceThresholds = IdentityEvidenceThresholds::DEFAULT;

#[must_use]
pub(super) fn confidence_from_score(score: u32) -> Confidence {
    if score <= THRESHOLDS.score_low_max {
        Confidence::Low
    } else if score <= THRESHOLDS.score_medium_max {
        Confidence::Medium
    } else {
        Confidence::High
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
    let needs_sources = THRESHOLDS.high_min_distinct_sources;
    if distinct_sources < needs_sources {
        return Confidence::Medium;
    }
    if matches!(kind, IdentityKind::Packer | IdentityKind::Protector)
        && score < THRESHOLDS.packer_protector_high_min_score
    {
        return Confidence::Medium;
    }
    c
}
