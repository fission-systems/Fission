use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn classify_must_emit_label_rejection(
        body: &[HirStmt],
        middle: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
        outside_refs: usize,
        middle_refs: usize,
    ) -> Option<PromotionGateRejection> {
        let effective_middle_refs = middle_refs
            .saturating_sub(Self::trailing_middle_fallthrough_equivalent_refs(middle, label));
        if effective_middle_refs > 0 {
            return Some(PromotionGateRejection::MustEmitLabelSurvivingMiddleRef);
        }
        if outside_refs > 1 {
            if Self::outside_refs_preserve_forward_owner(body, if_idx, label_idx, label) {
                return Some(PromotionGateRejection::MustEmitLabelSurvivingExternalRef);
            }
            return Some(PromotionGateRejection::MustEmitLabelOwnerConflict);
        }
        if outside_refs == 1 {
            if Self::outside_refs_are_elidable_next_flow(body, if_idx, label_idx, label) {
                return None;
            }
            return Some(PromotionGateRejection::MustEmitLabelSurvivingExternalRef);
        }
        None
    }

    pub(super) fn mark_promotion_shape_rejection(&mut self, reason: PromotionShapeRejection) {
        self.promotion_rejected_by_shape_count += 1;
        match reason {
            PromotionShapeRejection::MissingTerminalJoinTarget => {
                self.promotion_rejected_by_shape_missing_terminal_join_target_count += 1;
            }
            PromotionShapeRejection::EmptyNonterminalTail => {
                self.promotion_rejected_by_shape_empty_nonterminal_tail_count += 1;
            }
        }
    }

    pub(super) fn mark_noncanonical_layout_rejection(&mut self) {
        self.discovery_rejected_noncanonical_layout_count += 1;
        self.promotion_rejected_by_shape_count += 1;
    }

    pub(super) fn mark_guarded_tail_canonicalization_failure(
        &mut self,
        reason: GuardedTailCanonicalizationFailure,
    ) {
        self.mark_noncanonical_layout_rejection();
        match reason {
            GuardedTailCanonicalizationFailure::MultiplePayloadEntries => {
                self.canonicalization_failed_multiple_payload_entries += 1;
            }
            GuardedTailCanonicalizationFailure::InterleavedJoinUses => {
                self.canonicalization_failed_interleaved_join_uses += 1;
            }
            GuardedTailCanonicalizationFailure::NonterminalJoinLabel => {
                self.canonicalization_failed_nonterminal_join_label += 1;
            }
            GuardedTailCanonicalizationFailure::NestedTailEscape => {
                self.canonicalization_failed_nested_tail_escape += 1;
            }
            GuardedTailCanonicalizationFailure::AliasNotFallthrough => {
                self.canonicalization_failed_alias_not_fallthrough_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasMultipleInternalPredecessors => {
                self.canonicalization_failed_alias_has_multiple_internal_predecessors_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasHasNonlocalRef => {
                self.canonicalization_failed_alias_has_nonlocal_ref_count += 1;
            }
            GuardedTailCanonicalizationFailure::AliasBodyNotTrivial => {
                self.canonicalization_failed_alias_body_not_trivial_count += 1;
            }
            GuardedTailCanonicalizationFailure::PayloadCrossesJoin => {
                self.canonicalization_failed_payload_crosses_join_count += 1;
            }
        }
    }

    pub(super) fn mark_promotion_gate_rejection(&mut self, reason: PromotionGateRejection) {
        self.promotion_rejected_by_gate_count += 1;
        match reason {
            PromotionGateRejection::MustEmitLabel => self.rejected_must_emit_label += 1,
            PromotionGateRejection::MustEmitLabelSurvivingMiddleRef => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_surviving_middle_ref += 1;
            }
            PromotionGateRejection::MustEmitLabelSurvivingExternalRef => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_surviving_external_ref += 1;
            }
            PromotionGateRejection::MustEmitLabelOwnerConflict => {
                self.rejected_must_emit_label += 1;
                self.rejected_must_emit_label_owner_conflict += 1;
            }
            PromotionGateRejection::NotSinglePredSucc => self.rejected_not_single_pred_succ += 1,
            PromotionGateRejection::ExternalEntry => self.rejected_external_entry += 1,
            PromotionGateRejection::LoopOrSwitchTarget => self.rejected_loop_or_switch_target += 1,
        }
    }

    pub(crate) fn promote_single_entry_guarded_tail_regions(
        &mut self,
        body: &mut Vec<HirStmt>,
    ) -> bool {
        let (normalized, alias_rewrites) = normalize_guarded_tail_layout(std::mem::take(body));
        *body = normalized;
        let referenced = collect_referenced_label_counts(body);
        let mut changed = alias_rewrites > 0;
        let mut idx = 0usize;
        while idx < body.len() {
            let HirStmt::If {
                cond,
                then_body,
                else_body,
            } = &body[idx]
            else {
                idx += 1;
                continue;
            };

            let (target_label, keep_middle_when_cond_true) = if else_body.is_empty() {
                let Some(label) = single_goto_target(then_body) else {
                    idx += 1;
                    continue;
                };
                (label.to_string(), false)
            } else if then_body.is_empty() {
                let Some(label) = single_goto_target(else_body) else {
                    idx += 1;
                    continue;
                };
                (label.to_string(), true)
            } else {
                idx += 1;
                continue;
            };

            let Some((target_label, label_idx)) =
                self.resolve_terminal_join_target(body, idx, &target_label, &referenced)
            else {
                if Self::find_top_level_label_after(body, idx, &target_label).is_some() {
                    self.mark_promotion_shape_rejection(
                        PromotionShapeRejection::MissingTerminalJoinTarget,
                    );
                }
                idx += 1;
                continue;
            };
            if !has_non_ignorable_payload(&body[idx + 1..label_idx]) {
                idx += 1;
                continue;
            }

            let (middle, external_redirects) = match self.canonicalize_guarded_tail_segment(
                &body[idx + 1..label_idx],
                body,
                idx + 1,
                &referenced,
            ) {
                Ok(middle) => middle,
                Err(reason) => {
                    self.mark_guarded_tail_canonicalization_failure(reason);
                    idx += 1;
                    continue;
                }
            };
            if middle.is_empty() || has_top_level_label(&middle) {
                self.mark_guarded_tail_canonicalization_failure(
                    GuardedTailCanonicalizationFailure::InterleavedJoinUses,
                );
                idx += 1;
                continue;
            }

            let tail_end = (label_idx + 1..body.len())
                .find(|pos| matches!(body.get(*pos), Some(HirStmt::Label(_))))
                .unwrap_or(body.len());
            let tail = body[label_idx + 1..tail_end].to_vec();
            let terminal_guarded_tail = label_idx + 1 == body.len();
            if tail.is_empty() && !terminal_guarded_tail {
                self.mark_promotion_shape_rejection(PromotionShapeRejection::EmptyNonterminalTail);
                idx += 1;
                continue;
            }

            self.promotion_candidate_count += 1;
            let (outside_refs, middle_refs) = Self::surviving_label_refs_after_guarded_tail_promotion(
                body,
                &middle,
                idx,
                label_idx,
                &target_label,
            );
            if let Some(reason) = Self::classify_must_emit_label_rejection(
                body,
                &middle,
                idx,
                label_idx,
                &target_label,
                outside_refs,
                middle_refs,
            ) {
                self.mark_promotion_gate_rejection(reason);
                idx += 1;
                continue;
            }

            let replacement = HirStmt::If {
                cond: if keep_middle_when_cond_true {
                    cond.clone()
                } else {
                    negate_expr(cond.clone())
                },
                then_body: middle,
                else_body: Vec::new(),
            };

            for (from, to) in &external_redirects {
                Self::rewrite_goto_label_in_stmts(body, from, to);
            }

            body[idx] = replacement;
            body.drain(idx + 1..=label_idx);
            self.promoted_region_count += 1;
            changed = true;
            idx += 1;
        }
        changed
    }

    pub(crate) fn discover_guarded_tail_candidates(&mut self, body: &[HirStmt]) {
        let (normalized, _) = normalize_guarded_tail_layout(body.to_vec());
        self.discover_guarded_tail_candidates_in_body(&normalized);
    }

    fn discover_guarded_tail_candidates_in_body(&mut self, body: &[HirStmt]) {
        for stmt in body {
            match stmt {
                HirStmt::Block(inner)
                | HirStmt::While { body: inner, .. }
                | HirStmt::DoWhile { body: inner, .. } => {
                    self.discover_guarded_tail_candidates_in_body(inner);
                }
                HirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.discover_guarded_tail_candidates_in_body(then_body);
                    self.discover_guarded_tail_candidates_in_body(else_body);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        self.discover_guarded_tail_candidates_in_body(&case.body);
                    }
                    self.discover_guarded_tail_candidates_in_body(default);
                }
                HirStmt::Assign { .. }
                | HirStmt::Expr(_)
                | HirStmt::Label(_)
                | HirStmt::Goto(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue => {}
            }
        }

        let referenced = collect_referenced_label_counts(body);
        for idx in 0..body.len() {
            let HirStmt::If {
                then_body,
                else_body,
                ..
            } = &body[idx]
            else {
                continue;
            };

            let target_label = if else_body.is_empty() {
                single_goto_target(then_body)
            } else if then_body.is_empty() {
                single_goto_target(else_body)
            } else {
                None
            };
            let Some(target_label) = target_label else {
                continue;
            };
            if Self::find_top_level_label_after(body, idx, target_label).is_none() {
                continue;
            }
            self.discovery_seen_guarded_tail_like_shape_count += 1;

            let Some((target_label, label_idx)) =
                self.resolve_terminal_join_target(body, idx, target_label, &referenced)
            else {
                self.mark_guarded_tail_canonicalization_failure(
                    GuardedTailCanonicalizationFailure::NonterminalJoinLabel,
                );
                continue;
            };
            if !has_non_ignorable_payload(&body[idx + 1..label_idx]) {
                continue;
            }

            let (middle, _) = match self.canonicalize_guarded_tail_segment(
                &body[idx + 1..label_idx],
                body,
                idx + 1,
                &referenced,
            ) {
                Ok(middle) => middle,
                Err(reason) => {
                    self.mark_guarded_tail_canonicalization_failure(reason);
                    continue;
                }
            };

            self.promotion_candidate_count += 1;

            let (outside_refs, middle_refs) = Self::surviving_label_refs_after_guarded_tail_promotion(
                body,
                &middle,
                idx,
                label_idx,
                &target_label,
            );
            if let Some(reason) = Self::classify_must_emit_label_rejection(
                body,
                &middle,
                idx,
                label_idx,
                &target_label,
                outside_refs,
                middle_refs,
            ) {
                self.mark_promotion_gate_rejection(reason);
                continue;
            }
        }
    }

    pub(crate) fn accept_structured_region(
        &mut self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        self.promotion_candidate_count += 1;
        let accepted = !self.region_has_targeted_internal_entry(start_idx, skip_to, targeted)
            || self
                .is_minimal_structured_promotion_candidate(start_idx, skip_to, targeted)
                .is_ok();
        if !accepted
            && self.region_has_targeted_internal_entry(start_idx, skip_to, targeted)
            && let Err(reason) =
                self.is_minimal_structured_promotion_candidate(start_idx, skip_to, targeted)
        {
            self.mark_promotion_gate_rejection(reason);
        }
        if accepted {
            self.promoted_region_count += 1;
        }
        accepted
    }
}
