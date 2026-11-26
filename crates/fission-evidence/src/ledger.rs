//! Append-only evidence.

use crate::evidence::{
    Evidence, EvidenceError, EvidenceId, Lifecycle, Producer, VerificationStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EVIDENCE_LEDGER_SCHEMA: &str = "fission-evidence-ledger-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "axis", content = "to")]
pub enum StatusChange {
    Lifecycle(Lifecycle),
    Verification(VerificationStatus),
}

/// Something later learned about a claim. Events only accumulate; the newest
/// one on an axis is that axis's current value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEvent {
    pub target: EvidenceId,
    pub change: StatusChange,
    pub by: Producer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Added,
    /// The same claim was already there. Nothing changed.
    Duplicate,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error(transparent)]
    Evidence(#[from] EvidenceError),
    #[error("no claim {0} in this ledger")]
    UnknownEvidence(EvidenceId),
    #[error("ledger schema is {found:?}; this build reads {EVIDENCE_LEDGER_SCHEMA:?}")]
    Schema { found: String },
    #[error("claim stored as {stored} has content id {actual}")]
    IdMismatch {
        stored: EvidenceId,
        actual: EvidenceId,
    },
    #[error("ledger is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Claims keyed by content, plus the history of what changed about them.
///
/// Importing the same observations twice leaves the ledger byte-identical, and
/// the serialized form depends only on its contents and event order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceLedger {
    claims: BTreeMap<EvidenceId, Evidence>,
    history: Vec<StatusEvent>,
}

#[derive(Serialize, Deserialize)]
struct StoredClaim {
    id: EvidenceId,
    #[serde(flatten)]
    claim: Evidence,
}

#[derive(Serialize, Deserialize)]
struct StoredLedger {
    schema: String,
    claims: Vec<StoredClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<StatusEvent>,
}

impl EvidenceLedger {
    pub fn insert(&mut self, claim: Evidence) -> Result<InsertOutcome, LedgerError> {
        claim.validate()?;
        match self.claims.entry(claim.id()) {
            std::collections::btree_map::Entry::Occupied(_) => Ok(InsertOutcome::Duplicate),
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(claim);
                Ok(InsertOutcome::Added)
            }
        }
    }

    /// Append a status change, unless it is already the current value.
    pub fn record(&mut self, event: StatusEvent) -> Result<InsertOutcome, LedgerError> {
        if !self.claims.contains_key(&event.target) {
            return Err(LedgerError::UnknownEvidence(event.target));
        }
        let unchanged = match &event.change {
            StatusChange::Lifecycle(to) => self.lifecycle(&event.target) == *to,
            StatusChange::Verification(to) => self.verification(&event.target) == *to,
        };
        if unchanged {
            return Ok(InsertOutcome::Duplicate);
        }
        self.history.push(event);
        Ok(InsertOutcome::Added)
    }

    pub fn lifecycle(&self, id: &EvidenceId) -> Lifecycle {
        self.history
            .iter()
            .rev()
            .filter(|event| &event.target == id)
            .find_map(|event| match &event.change {
                StatusChange::Lifecycle(to) => Some(to.clone()),
                StatusChange::Verification(_) => None,
            })
            .unwrap_or(Lifecycle::Active)
    }

    pub fn verification(&self, id: &EvidenceId) -> VerificationStatus {
        self.history
            .iter()
            .rev()
            .filter(|event| &event.target == id)
            .find_map(|event| match event.change {
                StatusChange::Verification(to) => Some(to),
                StatusChange::Lifecycle(_) => None,
            })
            .unwrap_or_default()
    }

    /// Every claim ever inserted, in id order.
    pub fn claims(&self) -> impl Iterator<Item = (&EvidenceId, &Evidence)> {
        self.claims.iter()
    }

    pub fn history(&self) -> &[StatusEvent] {
        &self.history
    }

    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    pub fn to_json(&self) -> Result<String, LedgerError> {
        let stored = StoredLedger {
            schema: EVIDENCE_LEDGER_SCHEMA.to_string(),
            claims: self
                .claims
                .iter()
                .map(|(id, claim)| StoredClaim {
                    id: id.clone(),
                    claim: claim.clone(),
                })
                .collect(),
            history: self.history.clone(),
        };
        let mut text = serde_json::to_string_pretty(&stored)?;
        text.push('\n');
        Ok(text)
    }

    /// Read a stored ledger, re-deriving every id and re-validating every claim:
    /// an edited or corrupted file is refused rather than partly believed.
    pub fn from_json(text: &str) -> Result<Self, LedgerError> {
        let stored: StoredLedger = serde_json::from_str(text)?;
        if stored.schema != EVIDENCE_LEDGER_SCHEMA {
            return Err(LedgerError::Schema {
                found: stored.schema,
            });
        }
        let mut ledger = Self::default();
        for StoredClaim { id, claim } in stored.claims {
            let actual = claim.id();
            if actual != id {
                return Err(LedgerError::IdMismatch { stored: id, actual });
            }
            ledger.insert(claim)?;
        }
        for event in stored.history {
            ledger.record(event)?;
        }
        Ok(ledger)
    }
}
