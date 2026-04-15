use crate::nir::DispatcherProofUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Switch,
    GuardedTail,
    Conditional,
    Loop,
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionRejectionReason {
    MissingTerminalJoin,
    SideEntryConflict,
    AliasInterleaveConflict,
    AmbiguousFollow,
    EmitReadyFailed,
    IncompleteOrdinalDomain,
    NonCanonicalLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmitReadyFailureFamily {
    ProofMissing,
    ProofIncomplete,
    InvalidLegality,
    IncompleteOrdinalDomain,
    SharedTailConflict,
    SelectorHasSideEffects,
    MissingRecoveredCases,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmitReadyDecision {
    pub(crate) proof_present: bool,
    pub(crate) proof_complete: bool,
    pub(crate) emit_ready: bool,
    pub(crate) failure: Option<EmitReadyFailureFamily>,
}

impl EmitReadyDecision {
    pub(crate) fn from_dispatcher_proof(proof: Option<&DispatcherProofUnit>) -> Self {
        let Some(proof) = proof else {
            return Self {
                proof_present: false,
                proof_complete: false,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::ProofMissing),
            };
        };
        let Some(legality) = proof.legality_witness.as_ref() else {
            return Self {
                proof_present: true,
                proof_complete: proof.proof_complete,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::InvalidLegality),
            };
        };
        if !proof.proof_complete || proof.failure_family.is_some() {
            return Self {
                proof_present: true,
                proof_complete: proof.proof_complete,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::ProofIncomplete),
            };
        }
        if proof.recovered_cases.is_empty()
            || proof.selector_cardinality < 2
            || proof.target_cardinality < 2
        {
            return Self {
                proof_present: true,
                proof_complete: true,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::MissingRecoveredCases),
            };
        }
        if !legality.valid {
            return Self {
                proof_present: true,
                proof_complete: true,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::InvalidLegality),
            };
        }
        if !legality.side_effect_free_selector {
            return Self {
                proof_present: true,
                proof_complete: true,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::SelectorHasSideEffects),
            };
        }
        if !legality.ordinal_domain_complete {
            return Self {
                proof_present: true,
                proof_complete: true,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::IncompleteOrdinalDomain),
            };
        }
        if legality.shared_tail_conflict {
            return Self {
                proof_present: true,
                proof_complete: true,
                emit_ready: false,
                failure: Some(EmitReadyFailureFamily::SharedTailConflict),
            };
        }
        Self {
            proof_present: true,
            proof_complete: true,
            emit_ready: true,
            failure: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RegionLegality {
    pub(crate) entry_unique: bool,
    pub(crate) terminal_join_present: bool,
    pub(crate) follow_witness: bool,
    pub(crate) postdom_witness: bool,
    pub(crate) side_entry_free: bool,
    pub(crate) side_exit_legal: bool,
    pub(crate) alias_interleave_legal: bool,
    pub(crate) selector_side_effect_free: bool,
    pub(crate) ordinal_domain_complete: bool,
    pub(crate) shared_tail_conflict_free: bool,
}

impl RegionLegality {
    pub(crate) fn for_structured_region(kind: RegionKind) -> Self {
        let mut legality = Self {
            entry_unique: true,
            terminal_join_present: true,
            follow_witness: true,
            postdom_witness: true,
            side_entry_free: true,
            side_exit_legal: true,
            alias_interleave_legal: true,
            selector_side_effect_free: true,
            ordinal_domain_complete: true,
            shared_tail_conflict_free: true,
        };
        if !matches!(kind, RegionKind::Switch) {
            legality.selector_side_effect_free = false;
            legality.ordinal_domain_complete = false;
            legality.shared_tail_conflict_free = false;
        }
        legality
    }

    pub(crate) fn is_complete_for(self, kind: RegionKind) -> bool {
        let base = self.entry_unique
            && self.terminal_join_present
            && self.follow_witness
            && self.postdom_witness
            && self.side_entry_free
            && self.side_exit_legal
            && self.alias_interleave_legal;
        if !base {
            return false;
        }
        match kind {
            RegionKind::Switch => {
                self.selector_side_effect_free
                    && self.ordinal_domain_complete
                    && self.shared_tail_conflict_free
            }
            RegionKind::GuardedTail
            | RegionKind::Conditional
            | RegionKind::Loop
            | RegionKind::Sequence => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionProof {
    pub(crate) kind: RegionKind,
    pub(crate) entry: usize,
    pub(crate) members: Vec<usize>,
    pub(crate) exit: Option<usize>,
    pub(crate) follow: Option<usize>,
    pub(crate) postdom_anchor: Option<usize>,
    pub(crate) selector_or_condition: Option<String>,
    pub(crate) proof_complete: bool,
    pub(crate) emit_ready: bool,
    pub(crate) legality: RegionLegality,
    pub(crate) rejection_reason: Option<RegionRejectionReason>,
}

impl RegionProof {
    pub(crate) fn structured(
        kind: RegionKind,
        entry: usize,
        skip_to: usize,
        selector_or_condition: Option<String>,
    ) -> Self {
        let legality = RegionLegality::for_structured_region(kind);
        let members = (entry..skip_to).collect::<Vec<_>>();
        Self {
            kind,
            entry,
            members,
            exit: skip_to.checked_sub(1),
            follow: Some(skip_to),
            postdom_anchor: Some(skip_to),
            selector_or_condition,
            proof_complete: true,
            emit_ready: legality.is_complete_for(kind),
            legality,
            rejection_reason: None,
        }
    }
}
