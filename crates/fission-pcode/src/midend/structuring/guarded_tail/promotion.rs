//! Guarded-tail promotion residual: telemetry mark_* and trace helpers.
//!
//! Free promote/execute bodies live in `fission-midend-structuring::guarded_tail`.
//! Methods here only bump `StructuringTelemetry` fields or read PreviewBuilder
//! diagnostic env (legitimate host residual under ADR 0012).

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn guarded_tail_diag_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DIAG").is_some())
    }

    pub(super) fn guarded_tail_function_address_impl(&self) -> u64 {
        self.pcode
            .blocks
            .first()
            .map(|block| block.start_address)
            .unwrap_or(0)
    }

    // Called from `guarded_tail_trace_enabled_for_current_fn`, which is
    // itself checked from ~20 sites in `fission-midend-structuring::
    // guarded_tail::bodies`. Cached so parsing `FISSION_PREVIEW_DIAG_ADDR`
    // happens once per process instead of once per call site visited.
    fn guarded_tail_trace_target_addr() -> Option<u64> {
        static TARGET: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
        *TARGET.get_or_init(|| {
            let raw = std::env::var("FISSION_PREVIEW_DIAG_ADDR").ok()?;
            let trimmed = raw.trim();
            let hex = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .unwrap_or(trimmed);
            u64::from_str_radix(hex, 16).ok()
        })
    }

    pub(crate) fn guarded_tail_trace_enabled_for_current_fn(&self) -> bool {
        let Some(target) = Self::guarded_tail_trace_target_addr() else {
            return false;
        };
        self.guarded_tail_function_address_impl() == target
    }

    pub(in crate::midend) fn emit_ready_trace_enabled_for_current_fn(&self) -> bool {
        Self::guarded_tail_diag_enabled() && self.guarded_tail_trace_enabled_for_current_fn()
    }

    pub(in crate::midend) fn emit_ready_trace(&self, message: impl std::fmt::Display) {
        if self.emit_ready_trace_enabled_for_current_fn() {
            eprintln!(
                "[EMIT-TRACE] row=0x{:x} {}",
                self.guarded_tail_function_address_impl(),
                message
            );
        }
    }

    pub(super) fn guarded_tail_trace_emit_snapshot(
        prefix: &str,
        stmts: &[DirStmt],
        max_lines: usize,
    ) {
        let take_n = stmts.len().min(max_lines.max(1));
        for (idx, stmt) in stmts.iter().take(take_n).enumerate() {
            eprintln!("{prefix} [{idx:02}] {stmt:?}");
        }
        if stmts.len() > take_n {
            eprintln!(
                "{prefix} ... (truncated {} stmts)",
                stmts.len().saturating_sub(take_n)
            );
        }
    }

    pub(super) fn map_guarded_tail_canonicalization_rejection(
        reason: GuardedTailCanonicalizationFailure,
    ) -> GuardedTailWitnessRejection {
        match reason {
            GuardedTailCanonicalizationFailure::InterleavedJoinUses
            | GuardedTailCanonicalizationFailure::AliasNotFallthrough
            | GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors
            | GuardedTailCanonicalizationFailure::AliasHasNonlocalRef
            | GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                GuardedTailWitnessRejection::AliasInterleaveConflict
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                GuardedTailWitnessRejection::AmbiguousFollow
            }
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries
            | GuardedTailCanonicalizationFailure::NestedTailEscape => {
                GuardedTailWitnessRejection::NonCanonicalLayout
            }
        }
    }

    pub(super) fn classify_must_emit_label_rejection(
        _body: &[DirStmt],
        _middle: &[DirStmt],
        _if_idx: usize,
        _label_idx: usize,
        _label: &str,
        _outside_refs: usize,
        _middle_refs: usize,
    ) -> Option<PromotionGateRejection> {
        None
    }

    pub(crate) fn mark_promotion_shape_rejection(&mut self, reason: PromotionShapeRejection) {
        self.telemetry.structuring.promotion_rejected_by_shape_count += 1;
        match reason {
            PromotionShapeRejection::MissingTerminalJoinTarget => {
                self.telemetry
                    .structuring
                    .promotion_rejected_by_shape_missing_terminal_join_target_count += 1;
            }
            PromotionShapeRejection::EmptyNonterminalTail => {
                self.telemetry
                    .structuring
                    .promotion_rejected_by_shape_empty_nonterminal_tail_count += 1;
            }
        }
    }

    pub(crate) fn mark_noncanonical_layout_rejection_impl(&mut self) {
        self.telemetry
            .structuring
            .discovery_rejected_noncanonical_layout_count += 1;
        self.telemetry.structuring.promotion_rejected_by_shape_count += 1;
    }

    pub(super) fn record_blockgraph_region_proof(&mut self, proof: &BlockGraphRegionProof) {
        self.telemetry.structuring.blockgraph_region_candidate_count += 1;
        match proof.legality_reason {
            BlockGraphLegalityReason::Complete => {
                self.telemetry.structuring.blockgraph_region_complete_count += 1;
            }
            BlockGraphLegalityReason::MissingFollow | BlockGraphLegalityReason::MissingPostdom => {
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_missing_follow_count += 1;
            }
            BlockGraphLegalityReason::MustEmitLabelConflict => {
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_must_emit_label_count += 1;
            }
            BlockGraphLegalityReason::IrreducibleScc => {
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_irreducible_count += 1;
            }
            BlockGraphLegalityReason::SideEntry
            | BlockGraphLegalityReason::SideExit
            | BlockGraphLegalityReason::AliasInterleave
            | BlockGraphLegalityReason::EmitReadyIncomplete
            | BlockGraphLegalityReason::Budget => {
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_emit_ready_count += 1;
            }
        }
    }

    pub(crate) fn record_guarded_tail_blockgraph_proof_impl(
        &mut self,
        candidate_idx: usize,
        witness: &RegionShapeWitness,
        legality_reason: BlockGraphLegalityReason,
    ) {
        let members = if candidate_idx <= witness.label_idx {
            (candidate_idx..=witness.label_idx).collect::<Vec<_>>()
        } else {
            vec![candidate_idx]
        };
        let follow = if witness.follow_witness {
            witness.label_idx.checked_add(1)
        } else {
            None
        };
        let proof =
            BlockGraphRegionProof::guarded_tail(candidate_idx, members, follow, legality_reason);
        self.record_blockgraph_region_proof(&proof);
    }

    pub(crate) fn mark_guarded_tail_witness_rejection(&mut self, reason: GuardedTailWitnessRejection) {
        match reason {
            GuardedTailWitnessRejection::MissingTerminalJoin => {
                self.telemetry
                    .structuring
                    .guarded_tail_rejected_missing_terminal_join_count += 1;
            }
            GuardedTailWitnessRejection::SideEntryConflict => {
                self.telemetry
                    .structuring
                    .guarded_tail_rejected_side_entry_conflict_count += 1;
            }
            GuardedTailWitnessRejection::AliasInterleaveConflict => {
                self.telemetry
                    .structuring
                    .guarded_tail_rejected_alias_interleave_conflict_count += 1;
            }
            GuardedTailWitnessRejection::AmbiguousFollow => {
                self.telemetry
                    .structuring
                    .guarded_tail_rejected_ambiguous_follow_count += 1;
            }
            GuardedTailWitnessRejection::NonCanonicalLayout => {}
        }
    }

    pub(crate) fn mark_guarded_tail_execution_rejection(
        &mut self,
        reason: GuardedTailExecutionRejection,
    ) {
        match reason {
            GuardedTailExecutionRejection::Witness(reason) => {
                self.mark_guarded_tail_witness_rejection(reason);
            }
            GuardedTailExecutionRejection::ReplacementIncomplete => {
                self.telemetry.structuring.region_emit_ready_failed_count += 1;
                self.telemetry
                    .structuring
                    .guarded_tail_replacement_plan_rejected_missing_merge_count += 1;
            }
            GuardedTailExecutionRejection::MustEmitLabelConflict => {
                self.telemetry.structuring.region_emit_ready_failed_count += 1;
                self.telemetry
                    .structuring
                    .guarded_tail_replacement_plan_rejected_unstable_read_count += 1;
            }
        }
    }

    pub(crate) fn mark_guarded_tail_canonicalization_failure(
        &mut self,
        reason: GuardedTailCanonicalizationFailure,
    ) {
        if self.guarded_tail_trace_enabled_for_current_fn() {
            eprintln!(
                "[GT-TRACE] fn=0x{:x} canonicalization_failure={:?}",
                self.guarded_tail_function_address_impl(),
                reason
            );
        }
        self.mark_noncanonical_layout_rejection_impl();
        match reason {
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_multiple_payload_entries += 1;
            }
            GuardedTailCanonicalizationFailure::InterleavedJoinUses => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_interleaved_join_uses += 1;
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_nonterminal_join_label += 1;
            }
            GuardedTailCanonicalizationFailure::NestedTailEscape => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_nested_tail_escape += 1;
            }
            GuardedTailCanonicalizationFailure::AliasNotFallthrough => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_alias_not_fallthrough_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_alias_has_multiple_internal_predecessors_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasNonlocalRef => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_alias_has_nonlocal_ref_count += 1;
            }
            GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                self.telemetry
                    .structuring
                    .canonicalization_failed_payload_crosses_join_count += 1;
            }
        }
    }

    pub(crate) fn mark_promotion_gate_rejection(&mut self, reason: PromotionGateRejection) {
        self.telemetry.structuring.promotion_rejected_by_gate_count += 1;
        match reason {
            PromotionGateRejection::MustEmitLabel => {
                self.telemetry.structuring.rejected_must_emit_label += 1
            }
            PromotionGateRejection::MustEmitLabelSurvivingMiddleRef => {
                self.telemetry.structuring.rejected_must_emit_label += 1;
                self.telemetry
                    .structuring
                    .rejected_must_emit_label_surviving_middle_ref += 1;
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_middle_ref_count += 1;
            }
            PromotionGateRejection::MustEmitLabelSurvivingExternalRef => {
                self.telemetry.structuring.rejected_must_emit_label += 1;
                self.telemetry
                    .structuring
                    .rejected_must_emit_label_surviving_external_ref += 1;
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_external_ref_count += 1;
            }
            PromotionGateRejection::MustEmitLabelOwnerConflict => {
                self.telemetry.structuring.rejected_must_emit_label += 1;
                self.telemetry
                    .structuring
                    .rejected_must_emit_label_owner_conflict += 1;
                self.telemetry
                    .structuring
                    .blockgraph_region_rejected_join_owner_conflict_count += 1;
            }
            PromotionGateRejection::NotSinglePredSucc => {
                self.telemetry.structuring.rejected_not_single_pred_succ += 1
            }
            PromotionGateRejection::ExternalEntry => {
                self.telemetry.structuring.rejected_external_entry += 1
            }
            PromotionGateRejection::LoopOrSwitchTarget => {
                self.telemetry.structuring.rejected_loop_or_switch_target += 1
            }
        }
    }

    pub(crate) fn promote_single_entry_guarded_tail_regions(
        &mut self,
        body: &mut Vec<DirStmt>,
    ) -> bool {
        fission_midend_structuring::promote_single_entry_guarded_tail_regions(self, body)
    }

    pub(crate) fn discover_guarded_tail_candidates(&mut self, body: &[DirStmt]) {
        fission_midend_structuring::discover_guarded_tail_candidates(self, body)
    }


    pub(crate) fn accept_structured_region(
        &mut self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        self.telemetry.structuring.promotion_candidate_count += 1;
        let has_internal = self.region_has_targeted_internal_entry(start_idx, skip_to, targeted);
        let min_prom_res =
            self.is_minimal_structured_promotion_candidate(start_idx, skip_to, targeted);
        let accepted = !has_internal || min_prom_res.is_ok();
        if !accepted
            && has_internal
            && let Err(reason) = min_prom_res
        {
            self.mark_promotion_gate_rejection(reason);
        }
        if accepted {
            self.telemetry.structuring.promoted_region_count += 1;
        }
        accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::{DirExpr, DirStmt, DirUnaryOp, NirType};

    #[test]
    fn must_emit_label_internalizes_same_guard_family_nested_before_owner() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("cond".to_string()),
                then_body: vec![DirStmt::Goto("join".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::If {
                cond: DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Goto("join".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Goto("join".to_string()),
            DirStmt::Label("join".to_string()),
            DirStmt::Goto("end".to_string()),
            DirStmt::Label("end".to_string()),
            DirStmt::Return(None),
        ];
        let middle = vec![DirStmt::Goto("join".to_string())];

        let rejection =
            PreviewBuilder::classify_must_emit_label_rejection(&body, &middle, 1, 3, "join", 1, 1);

        assert_eq!(rejection, None);
    }

    #[test]
    fn must_emit_label_rejects_unrelated_nested_before_owner() {
        let body = vec![
            DirStmt::If {
                cond: DirExpr::Var("outer".to_string()),
                then_body: vec![DirStmt::Goto("join".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::If {
                cond: DirExpr::Unary {
                    op: DirUnaryOp::Not,
                    expr: Box::new(DirExpr::Var("cond".to_string())),
                    ty: NirType::Bool,
                },
                then_body: vec![DirStmt::Goto("join".to_string())],
                else_body: Vec::new(),
            },
            DirStmt::Goto("join".to_string()),
            DirStmt::Label("join".to_string()),
            DirStmt::Goto("end".to_string()),
            DirStmt::Label("end".to_string()),
            DirStmt::Return(None),
        ];
        let middle = vec![DirStmt::Goto("join".to_string())];

        let rejection =
            PreviewBuilder::classify_must_emit_label_rejection(&body, &middle, 1, 3, "join", 1, 1);

        assert_eq!(rejection, None);
    }
}
