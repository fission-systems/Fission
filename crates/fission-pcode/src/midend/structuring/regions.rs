use crate::midend::DispatcherProofUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Switch,
    GuardedTail,
    Conditional,
    Loop,
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockGraphRegionKind {
    Sequence,
    If,
    IfElse,
    Loop,
    Switch,
    GuardedTail,
    Irreducible,
}

impl From<RegionKind> for BlockGraphRegionKind {
    fn from(value: RegionKind) -> Self {
        match value {
            RegionKind::Switch => Self::Switch,
            RegionKind::GuardedTail => Self::GuardedTail,
            RegionKind::Conditional => Self::If,
            RegionKind::Loop => Self::Loop,
            RegionKind::Sequence => Self::Sequence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockGraphLegalityReason {
    Complete,
    MissingFollow,
    MissingPostdom,
    SideEntry,
    SideExit,
    MustEmitLabelConflict,
    AliasInterleave,
    EmitReadyIncomplete,
    IrreducibleScc,
    Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockGraphRegionProof {
    pub(crate) kind: BlockGraphRegionKind,
    pub(crate) entry: usize,
    pub(crate) members: Vec<usize>,
    pub(crate) exits: Vec<usize>,
    pub(crate) follow: Option<usize>,
    pub(crate) immediate_postdom: Option<usize>,
    pub(crate) scc_id: Option<usize>,
    pub(crate) legality_reason: BlockGraphLegalityReason,
    pub(crate) emit_ready: bool,
}

impl BlockGraphRegionProof {
    pub(crate) fn new(
        kind: BlockGraphRegionKind,
        entry: usize,
        members: Vec<usize>,
        exits: Vec<usize>,
        follow: Option<usize>,
        immediate_postdom: Option<usize>,
        scc_id: Option<usize>,
        legality_reason: BlockGraphLegalityReason,
    ) -> Self {
        Self {
            kind,
            entry,
            members,
            exits,
            follow,
            immediate_postdom,
            scc_id,
            legality_reason,
            emit_ready: matches!(legality_reason, BlockGraphLegalityReason::Complete),
        }
    }

    pub(crate) fn guarded_tail(
        entry: usize,
        members: Vec<usize>,
        follow: Option<usize>,
        legality_reason: BlockGraphLegalityReason,
    ) -> Self {
        Self::new(
            BlockGraphRegionKind::GuardedTail,
            entry,
            members,
            follow.into_iter().collect(),
            follow,
            follow,
            None,
            legality_reason,
        )
    }

    pub(crate) fn reason_from_legality(legality: RegionLegality) -> BlockGraphLegalityReason {
        if !legality.terminal_join_present {
            return BlockGraphLegalityReason::MissingFollow;
        }
        if !legality.follow_witness {
            return BlockGraphLegalityReason::MissingFollow;
        }
        if !legality.postdom_witness {
            return BlockGraphLegalityReason::MissingPostdom;
        }
        if !legality.entry_unique {
            return BlockGraphLegalityReason::SideEntry;
        }
        if !legality.side_entry_free {
            return BlockGraphLegalityReason::SideEntry;
        }
        if !legality.side_exit_legal {
            return BlockGraphLegalityReason::SideExit;
        }
        if !legality.alias_interleave_legal {
            return BlockGraphLegalityReason::AliasInterleave;
        }
        if legality.is_complete_for(RegionKind::GuardedTail) {
            BlockGraphLegalityReason::Complete
        } else {
            BlockGraphLegalityReason::EmitReadyIncomplete
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockgraph_region_proof_records_complete_guarded_tail() {
        let proof = BlockGraphRegionProof::guarded_tail(
            2,
            vec![2, 3],
            Some(4),
            BlockGraphLegalityReason::Complete,
        );
        assert_eq!(proof.kind, BlockGraphRegionKind::GuardedTail);
        assert_eq!(proof.entry, 2);
        assert_eq!(proof.exits, vec![4]);
        assert_eq!(proof.follow, Some(4));
        assert_eq!(proof.immediate_postdom, Some(4));
        assert!(proof.emit_ready);
    }

    #[test]
    fn blockgraph_region_reason_prefers_missing_follow() {
        let legality = RegionLegality {
            entry_unique: true,
            terminal_join_present: true,
            follow_witness: false,
            postdom_witness: true,
            side_entry_free: true,
            side_exit_legal: true,
            alias_interleave_legal: true,
            selector_side_effect_free: false,
            ordinal_domain_complete: false,
            shared_tail_conflict_free: false,
        };
        assert_eq!(
            BlockGraphRegionProof::reason_from_legality(legality),
            BlockGraphLegalityReason::MissingFollow
        );
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
