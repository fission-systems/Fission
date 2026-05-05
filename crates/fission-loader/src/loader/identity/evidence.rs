//! Bounded evidence attachment for identity detections.

use super::model::{EvidenceLocation, IdentityEvidence, IdentitySource};

pub struct EvidenceBudget {
    remaining: usize,
}

impl EvidenceBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    pub fn push(&mut self, bucket: &mut Vec<IdentityEvidence>, ev: IdentityEvidence) {
        if self.remaining == 0 {
            return;
        }
        bucket.push(ev);
        self.remaining -= 1;
    }

    pub fn push_simple(
        &mut self,
        bucket: &mut Vec<IdentityEvidence>,
        source: IdentitySource,
        location: EvidenceLocation,
        description: impl Into<String>,
        matched: impl Into<String>,
    ) {
        self.push(
            bucket,
            IdentityEvidence {
                source,
                location,
                description: description.into(),
                matched: matched.into(),
            },
        );
    }
}
