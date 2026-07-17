use fission_midend_core::ir::DispatcherProofUnit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Switch,
    GuardedTail,
    Conditional,
    Loop,
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockGraphRegionKind {
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
pub enum BlockGraphLegalityReason {
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
pub struct BlockGraphRegionProof {
    pub kind: BlockGraphRegionKind,
    pub entry: usize,
    pub members: Vec<usize>,
    pub exits: Vec<usize>,
    pub follow: Option<usize>,
    pub immediate_postdom: Option<usize>,
    pub scc_id: Option<usize>,
    pub legality_reason: BlockGraphLegalityReason,
    pub emit_ready: bool,
}

impl BlockGraphRegionProof {
    pub fn new(
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

    pub fn guarded_tail(
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

    pub fn reason_from_legality(legality: RegionLegality) -> BlockGraphLegalityReason {
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
pub enum RegionRejectionReason {
    MissingTerminalJoin,
    SideEntryConflict,
    AliasInterleaveConflict,
    AmbiguousFollow,
    EmitReadyFailed,
    IncompleteOrdinalDomain,
    NonCanonicalLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitReadyFailureFamily {
    ProofMissing,
    ProofIncomplete,
    InvalidLegality,
    IncompleteOrdinalDomain,
    SharedTailConflict,
    SelectorHasSideEffects,
    MissingRecoveredCases,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitReadyDecision {
    pub proof_present: bool,
    pub proof_complete: bool,
    pub emit_ready: bool,
    pub failure: Option<EmitReadyFailureFamily>,
}

impl EmitReadyDecision {
    pub fn from_dispatcher_proof(proof: Option<&DispatcherProofUnit>) -> Self {
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
pub struct RegionLegality {
    pub entry_unique: bool,
    pub terminal_join_present: bool,
    pub follow_witness: bool,
    pub postdom_witness: bool,
    pub side_entry_free: bool,
    pub side_exit_legal: bool,
    pub alias_interleave_legal: bool,
    pub selector_side_effect_free: bool,
    pub ordinal_domain_complete: bool,
    pub shared_tail_conflict_free: bool,
}

impl RegionLegality {
    pub fn for_structured_region(kind: RegionKind) -> Self {
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

    pub fn is_complete_for(self, kind: RegionKind) -> bool {
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
pub struct RegionProof {
    pub kind: RegionKind,
    pub entry: usize,
    pub members: Vec<usize>,
    pub exit: Option<usize>,
    pub follow: Option<usize>,
    pub postdom_anchor: Option<usize>,
    pub selector_or_condition: Option<String>,
    pub proof_complete: bool,
    pub emit_ready: bool,
    pub legality: RegionLegality,
    pub rejection_reason: Option<RegionRejectionReason>,
}

impl RegionProof {
    pub fn structured(
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
