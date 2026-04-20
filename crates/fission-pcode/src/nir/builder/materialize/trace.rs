use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    fn bump_materialize_owner_histogram(
        map: &mut std::collections::BTreeMap<String, usize>,
        key: impl Into<String>,
    ) {
        *map.entry(key.into()).or_insert(0) += 1;
    }

    fn format_materialize_owner_histogram(
        map: &std::collections::BTreeMap<String, usize>,
    ) -> Option<String> {
        if map.is_empty() {
            return None;
        }
        Some(
            map.iter()
                .map(|(key, count)| format!("{key}={count}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }

    pub(super) fn record_materialize_rejection_reason(
        &self,
        reason: MaterializationRejectionReason,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let mut summary = self.materialize_owner_repartition.borrow_mut();
        Self::bump_materialize_owner_histogram(
            &mut summary.materialization_rejection_reason,
            format!("{reason:?}"),
        );
    }

    pub(in crate::nir::builder) fn trace_materialize_owner_repartition_summary(&self) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let summary = self.materialize_owner_repartition.borrow();
        let families = [
            (
                "alias_unsafe_hazard_kind",
                Self::format_materialize_owner_histogram(&summary.alias_unsafe_hazard_kind),
            ),
            (
                "disallowed_single_consumer_reason",
                Self::format_materialize_owner_histogram(
                    &summary.disallowed_single_consumer_reason,
                ),
            ),
            (
                "disallowed_single_consumer_consumer_kind",
                Self::format_materialize_owner_histogram(
                    &summary.disallowed_single_consumer_consumer_kind,
                ),
            ),
            (
                "disallowed_single_consumer_rhs_kind",
                Self::format_materialize_owner_histogram(
                    &summary.disallowed_single_consumer_rhs_kind,
                ),
            ),
            (
                "single_consumer_call_rhs_family",
                Self::format_materialize_owner_histogram(&summary.single_consumer_call_rhs_family),
            ),
            (
                "single_consumer_call_rhs_effect_source",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_call_rhs_effect_source,
                ),
            ),
            (
                "single_consumer_call_rhs_consumer_kind",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_call_rhs_consumer_kind,
                ),
            ),
            (
                "single_consumer_call_rhs_downstream_opcode",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_call_rhs_downstream_opcode,
                ),
            ),
            (
                "carry_intrinsic_predicate_family",
                Self::format_materialize_owner_histogram(&summary.carry_intrinsic_predicate_family),
            ),
            (
                "carry_intrinsic_boolor_downstream_use",
                Self::format_materialize_owner_histogram(
                    &summary.carry_intrinsic_boolor_downstream_use,
                ),
            ),
            (
                "carry_intrinsic_final_predicate_context",
                Self::format_materialize_owner_histogram(
                    &summary.carry_intrinsic_final_predicate_context,
                ),
            ),
            (
                "intrinsic_compare_only_family",
                Self::format_materialize_owner_histogram(&summary.intrinsic_compare_only_family),
            ),
            (
                "intrinsic_compare_only_final_predicate_context",
                Self::format_materialize_owner_histogram(
                    &summary.intrinsic_compare_only_final_predicate_context,
                ),
            ),
            (
                "single_consumer_load_rhs_family",
                Self::format_materialize_owner_histogram(&summary.single_consumer_load_rhs_family),
            ),
            (
                "single_consumer_load_rhs_alias_class",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_load_rhs_alias_class,
                ),
            ),
            (
                "missing_merge_binding_relation",
                Self::format_materialize_owner_histogram(&summary.missing_merge_binding_relation),
            ),
            (
                "join_merge_missing_reason",
                Self::format_materialize_owner_histogram(&summary.join_merge_missing_reason),
            ),
            (
                "merge_binding_candidate_result",
                Self::format_materialize_owner_histogram(&summary.merge_binding_candidate_result),
            ),
            (
                "merge_binding_candidate_incoming_kind",
                Self::format_materialize_owner_histogram(
                    &summary.merge_binding_candidate_incoming_kind,
                ),
            ),
            (
                "missing_incoming_pred_kind",
                Self::format_materialize_owner_histogram(&summary.missing_incoming_pred_kind),
            ),
            (
                "missing_no_prior_def_reason",
                Self::format_materialize_owner_histogram(&summary.missing_no_prior_def_reason),
            ),
            (
                "temp_only_representative_reason",
                Self::format_materialize_owner_histogram(&summary.temp_only_representative_reason),
            ),
            (
                "stable_representative_owner_reason",
                Self::format_materialize_owner_histogram(
                    &summary.stable_representative_owner_reason,
                ),
            ),
            (
                "stable_representative_consumer_kind",
                Self::format_materialize_owner_histogram(
                    &summary.stable_representative_consumer_kind,
                ),
            ),
            (
                "stable_representative_downstream_opcode",
                Self::format_materialize_owner_histogram(
                    &summary.stable_representative_downstream_opcode,
                ),
            ),
            (
                "dominating_prior_def_proof_result",
                Self::format_materialize_owner_histogram(
                    &summary.dominating_prior_def_proof_result,
                ),
            ),
            (
                "unknown_missing_merge_attribution_reason",
                Self::format_materialize_owner_histogram(
                    &summary.unknown_missing_merge_attribution_reason,
                ),
            ),
            (
                "unknown_missing_merge_consumer_kind",
                Self::format_materialize_owner_histogram(
                    &summary.unknown_missing_merge_consumer_kind,
                ),
            ),
            (
                "unknown_missing_merge_rhs_kind",
                Self::format_materialize_owner_histogram(&summary.unknown_missing_merge_rhs_kind),
            ),
            (
                "synthetic_root_merge_attribution_reason",
                Self::format_materialize_owner_histogram(
                    &summary.synthetic_root_merge_attribution_reason,
                ),
            ),
            (
                "forward_join_not_selected_rejected_reason",
                Self::format_materialize_owner_histogram(
                    &summary.forward_join_not_selected_rejected_reason,
                ),
            ),
            (
                "ambiguous_join_pred_reason",
                Self::format_materialize_owner_histogram(&summary.ambiguous_join_pred_reason),
            ),
            (
                "unknown_consumer_kind_reason",
                Self::format_materialize_owner_histogram(&summary.unknown_consumer_kind_reason),
            ),
            (
                "unknown_consumer_kind_opcode",
                Self::format_materialize_owner_histogram(&summary.unknown_consumer_kind_opcode),
            ),
            (
                "popcount_consumer_result_use",
                Self::format_materialize_owner_histogram(&summary.popcount_consumer_result_use),
            ),
            (
                "popcount_consumer_downstream_opcode",
                Self::format_materialize_owner_histogram(
                    &summary.popcount_consumer_downstream_opcode,
                ),
            ),
            (
                "popcount_intand_mask_kind",
                Self::format_materialize_owner_histogram(&summary.popcount_intand_mask_kind),
            ),
            (
                "popcount_intand_downstream_use",
                Self::format_materialize_owner_histogram(&summary.popcount_intand_downstream_use),
            ),
            (
                "parity_chain_regression_role",
                Self::format_materialize_owner_histogram(&summary.parity_chain_regression_role),
            ),
            (
                "parity_chain_regression_before_event",
                Self::format_materialize_owner_histogram(
                    &summary.parity_chain_regression_before_event,
                ),
            ),
            (
                "parity_chain_regression_consumer_context",
                Self::format_materialize_owner_histogram(
                    &summary.parity_chain_regression_consumer_context,
                ),
            ),
            (
                "single_consumer_predicate_family",
                Self::format_materialize_owner_histogram(&summary.single_consumer_predicate_family),
            ),
            (
                "single_consumer_predicate_guard_family",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_predicate_guard_family,
                ),
            ),
            (
                "single_consumer_predicate_same_guard",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_predicate_same_guard,
                ),
            ),
            (
                "single_consumer_predicate_requires_stable",
                Self::format_materialize_owner_histogram(
                    &summary.single_consumer_predicate_requires_stable,
                ),
            ),
            (
                "arithmetic_predicate_shape",
                Self::format_materialize_owner_histogram(&summary.arithmetic_predicate_shape),
            ),
            (
                "arithmetic_predicate_consumer_guard",
                Self::format_materialize_owner_histogram(
                    &summary.arithmetic_predicate_consumer_guard,
                ),
            ),
            (
                "arithmetic_predicate_boolean_width",
                Self::format_materialize_owner_histogram(
                    &summary.arithmetic_predicate_boolean_width,
                ),
            ),
            (
                "arithmetic_predicate_stable_reason",
                Self::format_materialize_owner_histogram(
                    &summary.arithmetic_predicate_stable_reason,
                ),
            ),
            (
                "low_bit_mask_predicate_family",
                Self::format_materialize_owner_histogram(&summary.low_bit_mask_predicate_family),
            ),
            (
                "low_bit_mask_input_origin_kind",
                Self::format_materialize_owner_histogram(&summary.low_bit_mask_input_origin_kind),
            ),
            (
                "low_bit_mask_feeds_only_predicate",
                Self::format_materialize_owner_histogram(
                    &summary.low_bit_mask_feeds_only_predicate,
                ),
            ),
            (
                "low_bit_mask_input_is_boolean_like",
                Self::format_materialize_owner_histogram(
                    &summary.low_bit_mask_input_is_boolean_like,
                ),
            ),
            (
                "materialization_rejection_reason",
                Self::format_materialize_owner_histogram(&summary.materialization_rejection_reason),
            ),
            (
                "malformed_def_use_window_relation",
                Self::format_materialize_owner_histogram(
                    &summary.malformed_def_use_window_relation,
                ),
            ),
            (
                "cross_block_consumer_relation",
                Self::format_materialize_owner_histogram(&summary.cross_block_consumer_relation),
            ),
            (
                "cross_block_redefinition_relation",
                Self::format_materialize_owner_histogram(
                    &summary.cross_block_redefinition_relation,
                ),
            ),
            (
                "same_block_overwrite_shape_kind",
                Self::format_materialize_owner_histogram(&summary.same_block_overwrite_shape_kind),
            ),
            (
                "loop_carried_value_kind",
                Self::format_materialize_owner_histogram(&summary.loop_carried_value_kind),
            ),
            (
                "loop_boolean_guard_family",
                Self::format_materialize_owner_histogram(&summary.loop_boolean_guard_family),
            ),
        ];
        for (family, values) in families {
            if let Some(values) = values {
                self.emit_ready_trace(format!(
                    "materialize-owner-repartition family={} values=[{}]",
                    family, values
                ));
            }
        }
    }

    pub(super) fn trace_materialization_plan(
        &self,
        block_addr: u64,
        op: &PcodeOp,
        output: &Varnode,
        rhs: &HirExpr,
        plan: ReplacementValuePlan,
        event: &str,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let reason = match plan.completeness {
            ReplacementCompleteness::Complete => "Complete".to_string(),
            ReplacementCompleteness::Incomplete(reason) => format!("{reason:?}"),
        };
        self.emit_ready_trace(format!(
            "materialization_drift event={} block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} dominant_read={:?} reason={} rhs={:?}",
            event,
            block_addr,
            op.seq_num,
            output.space_id,
            output.offset,
            output.size,
            plan.dominant_read,
            reason,
            rhs,
        ));
    }

    pub(super) fn trace_alias_unsafe_hazard(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        hazard: AliasUnsafeHazard,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.alias_unsafe_hazard_kind,
                format!("{:?}", hazard.kind),
            );
        }
        let use_stmt_idx = hazard
            .use_stmt_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let hazard_stmt_idx = hazard
            .hazard_stmt_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let hazard_op = hazard
            .hazard_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "alias-unsafe-shape output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} use_block=0x{:x} first_alias_hazard={:?} use_stmt_idx={} hazard_stmt={} hazard_op={}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            block_addr,
            hazard.kind,
            use_stmt_idx,
            hazard_stmt_idx,
            hazard_op,
        ));
        if hazard.kind == AliasUnsafeHazardKind::DisallowedSingleConsumer {
            self.trace_disallowed_single_consumer(block_addr, op_seq, output, rhs);
        }
        if matches!(
            hazard.kind,
            AliasUnsafeHazardKind::UnknownNoConsumerFound
                | AliasUnsafeHazardKind::UnknownConsumerAfterTerminator
                | AliasUnsafeHazardKind::UnknownUnhandledConsumerKind
                | AliasUnsafeHazardKind::UnknownMalformedDefUseWindow
        ) {
            self.trace_alias_unsafe_unknown_shape(block_addr, op_seq, output, rhs, hazard);
        }
    }

    pub(super) fn trace_alias_unsafe_unknown_shape(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        hazard: AliasUnsafeHazard,
    ) {
        let Some(block) = self
            .pcode
            .blocks
            .iter()
            .find(|block| block.start_address == block_addr)
        else {
            return;
        };
        let Some(op_idx) = block.ops.iter().position(|op| op.seq_num == op_seq) else {
            return;
        };
        let terminator_index = self.block_terminator_index(block);
        let same_block_consumers = Self::collect_output_use_sites_in_block(block, op_idx, output);
        let consumer_count = same_block_consumers.len();
        let first_consumer = same_block_consumers.first().copied();
        let first_consumer_stmt = first_consumer
            .map(|(idx, _)| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_op = first_consumer
            .map(|(_, op)| format!("{:?}", op.opcode))
            .unwrap_or_else(|| "None".to_string());
        let first_consumer_relation = match (first_consumer, terminator_index) {
            (Some((idx, _)), Some(term_idx)) if idx > term_idx => "AfterTerminator",
            (Some(_), _) => "BetweenDefAndTerminator",
            (None, Some(term_idx)) if op_idx > term_idx => "BeforeDef",
            (None, _) => "None",
        };
        let terminator_idx = terminator_index
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "alias-unsafe-unknown-shape output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} terminator_idx={} consumer_count={} same_block_consumers={} first_consumer_stmt={} first_consumer_op={} first_consumer_relation={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            terminator_idx,
            consumer_count,
            consumer_count,
            first_consumer_stmt,
            first_consumer_op,
            first_consumer_relation,
            hazard.kind,
        ));
        if hazard.kind == AliasUnsafeHazardKind::UnknownMalformedDefUseWindow {
            self.trace_malformed_def_use_window(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_disallowed_single_consumer(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(block) = self
            .pcode
            .blocks
            .iter()
            .find(|block| block.start_address == block_addr)
        else {
            return;
        };
        let Some(op_idx) = block.ops.iter().position(|op| op.seq_num == op_seq) else {
            return;
        };
        let Some(proof) =
            Self::describe_disallowed_single_consumer_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.disallowed_single_consumer_reason,
                format!("{:?}", proof.reason),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.disallowed_single_consumer_consumer_kind,
                format!("{:?}", proof.consumer_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.disallowed_single_consumer_rhs_kind,
                format!("{:?}", proof.rhs_kind),
            );
        }
        self.emit_ready_trace(format!(
            "disallowed-single-consumer output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_block=0x{:x} consumer_op_seq={} consumer_opcode={:?} consumer_kind={:?} rhs_kind={:?} rhs_low_cost={} rhs_has_load={} rhs_has_call={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            proof.consumer_block_addr,
            proof.consumer_op_seq,
            proof.consumer_opcode,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.rhs_low_cost,
            proof.rhs_has_load,
            proof.rhs_has_call,
            proof.reason,
        ));
        if proof.reason == DisallowedSingleConsumerReason::RhsHasCall {
            self.trace_single_consumer_call_rhs_proof(block, op_idx, output, rhs);
        } else if proof.reason == DisallowedSingleConsumerReason::RhsHasLoad {
            self.trace_single_consumer_load_rhs_proof(block, op_idx, output, rhs);
        } else if proof.reason == DisallowedSingleConsumerReason::ConsumerIsPredicate {
            self.trace_single_consumer_predicate_proof(block, op_idx, output, rhs);
        } else if proof.reason == DisallowedSingleConsumerReason::UnknownConsumerKind {
            self.trace_unknown_consumer_kind(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_single_consumer_call_rhs_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_single_consumer_call_rhs_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_call_rhs_family,
                format!("{:?}", proof.family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_call_rhs_effect_source,
                proof
                    .call_effect_source
                    .map(|source| format!("{source:?}"))
                    .unwrap_or_else(|| "None".to_string()),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_call_rhs_consumer_kind,
                format!("{:?}", proof.consumer_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_call_rhs_downstream_opcode,
                proof
                    .downstream_opcode
                    .map(|opcode| format!("{opcode:?}"))
                    .unwrap_or_else(|| "None".to_string()),
            );
        }
        let call_effect_source = proof
            .call_effect_source
            .map(|source| format!("{source:?}"))
            .unwrap_or_else(|| "None".to_string());
        let writes_memory = proof
            .writes_memory
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let may_call_unknown = proof
            .may_call_unknown
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let may_exit = proof
            .may_exit
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let downstream_opcode = proof
            .downstream_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "single-consumer-call-rhs-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_op_seq={} call_target={} family={:?} call_effect_source={} writes_memory={} may_call_unknown={} may_exit={} return_used={} consumer_kind={:?} downstream_opcode={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_op_seq,
            proof.call_target,
            proof.family,
            call_effect_source,
            writes_memory,
            may_call_unknown,
            may_exit,
            proof.return_used,
            proof.consumer_kind,
            downstream_opcode,
        ));
        if matches!(proof.call_target.as_str(), "__carry" | "__scarry")
            && proof.consumer_kind == DisallowedSingleConsumerConsumerKind::Predicate
        {
            self.trace_carry_intrinsic_predicate_proof(block, op_idx, output, rhs);
        }
        if proof.family == SingleConsumerCallRhsFamily::KnownPureIntrinsic
            && matches!(
                proof.consumer_opcode,
                PcodeOpcode::IntEqual | PcodeOpcode::IntNotEqual
            )
        {
            self.trace_intrinsic_compare_only_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_carry_intrinsic_predicate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_carry_intrinsic_predicate_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.carry_intrinsic_predicate_family,
                format!("{:?}", proof.bool_chain_role),
            );
            if let Some(boolor_use) = proof.boolor_downstream_use {
                Self::bump_materialize_owner_histogram(
                    &mut summary.carry_intrinsic_boolor_downstream_use,
                    format!("{:?}", boolor_use),
                );
            }
            Self::bump_materialize_owner_histogram(
                &mut summary.carry_intrinsic_final_predicate_context,
                format!("{:?}", proof.final_predicate_context),
            );
        }
        self.emit_ready_trace(format!(
            "carry-intrinsic-predicate-proof output=space:{} off:0x{:x} size:{} call_target={} args={:?} consumer_kind={:?} downstream_opcode={:?} bool_chain_role={:?} rhs_low_cost={} args_side_effect_free={} final_predicate_context={:?}",
            output.space_id,
            output.offset,
            output.size,
            proof.call_target,
            proof.args,
            proof.consumer_kind,
            proof.downstream_opcode,
            proof.bool_chain_role,
            proof.rhs_low_cost,
            proof.args_side_effect_free,
            proof.final_predicate_context,
        ));
    }

    pub(super) fn trace_intrinsic_compare_only_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_intrinsic_compare_only_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.intrinsic_compare_only_family,
                format!("{:?}", proof.family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.intrinsic_compare_only_final_predicate_context,
                format!("{:?}", proof.final_predicate_context),
            );
        }
        self.emit_ready_trace(format!(
            "intrinsic-compare-only-proof output=space:{} off:0x{:x} size:{} call_target={} args={:?} downstream_opcode={:?} compare_const={:?} rhs_low_cost={} args_side_effect_free={} final_predicate_context={:?}",
            output.space_id,
            output.offset,
            output.size,
            proof.call_target,
            proof.args,
            proof.downstream_opcode,
            proof.compare_const,
            proof.rhs_low_cost,
            proof.args_side_effect_free,
            proof.final_predicate_context,
        ));
    }

    pub(super) fn trace_single_consumer_load_rhs_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = Self::describe_single_consumer_load_rhs_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_load_rhs_family,
                format!("{:?}", proof.family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_load_rhs_alias_class,
                format!("{:?}", proof.alias_class),
            );
        }
        self.emit_ready_trace(format!(
            "single-consumer-load-rhs-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_op_seq={} load_ptr={} consumer_kind={:?} downstream_opcode={:?} alias_class={:?} same_block_store_before={} same_block_store_after={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_op_seq,
            proof.load_ptr,
            proof.consumer_kind,
            proof.consumer_opcode,
            proof.alias_class,
            proof.same_block_store_before,
            proof.same_block_store_after,
        ));
    }

    pub(super) fn trace_missing_merge_binding_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.missing_merge_binding_relation,
                format!("{:?}", proof.relation),
            );
        }
        self.emit_ready_trace(format!(
            "missing-merge-binding-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} merge_block=0x{:x} predecessor_count={} incoming_value_count={} has_existing_binding={} consumer_kind={:?} rhs_kind={:?} relation={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.merge_block,
            proof.predecessor_count,
            proof.incoming_value_count,
            proof.has_existing_binding,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.relation,
        ));
        if proof.relation == MissingMergeBindingRelation::RepresentativeOnlyMissing {
            self.trace_temp_only_representative_proof(
                proof.merge_block,
                None,
                output,
                proof.consumer_kind,
                proof.rhs_kind,
                "RepresentativeOnlyMissing",
                false,
                false,
            );
        }
        if proof.relation == MissingMergeBindingRelation::JoinMergeMissing {
            self.trace_join_merge_missing_proof(block, op_idx, output, rhs);
            self.trace_merge_binding_candidate_proof(block, op_idx, output, rhs);
        }
        if proof.relation == MissingMergeBindingRelation::UnknownMissingMerge {
            self.trace_unknown_missing_merge_attribution(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_join_merge_missing_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_join_merge_missing_proof(block, op_idx, output, rhs) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.join_merge_missing_reason,
                format!("{:?}", proof.reason),
            );
        }
        let predecessor_blocks = proof
            .predecessor_blocks
            .iter()
            .map(|addr| format!("0x{addr:x}"))
            .collect::<Vec<_>>()
            .join(",");
        let incoming_values = proof.incoming_values.join(" | ");
        self.emit_ready_trace(format!(
            "join-merge-missing-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} event_block=0x{:x} merge_block=0x{:x} predecessor_count={} predecessor_blocks=[{}] incoming_value_count={} incoming_values=[{}] values_same_across_preds={} has_missing_incoming={} has_conflicting_incoming={} consumer_kind={:?} rhs_kind={:?} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.event_block,
            proof.merge_block,
            proof.predecessor_blocks.len(),
            predecessor_blocks,
            proof.incoming_value_count,
            incoming_values,
            proof.values_same_across_preds,
            proof.has_missing_incoming,
            proof.has_conflicting_incoming,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.reason,
        ));
        if proof.reason == JoinMergeMissingReason::MissingIncomingForSomePred {
            self.trace_missing_incoming_pred_proof(
                proof.event_block,
                proof.merge_block,
                output,
                proof.consumer_kind,
                proof.rhs_kind,
            );
        }
    }

    pub(super) fn trace_merge_binding_candidate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.merge_binding_candidate_result,
                format!("{:?}", proof.result),
            );
            for kind in &proof.incoming_value_kinds {
                Self::bump_materialize_owner_histogram(
                    &mut summary.merge_binding_candidate_incoming_kind,
                    format!("{:?}", kind),
                );
            }
        }
        let incoming_value_kinds = proof
            .incoming_value_kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(",");
        self.emit_ready_trace(format!(
            "merge-binding-candidate-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} merge_block=0x{:x} predecessor_count={} missing_incoming_count={} conflicting_incoming_count={} incoming_value_kinds=[{}] consumer_kind={:?} rhs_kind={:?} can_synthesize_phi_like_binding={} result={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.merge_block,
            proof.predecessor_count,
            proof.missing_incoming_count,
            proof.conflicting_incoming_count,
            incoming_value_kinds,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.can_synthesize_phi_like_binding,
            proof.result,
        ));
    }

    pub(super) fn trace_unknown_missing_merge_attribution(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) =
            self.describe_unknown_missing_merge_attribution(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.unknown_missing_merge_attribution_reason,
                format!("{:?}", proof.reason),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.unknown_missing_merge_consumer_kind,
                format!("{:?}", proof.consumer_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.unknown_missing_merge_rhs_kind,
                format!("{:?}", proof.rhs_kind),
            );
        }
        self.emit_ready_trace(format!(
            "unknown-missing-merge-attribution output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} merge_block=0x{:x} function_entry_block=0x{:x} merge_block_is_entry={} predecessor_count={} successor_count={} incoming_value_count={} consumer_kind={:?} rhs_kind={:?} output_space={} output_size={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.merge_block,
            proof.function_entry_block,
            proof.merge_block_is_entry,
            proof.predecessor_count,
            proof.successor_count,
            proof.incoming_value_count,
            proof.consumer_kind,
            proof.rhs_kind,
            output.space_id,
            output.size,
            proof.reason,
        ));
        if matches!(
            proof.reason,
            UnknownMissingMergeAttributionReason::SyntheticRootBlock
                | UnknownMissingMergeAttributionReason::OtherDataRepresentative
        ) {
            self.trace_temp_only_representative_proof(
                proof.merge_block,
                None,
                output,
                proof.consumer_kind,
                proof.rhs_kind,
                match proof.reason {
                    UnknownMissingMergeAttributionReason::SyntheticRootBlock => {
                        "SyntheticRootBlock"
                    }
                    UnknownMissingMergeAttributionReason::OtherDataRepresentative => {
                        "OtherDataRepresentative"
                    }
                    _ => "UnknownRepresentative",
                },
                proof.reason == UnknownMissingMergeAttributionReason::SyntheticRootBlock,
                false,
            );
        }
        if proof.reason == UnknownMissingMergeAttributionReason::SyntheticRootBlock {
            self.trace_synthetic_root_merge_attribution(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_synthetic_root_merge_attribution(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) =
            self.describe_synthetic_root_merge_attribution(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.synthetic_root_merge_attribution_reason,
                format!("{:?}", proof.reason),
            );
        }
        let nearest_join_block = proof
            .nearest_join_block
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let nearest_join_distance = proof
            .nearest_join_distance
            .map(|distance| distance.to_string())
            .unwrap_or_else(|| "none".to_string());
        let nearest_postdom_join = proof
            .nearest_postdom_join
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let postdom_distance = proof
            .postdom_distance
            .map(|distance| distance.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "synthetic-root-merge-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} event_block=0x{:x} entry_block=0x{:x} selected_merge_block=0x{:x} selected_is_entry={} block_is_entry={} event_block_is_entry={} event_block_dominates={} nearest_join_block={} nearest_join_distance={} nearest_postdom_join={} postdom_distance={} block_successor_count={} entry_successor_count={} consumer_kind={:?} rhs_kind={:?} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.event_block,
            proof.entry_block,
            proof.selected_merge_block,
            proof.selected_is_entry,
            block.start_address == proof.entry_block,
            proof.event_block_is_entry,
            proof.event_block_dominates,
            nearest_join_block,
            nearest_join_distance,
            nearest_postdom_join,
            postdom_distance,
            proof.block_successor_count,
            proof.entry_successor_count,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.reason,
        ));
        if proof.reason == SyntheticRootMergeAttributionReason::ForwardJoinExistsButNotSelected {
            self.trace_forward_join_not_selected_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_forward_join_not_selected_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_forward_join_not_selected_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.forward_join_not_selected_rejected_reason,
                format!("{:?}", proof.rejected_reason),
            );
        }
        let forward_join_distance = proof
            .forward_join_distance
            .map(|distance| distance.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "forward-join-not-selected-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} event_block=0x{:x} selected_merge_block=0x{:x} forward_join_block=0x{:x} forward_join_distance={} forward_join_predecessor_count={} forward_join_successor_count={} event_reaches_forward_join={} forward_join_postdominates_event={} consumer_kind={:?} rhs_kind={:?} rejected_reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.event_block,
            proof.selected_merge_block,
            proof.forward_join_block,
            forward_join_distance,
            proof.forward_join_predecessor_count,
            proof.forward_join_successor_count,
            proof.event_reaches_forward_join,
            proof.forward_join_postdominates_event,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.rejected_reason,
        ));
        if proof.rejected_reason
            == ForwardJoinNotSelectedRejectedReason::JoinHasMultipleAmbiguousPreds
        {
            self.trace_ambiguous_join_pred_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_ambiguous_join_pred_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_ambiguous_join_pred_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.ambiguous_join_pred_reason,
                format!("{:?}", proof.reason),
            );
        }
        let predecessor_blocks = proof
            .predecessor_blocks
            .iter()
            .map(|addr| format!("0x{addr:x}"))
            .collect::<Vec<_>>()
            .join(",");
        let incoming_values = proof.incoming_values.join(" | ");
        let event_pred_index = proof
            .event_pred_index
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let event_pred_value = proof
            .event_pred_value
            .clone()
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "ambiguous-join-pred-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} event_block=0x{:x} forward_join_block=0x{:x} predecessor_count={} predecessor_blocks=[{}] incoming_value_count={} incoming_values=[{}] event_pred_index={} event_pred_value={} values_same_across_preds={} has_missing_incoming={} has_conflicting_incoming={} consumer_kind={:?} rhs_kind={:?} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.event_block,
            proof.forward_join_block,
            proof.predecessor_blocks.len(),
            predecessor_blocks,
            proof.incoming_value_count,
            incoming_values,
            event_pred_index,
            event_pred_value,
            proof.values_same_across_preds,
            proof.has_missing_incoming,
            proof.has_conflicting_incoming,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.reason,
        ));
        if proof.reason == AmbiguousJoinPredReason::MissingIncomingForSomePred {
            self.trace_missing_incoming_pred_proof(
                proof.event_block,
                proof.forward_join_block,
                output,
                proof.consumer_kind,
                proof.rhs_kind,
            );
        }
    }

    pub(super) fn trace_missing_incoming_pred_proof(
        &self,
        event_block: u64,
        merge_block: u64,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let proofs = self.describe_missing_incoming_pred_proofs(event_block, merge_block, output);
        for proof in proofs {
            {
                let mut summary = self.materialize_owner_repartition.borrow_mut();
                Self::bump_materialize_owner_histogram(
                    &mut summary.missing_incoming_pred_kind,
                    format!("{:?}", proof.incoming_kind),
                );
            }
            let prior_def_block = proof
                .prior_def_block
                .map(|addr| format!("0x{addr:x}"))
                .unwrap_or_else(|| "none".to_string());
            let prior_def_op_seq = proof
                .prior_def_op_seq
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "none".to_string());
            self.emit_ready_trace(format!(
                "missing-incoming-pred-proof output=space:{} off:0x{:x} size:{} event_block=0x{:x} merge_block=0x{:x} pred_block=0x{:x} pred_reaches_merge={} pred_has_definition={} pred_has_prior_definition={} prior_def_block={} prior_def_op_seq={} incoming_kind={:?}",
                output.space_id,
                output.offset,
                output.size,
                proof.event_block,
                proof.merge_block,
                proof.pred_block,
                proof.pred_reaches_merge,
                proof.pred_has_definition,
                proof.pred_has_prior_definition,
                prior_def_block,
                prior_def_op_seq,
                proof.incoming_kind,
            ));
            if proof.incoming_kind == MissingIncomingPredKind::MissingBecausePriorDefDominates {
                self.trace_dominating_prior_def_incoming_proof(
                    merge_block,
                    proof.pred_block,
                    output,
                    consumer_kind,
                    rhs_kind,
                );
            } else if matches!(
                proof.incoming_kind,
                MissingIncomingPredKind::MissingBecauseNoPriorDef
                    | MissingIncomingPredKind::MissingBecauseEntryDefault
                    | MissingIncomingPredKind::MissingBecauseDeadPred
            ) {
                self.trace_missing_no_prior_def_proof(
                    merge_block,
                    proof.pred_block,
                    output,
                    consumer_kind,
                    rhs_kind,
                );
            }
        }
    }

    pub(super) fn trace_missing_no_prior_def_proof(
        &self,
        merge_block: u64,
        pred_block: u64,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_missing_no_prior_def_proof(
            merge_block,
            pred_block,
            output,
            consumer_kind,
            rhs_kind,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.missing_no_prior_def_reason,
                format!("{:?}", proof.reason),
            );
        }
        self.emit_ready_trace(format!(
            "missing-no-prior-def-proof output=space:{} off:0x{:x} size:{} merge_block=0x{:x} pred_block=0x{:x} pred_reaches_merge={} pred_is_entry={} pred_is_dead={} output_space={} output_size={} consumer_kind={:?} rhs_kind={:?} default_candidate={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            proof.merge_block,
            proof.pred_block,
            proof.pred_reaches_merge,
            proof.pred_is_entry,
            proof.pred_is_dead,
            proof.output_space,
            proof.output_size,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.default_candidate,
            proof.reason,
        ));
        if matches!(
            proof.reason,
            MissingNoPriorDefReason::TempOnlyNoDef | MissingNoPriorDefReason::DeadPredNoDef
        ) {
            self.trace_temp_only_representative_proof(
                merge_block,
                Some(pred_block),
                output,
                consumer_kind,
                rhs_kind,
                match proof.reason {
                    MissingNoPriorDefReason::TempOnlyNoDef => "TempOnlyNoDef",
                    MissingNoPriorDefReason::DeadPredNoDef => "DeadPredNoDef",
                    _ => "UnknownNoPriorDef",
                },
                false,
                proof.reason == MissingNoPriorDefReason::DeadPredNoDef,
            );
        }
    }

    pub(super) fn trace_temp_only_representative_proof(
        &self,
        merge_block: u64,
        pred_block: Option<u64>,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
        source_event: &str,
        root_attributed: bool,
        dead_pred: bool,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_temp_only_representative_proof(
            merge_block,
            pred_block,
            output,
            consumer_kind,
            rhs_kind,
            source_event,
            root_attributed,
            dead_pred,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.temp_only_representative_reason,
                format!("{:?}", proof.reason),
            );
        }
        let pred_block = proof
            .pred_block
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "temp-only-representative-proof output=space:{} off:0x{:x} size:{} merge_block=0x{:x} pred_block={} consumer_kind={:?} rhs_kind={:?} defining_event={} materialization_event={} has_real_storage={} has_later_use={} crosses_merge={} root_attributed={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            proof.merge_block,
            pred_block,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.defining_event,
            proof.materialization_event,
            proof.has_real_storage,
            proof.has_later_use,
            proof.crosses_merge,
            proof.root_attributed,
            proof.reason,
        ));
    }

    pub(super) fn classify_nonlocal_materialization_rejection_reason(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> MaterializationRejectionReason {
        let Some(proof) =
            self.describe_temp_only_representative_site_proof(block, op_idx, output, rhs)
        else {
            return MaterializationRejectionReason::MissingMergeBinding;
        };
        match proof.reason {
            TempOnlyRepresentativeReason::RootAttributedTemp => {
                MaterializationRejectionReason::RepresentativeRootAttribution
            }
            TempOnlyRepresentativeReason::DeadTempRepresentative => {
                MaterializationRejectionReason::DeadTempRepresentative
            }
            TempOnlyRepresentativeReason::TempRepresentativeResidue
            | TempOnlyRepresentativeReason::MergeCrossingTemp
            | TempOnlyRepresentativeReason::StoreValueTemp
            | TempOnlyRepresentativeReason::OtherDataTemp
            | TempOnlyRepresentativeReason::UnknownTempRepresentative => {
                MaterializationRejectionReason::TempOnlyRepresentativeLifecycle
            }
        }
    }

    pub(super) fn trace_stable_representative_owner_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_stable_representative_owner_proof(
            block,
            op_idx,
            terminator_index,
            output,
            rhs,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.stable_representative_owner_reason,
                format!("{:?}", proof.reason),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.stable_representative_consumer_kind,
                format!("{:?}", proof.consumer_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.stable_representative_downstream_opcode,
                proof
                    .downstream_opcode
                    .map(|opcode| format!("{:?}", opcode))
                    .unwrap_or_else(|| "none".to_string()),
            );
        }
        self.emit_ready_trace(format!(
            "stable-representative-owner-proof output=space:{} off:0x{:x} size:{} block=0x{:x} op_seq={} consumer_kind={:?} rhs_kind={:?} overlaps_representative_root_attribution={} overlaps_temp_only_lifecycle={} overlaps_real_missing_merge={} downstream_opcode={:?} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_kind,
            proof.rhs_kind,
            proof.overlaps_representative_root_attribution,
            proof.overlaps_temp_only_lifecycle,
            proof.overlaps_real_missing_merge,
            proof.downstream_opcode,
            proof.reason,
        ));
    }

    pub(super) fn trace_dominating_prior_def_incoming_proof(
        &self,
        merge_block: u64,
        pred_block: u64,
        output: &Varnode,
        consumer_kind: DisallowedSingleConsumerConsumerKind,
        rhs_kind: DisallowedSingleConsumerRhsKind,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_dominating_prior_def_incoming_proof(
            merge_block,
            pred_block,
            output,
            consumer_kind,
            rhs_kind,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.dominating_prior_def_proof_result,
                format!("{:?}", proof.proof_result),
            );
        }
        self.emit_ready_trace(format!(
            "dominating-prior-def-incoming-proof output=space:{} off:0x{:x} size:{} merge_block=0x{:x} pred_block=0x{:x} prior_def_block=0x{:x} prior_def_op_seq={} prior_def_rhs={} prior_def_dominates_pred={} prior_def_dominates_merge={} redefined_between_prior_and_merge={} redefined_on_pred_path={} consumer_kind={:?} rhs_kind={:?} proof_result={:?}",
            output.space_id,
            output.offset,
            output.size,
            proof.merge_block,
            proof.pred_block,
            proof.prior_def_block,
            proof.prior_def_op_seq,
            proof.prior_def_rhs,
            proof.prior_def_dominates_pred,
            proof.prior_def_dominates_merge,
            proof.redefined_between_prior_and_merge,
            proof.redefined_on_pred_path,
            proof.consumer_kind,
            proof.rhs_kind,
            proof.proof_result,
        ));
    }

    pub(super) fn trace_unknown_consumer_kind(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = Self::describe_unknown_consumer_kind_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.unknown_consumer_kind_reason,
                format!("{:?}", proof.reason),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.unknown_consumer_kind_opcode,
                format!("{:?}", proof.consumer_opcode),
            );
        }
        self.emit_ready_trace(format!(
            "unknown-consumer-kind output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_block=0x{:x} consumer_op_seq={} consumer_opcode={:?} matched_input_indices={:?} rhs_kind={:?} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_block_addr,
            proof.consumer_op_seq,
            proof.consumer_opcode,
            proof.matched_input_indices,
            proof.rhs_kind,
            proof.reason,
        ));
        if proof.consumer_opcode == PcodeOpcode::PopCount {
            self.trace_popcount_consumer_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_popcount_consumer_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_popcount_consumer_proof(block, op_idx, output, rhs) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.popcount_consumer_result_use,
                format!("{:?}", proof.popcount_result_used_by),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.popcount_consumer_downstream_opcode,
                proof
                    .downstream_consumer_opcode
                    .map(|opcode| format!("{opcode:?}"))
                    .unwrap_or_else(|| "None".to_string()),
            );
        }
        let output_width = proof
            .output_width
            .map(|width| width.to_string())
            .unwrap_or_else(|| "none".to_string());
        let downstream_consumer_opcode = proof
            .downstream_consumer_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "popcount-consumer-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_op_seq={} input_width={} output_width={} rhs_kind={:?} rhs={:?} rhs_has_call={} rhs_has_load={} rhs_low_cost={} popcount_result_used_by={:?} downstream_consumer_opcode={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_op_seq,
            proof.input_width,
            output_width,
            proof.rhs_kind,
            rhs,
            proof.rhs_has_call,
            proof.rhs_has_load,
            proof.rhs_low_cost,
            proof.popcount_result_used_by,
            downstream_consumer_opcode,
        ));
        if proof.downstream_consumer_opcode == Some(PcodeOpcode::IntAnd) {
            self.trace_popcount_intand_chain_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_popcount_intand_chain_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_popcount_intand_chain_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.popcount_intand_mask_kind,
                format!("{:?}", proof.intand_mask_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.popcount_intand_downstream_use,
                format!("{:?}", proof.intand_result_consumer),
            );
        }
        let intand_mask = proof
            .intand_mask
            .map(|value| format!("0x{value:x}"))
            .unwrap_or_else(|| "none".to_string());
        let downstream_consumer_opcode = proof
            .downstream_consumer_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "popcount-intand-chain-proof output=space:{} off:0x{:x} size:{} popcount_input_rhs={:?} popcount_result={} def_block=0x{:x} def_op_seq={} consumer_op_seq={} intand_op_seq={} intand_mask={} intand_mask_kind={:?} intand_result_consumer={:?} downstream_consumer_opcode={} chain_low_cost={} chain_side_effect_free={}",
            output.space_id,
            output.offset,
            output.size,
            rhs,
            proof.popcount_result,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.popcount_consumer_op_seq,
            proof.intand_op_seq,
            intand_mask,
            proof.intand_mask_kind,
            proof.intand_result_consumer,
            downstream_consumer_opcode,
            proof.chain_low_cost,
            proof.chain_side_effect_free,
        ));
    }

    fn classify_parity_chain_consumer_context(
        proof: &ParityChainProof,
    ) -> ParityChainConsumerContext {
        match (proof.compare_opcode, proof.compare_const) {
            (PcodeOpcode::IntEqual, 0) => ParityChainConsumerContext::CompareZero,
            (PcodeOpcode::IntNotEqual, 0) => ParityChainConsumerContext::CompareNonZero,
            (PcodeOpcode::IntEqual, 1) => ParityChainConsumerContext::CompareOne,
            (PcodeOpcode::IntNotEqual, 1) => ParityChainConsumerContext::CompareNotOne,
            _ => ParityChainConsumerContext::CompareZero,
        }
    }

    pub(super) fn trace_parity_chain_regression_attribution(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
        proof: &ParityChainProof,
        legacy_inline_candidate: bool,
        fallback_plan: ReplacementValuePlan,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let (before_materialized, before_event) = if fallback_plan.is_complete() {
            (false, "representative_downgrade")
        } else if legacy_inline_candidate {
            (true, "inline_suppressed")
        } else {
            (true, "materialized_binding")
        };
        let consumer_context = Self::classify_parity_chain_consumer_context(proof);
        let final_hir_expr = Self::describe_parity_chain_final_hir_expr(rhs, proof)
            .unwrap_or_else(|| format!("{rhs:?}"));
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.parity_chain_regression_role,
                format!("{:?}", proof.role),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.parity_chain_regression_before_event,
                before_event,
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.parity_chain_regression_consumer_context,
                format!("{:?}", consumer_context),
            );
        }
        self.emit_ready_trace(format!(
            "parity-chain-regression-attribution output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} role={:?} popcount_op_seq={} intand_op_seq={} compare_op_seq={} before_materialized={} after_materialized=false before_event={} after_event=parity_chain_materialized final_hir_expr={} consumer_context={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.role,
            proof.popcount_op_seq,
            proof.intand_op_seq,
            proof.compare_op_seq,
            before_materialized,
            before_event,
            final_hir_expr,
            consumer_context,
        ));
    }

    pub(super) fn trace_parity_chain_materialized(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        proof: &ParityChainProof,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "parity-chain-materialized output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} role={:?} popcount_op_seq={} intand_op_seq={} compare_op_seq={} compare_opcode={:?} compare_const={} chain_low_cost={} chain_side_effect_free={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.role,
            proof.popcount_op_seq,
            proof.intand_op_seq,
            proof.compare_op_seq,
            proof.compare_opcode,
            proof.compare_const,
            proof.chain_low_cost,
            proof.chain_side_effect_free,
        ));
    }

    pub(super) fn trace_parity_chain_kept(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        reason: ParityChainKeepReason,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "parity-chain-kept output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} reason={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            reason,
        ));
    }

    pub(super) fn trace_single_consumer_predicate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) =
            Self::describe_single_consumer_predicate_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_predicate_family,
                format!("{:?}", proof.predicate_family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_predicate_guard_family,
                format!("{:?}", proof.guard_family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_predicate_same_guard,
                proof.same_guard_as_consumer.to_string(),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.single_consumer_predicate_requires_stable,
                proof.requires_stable_representative.to_string(),
            );
        }
        self.emit_ready_trace(format!(
            "single-consumer-predicate-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_block=0x{:x} consumer_op_seq={} consumer_opcode={:?} rhs_kind={:?} rhs={:?} predicate_family={:?} guard_family={:?} same_guard_as_consumer={} requires_stable_representative={} low_cost_if_predicate={} has_call={} has_load={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            proof.consumer_block_addr,
            proof.consumer_op_seq,
            proof.consumer_opcode,
            proof.rhs_kind,
            rhs,
            proof.predicate_family,
            proof.guard_family,
            proof.same_guard_as_consumer,
            proof.requires_stable_representative,
            proof.low_cost_if_predicate,
            proof.has_call,
            proof.has_load,
        ));
        if proof.predicate_family == SingleConsumerPredicateFamily::UnknownPredicate {
            self.trace_arithmetic_predicate_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_arithmetic_predicate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = Self::describe_arithmetic_predicate_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.arithmetic_predicate_shape,
                format!("{:?}", proof.mask_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.arithmetic_predicate_consumer_guard,
                format!("{:?}", proof.consumer_guard),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.arithmetic_predicate_boolean_width,
                proof.boolean_width.to_string(),
            );
            if let Some(reason) = proof.stable_required_reason {
                Self::bump_materialize_owner_histogram(
                    &mut summary.arithmetic_predicate_stable_reason,
                    format!("{reason:?}"),
                );
            }
        }
        let mask_value = proof
            .mask_value
            .map(|value| format!("0x{value:x}"))
            .unwrap_or_else(|| "none".to_string());
        let stable_reason = proof
            .stable_required_reason
            .map(|reason| format!("{reason:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "arithmetic-predicate-proof output=space:{} off:0x{:x} size:{} rhs={:?} mask_kind={:?} mask_value={} consumer_guard={:?} boolean_width={} low_cost={} stable_required={} stable_required_reason={}",
            output.space_id,
            output.offset,
            output.size,
            rhs,
            proof.mask_kind,
            mask_value,
            proof.consumer_guard,
            proof.boolean_width,
            proof.low_cost,
            proof.stable_required,
            stable_reason,
        ));
        if proof.mask_kind == ArithmeticPredicateShape::LowBitAndOne {
            self.trace_low_bit_mask_predicate_proof(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_low_bit_mask_predicate_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = Self::describe_low_bit_mask_predicate_proof(block, op_idx, output, rhs)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.low_bit_mask_predicate_family,
                format!("{:?}", proof.family),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.low_bit_mask_input_origin_kind,
                format!("{:?}", proof.input_origin_kind),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.low_bit_mask_feeds_only_predicate,
                proof.feeds_only_predicate.to_string(),
            );
            Self::bump_materialize_owner_histogram(
                &mut summary.low_bit_mask_input_is_boolean_like,
                proof.input_is_boolean_like.to_string(),
            );
        }
        let stable_reason = proof
            .stable_required_reason
            .map(|reason| format!("{reason:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "low-bit-mask-proof output=space:{} off:0x{:x} size:{} rhs={:?} mask_input={} consumer_guard={:?} feeds_only_predicate={} input_is_boolean_like={} input_origin_kind={:?} stable_required_reason={}",
            output.space_id,
            output.offset,
            output.size,
            rhs,
            proof.mask_input,
            proof.consumer_guard,
            proof.feeds_only_predicate,
            proof.input_is_boolean_like,
            proof.input_origin_kind,
            stable_reason,
        ));
    }

    pub(super) fn trace_no_consumer_materialization(
        &self,
        block_addr: u64,
        op_seq: u32,
        event: &str,
        output: &Varnode,
        rhs: &HirExpr,
        preserve_materialization: bool,
        legacy_inline_candidate: bool,
        profile: NoConsumerMaterializationProfile,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-materialization output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?} materialization_event={} preserve_materialization={} legacy_inline_candidate={} has_later_block_use={} has_phi_merge_use={} has_debug_use={} same_block_consumers={} cross_block_consumers={} rhs_side_effectful={}",
            output.space_id,
            output.offset,
            output.size,
            block_addr,
            op_seq,
            rhs,
            event,
            preserve_materialization,
            legacy_inline_candidate,
            profile.has_later_block_use,
            profile.has_phi_merge_use,
            profile.has_debug_use,
            profile.same_block_consumers,
            profile.cross_block_consumers,
            profile.rhs_side_effectful,
        ));
    }

    pub(super) fn trace_malformed_def_use_window(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let detail = self.describe_malformed_def_use_window(block, op_idx, output, rhs);
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.malformed_def_use_window_relation,
                format!("{:?}", detail.relation),
            );
        }
        let terminator_idx = detail
            .terminator_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_block = detail
            .first_consumer_block
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_idx = detail
            .first_consumer_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "none".to_string());
        let first_consumer_op_seq = detail
            .first_consumer_op_seq
            .map(|seq| seq.to_string())
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "malformed-def-use-window output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} def_op_idx={} terminator_idx={} consumer_count={} first_consumer_block={} first_consumer_idx={} first_consumer_op_seq={} relation={:?} rhs_kind={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops
                .get(op_idx)
                .map(|op| op.seq_num.to_string())
                .unwrap_or_else(|| "none".to_string()),
            detail.def_op_idx,
            terminator_idx,
            detail.consumer_count,
            first_consumer_block,
            first_consumer_idx,
            first_consumer_op_seq,
            detail.relation,
            detail.rhs_kind,
        ));
        if detail.relation == MalformedDefUseWindowRelation::ConsumerInDifferentBlock {
            self.trace_cross_block_consumer_provenance(block, op_idx, output, rhs);
        }
    }

    pub(super) fn trace_copy_overwrite_restart_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_copy_overwrite_restart_proof(block, op_idx, output, redef)
        else {
            return;
        };
        self.emit_ready_trace(format!(
            "overwrite-copy-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} consumer_block=0x{:x} consumer_op_seq={} same_value={} redef_dominates_consumer={} old_def_has_pre_redef_use={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops[op_idx].seq_num,
            redef.redef_op_seq,
            proof.redef_rhs,
            proof.consumer_block_addr,
            proof.consumer_op_seq,
            proof.same_value,
            proof.redef_dominates_consumer,
            proof.old_def_has_pre_redef_use,
        ));
    }

    pub(super) fn trace_predicate_overwrite_refresh_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_relation: CrossBlockConsumerRelation,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_predicate_overwrite_refresh_proof(
            block,
            op_idx,
            output,
            redef,
            consumer_relation,
        ) else {
            return;
        };
        self.emit_ready_trace(format!(
            "predicate-overwrite-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} predicate_consumer_block=0x{:x} predicate_consumer_op_seq={} predicate_rhs={} same_guard_family={} old_def_has_pre_redef_use={} redef_dominates_predicate={} consumer_relation={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops[op_idx].seq_num,
            proof.redef_op_seq,
            proof.redef_rhs,
            proof.predicate_consumer_block_addr,
            proof.predicate_consumer_op_seq,
            proof.predicate_rhs,
            proof.same_guard_family,
            proof.old_def_has_pre_redef_use,
            proof.redef_dominates_predicate,
            proof.consumer_relation,
        ));
    }

    pub(super) fn trace_loop_carried_overwrite_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(provenance) = self.describe_loop_carried_overwrite_provenance(
            block,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.loop_carried_value_kind,
                format!("{:?}", provenance.carried_value_kind),
            );
        }
        self.emit_ready_trace(format!(
            "loop-carried-overwrite output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} loop_header=0x{:x} backedge_block=0x{:x} consumer_block=0x{:x} consumer_op_seq={} has_multiequal={} phi_input_count={} induction_like={} carried_value_kind={:?}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            provenance.redef_op_seq,
            provenance.redef_rhs,
            provenance.loop_header,
            provenance.backedge_block,
            provenance.consumer_block,
            provenance.consumer_op_seq,
            provenance.has_multiequal,
            provenance.phi_input_count,
            provenance.induction_like,
            provenance.carried_value_kind,
        ));
        if provenance.carried_value_kind == LoopCarriedValueKind::BooleanFlag {
            self.trace_loop_boolean_flag_proof(
                block,
                op_idx,
                output,
                redef,
                consumer_block_addr,
                consumer_op_seq,
            );
        }
    }

    pub(super) fn trace_loop_boolean_flag_proof(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_loop_boolean_flag_proof(
            block,
            op_idx,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        ) else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.loop_boolean_guard_family,
                format!("{:?}", proof.guard_family),
            );
        }
        let exit_edge = proof
            .exit_edge
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let backedge_edge = proof
            .backedge_edge
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "loop-boolean-flag-proof output=space:{} off:0x{:x} size:{} loop_header=0x{:x} def_block=0x{:x} def_op_seq={} redef_op_seq={} redef_rhs={} consumer_block=0x{:x} consumer_op_seq={} consumer_opcode={:?} exit_edge={} backedge_edge={} guard_family={:?} same_guard_as_exit={} old_def_has_pre_redef_use={} redef_dominates_backedge={} consumer_is_loop_header_predicate={}",
            output.space_id,
            output.offset,
            output.size,
            consumer_block_addr,
            block.start_address,
            block.ops.get(op_idx).map(|op| op.seq_num).unwrap_or_default(),
            redef.redef_op_seq,
            self.format_redefinition_rhs(redef),
            consumer_block_addr,
            consumer_op_seq,
            proof.consumer_opcode,
            exit_edge,
            backedge_edge,
            proof.guard_family,
            proof.same_guard_as_exit,
            proof.old_def_has_pre_redef_use,
            proof.redef_dominates_backedge,
            proof.consumer_is_loop_header_predicate,
        ));
        if proof.same_guard_as_exit && proof.consumer_is_loop_header_predicate {
            self.trace_loop_guard_refresh_dominance(
                block,
                op_idx,
                output,
                redef,
                consumer_block_addr,
                consumer_op_seq,
                &proof,
            );
        }
    }

    pub(super) fn trace_loop_guard_refresh_dominance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        redef: &CrossBlockRedefinitionDetail,
        consumer_block_addr: u64,
        consumer_op_seq: u32,
        boolean_proof: &LoopBooleanFlagProof,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(proof) = self.describe_loop_guard_refresh_dominance_proof(
            block,
            op_idx,
            output,
            redef,
            consumer_block_addr,
            consumer_op_seq,
        ) else {
            return;
        };
        let exit_edge = boolean_proof
            .exit_edge
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let backedge_edge = boolean_proof
            .backedge_edge
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "loop-guard-refresh-dominance loop_header=0x{:x} def_block=0x{:x} redef_block=0x{:x} redef_op_seq={} backedge_block=0x{:x} backedge_edge={} exit_edge={} redef_before_backedge_branch={} all_backedge_paths_covered={} header_predicate_uses_redef={} reason={:?}",
            consumer_block_addr,
            block.start_address,
            proof.redef_block,
            redef.redef_op_seq,
            proof.backedge_block,
            backedge_edge,
            exit_edge,
            proof.redef_before_backedge_branch,
            proof.all_backedge_paths_covered,
            proof.header_predicate_uses_redef,
            proof.reason,
        ));
    }

    pub(super) fn trace_loop_boundary_binding_correlation(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        reason: MaterializationRejectionReason,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(correlation) =
            self.describe_loop_boundary_binding_correlation(block, op_idx, output, reason)
        else {
            return;
        };
        let merge_block = correlation
            .merge_block
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let existing_binding = correlation
            .existing_binding
            .unwrap_or_else(|| "none".to_string());
        self.emit_ready_trace(format!(
            "loop-boundary-binding-correlation output=space:{} off:0x{:x} size:{} loop_header=0x{:x} family={:?} missing_merge_binding={} stable_representative_required={} merge_block={} candidate_binding={} existing_binding={}",
            output.space_id,
            output.offset,
            output.size,
            correlation.loop_header,
            correlation.family,
            correlation.missing_merge_binding,
            correlation.stable_representative_required,
            merge_block,
            correlation.candidate_binding,
            existing_binding,
        ));
    }

    pub(super) fn trace_no_consumer_suppressed(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-suppressed output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?}",
            output.space_id, output.offset, output.size, block_addr, op_seq, rhs,
        ));
    }

    pub(super) fn trace_no_consumer_kept(
        &self,
        block_addr: u64,
        op_seq: u32,
        output: &Varnode,
        rhs: &HirExpr,
        reason: NoConsumerMaterializationKeepReason,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        self.emit_ready_trace(format!(
            "no-consumer-kept output=space:{} off:0x{:x} size:{} def_block=0x{:x} op_seq={} rhs={:?} reason={:?}",
            output.space_id, output.offset, output.size, block_addr, op_seq, rhs, reason,
        ));
    }

    pub(super) fn trace_no_consumer_suppression_detail(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
        applied: bool,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let rhs_kind = Self::classify_no_consumer_suppression_rhs_kind(rhs);
        let output_kind = Self::classify_no_consumer_suppression_output_kind(output);
        let block_position = self.classify_no_consumer_suppression_block_position(block, op_idx);
        self.emit_ready_trace(format!(
            "no-consumer-suppression-detail output=space:{} off:0x{:x} size:{} rhs={:?} rhs_kind={:?} block=0x{:x} op_seq={} block_position={:?} output_kind={:?} applied={} preserve=false unique={}",
            output.space_id,
            output.offset,
            output.size,
            rhs,
            rhs_kind,
            block.start_address,
            block.ops[op_idx].seq_num,
            block_position,
            output_kind,
            applied,
            output.space_id == UNIQUE_SPACE_ID && !output.is_constant,
        ));
    }

    pub(super) fn trace_cross_block_consumer_provenance(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) {
        if !self.emit_ready_trace_enabled_for_current_fn() {
            return;
        }
        let Some(provenance) = self.describe_cross_block_consumer_provenance(block, op_idx, output)
        else {
            return;
        };
        {
            let mut summary = self.materialize_owner_repartition.borrow_mut();
            Self::bump_materialize_owner_histogram(
                &mut summary.cross_block_consumer_relation,
                format!("{:?}", provenance.2.relation),
            );
        }
        let def_successors = self
            .address_to_index
            .get(&block.start_address)
            .and_then(|idx| self.successors.get(*idx))
            .map(|succs| {
                succs
                    .iter()
                    .filter_map(|succ| {
                        self.pcode
                            .blocks
                            .get(*succ)
                            .map(|block| format!("0x{:x}", block.start_address))
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "none".to_string());
        let consumer_block = provenance
            .0
            .map(|addr| format!("0x{addr:x}"))
            .unwrap_or_else(|| "none".to_string());
        let consumer_op_seq = provenance
            .1
            .map(|seq| seq.to_string())
            .unwrap_or_else(|| "none".to_string());
        let consumer_opcode = provenance
            .2
            .consumer_opcode
            .map(|opcode| format!("{opcode:?}"))
            .unwrap_or_else(|| "None".to_string());
        self.emit_ready_trace(format!(
            "cross-block-consumer output=space:{} off:0x{:x} size:{} def_block=0x{:x} consumer_block={} consumer_op_seq={} consumer_opcode={} relation={:?} def_successors=[{}] def_successor_count={} consumer_predecessors={} consumer_is_multiequal={} immediate_successor={} consumer_is_join={} redefined_before_consumer={}",
            output.space_id,
            output.offset,
            output.size,
            block.start_address,
            consumer_block,
            consumer_op_seq,
            consumer_opcode,
            provenance.2.relation,
            def_successors,
            provenance.2.def_successor_count,
            provenance.2.consumer_predecessor_count,
            provenance.2.consumer_is_multiequal,
            provenance.2.immediate_successor,
            provenance.2.consumer_is_join,
            provenance.2.redefined_before_consumer,
        ));
        if let Some(proof) = self.describe_cross_block_replacement_proof(block, op_idx, output, rhs)
        {
            let consumer_block = provenance
                .0
                .map(|addr| format!("0x{addr:x}"))
                .unwrap_or_else(|| "none".to_string());
            let consumer_opcode = proof
                .consumer_opcode
                .map(|opcode| format!("{opcode:?}"))
                .unwrap_or_else(|| "None".to_string());
            self.emit_ready_trace(format!(
                "cross-block-replacement-proof output=space:{} off:0x{:x} size:{} def_block=0x{:x} consumer_block={} relation={:?} def_successor_count={} consumer_predecessor_count={} dominates_consumer={} consumer_opcode={} rhs_low_cost={} preserve_materialization={} no_redefinition_before_consumer={} merge_phi={} narrow_candidate={}",
                output.space_id,
                output.offset,
                output.size,
                block.start_address,
                consumer_block,
                proof.relation,
                proof.def_successor_count,
                proof.consumer_predecessor_count,
                proof.dominates_consumer,
                consumer_opcode,
                proof.rhs_low_cost,
                proof.preserve_materialization,
                proof.no_redefinition_before_consumer,
                proof.merge_phi,
                proof.narrow_candidate,
            ));
            if let Some(redef) =
                self.describe_cross_block_redefinition_detail(block, op_idx, output, provenance.0)
            {
                {
                    let mut summary = self.materialize_owner_repartition.borrow_mut();
                    Self::bump_materialize_owner_histogram(
                        &mut summary.cross_block_redefinition_relation,
                        format!("{:?}", redef.relation),
                    );
                    Self::bump_materialize_owner_histogram(
                        &mut summary.same_block_overwrite_shape_kind,
                        format!("{:?}", redef.overwrite_shape),
                    );
                }
                self.emit_ready_trace(format!(
                    "cross-block-redefinition output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} consumer_block={} relation={:?} redef_block=0x{:x} redef_op_seq={} redef_opcode={:?} redef_rhs_kind={:?} overwrite_shape={:?} redef_relation={:?} consumer_op_seq={} terminator_idx={} def_to_redef_gap={} redef_to_terminator_gap={}",
                    output.space_id,
                    output.offset,
                    output.size,
                    block.start_address,
                    block.ops[op_idx].seq_num,
                    consumer_block,
                    proof.relation,
                    redef.redef_block_addr,
                    redef.redef_op_seq,
                    redef.redef_opcode,
                    redef.redef_rhs_kind,
                    redef.overwrite_shape,
                    redef.relation,
                    consumer_op_seq,
                    self.block_terminator_index(block)
                        .map(|idx| idx.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    redef.def_to_redef_gap,
                    redef
                        .redef_to_terminator_gap
                        .map(|gap| gap.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                ));
                if redef.overwrite_shape == SameBlockOverwriteShapeKind::OverwriteAtCopy {
                    self.trace_copy_overwrite_restart_proof(block, op_idx, output, &redef);
                } else if redef.overwrite_shape
                    == SameBlockOverwriteShapeKind::OverwriteAtPredicateProducer
                {
                    self.trace_predicate_overwrite_refresh_proof(
                        block,
                        op_idx,
                        output,
                        &redef,
                        proof.relation,
                    );
                } else if redef.overwrite_shape
                    == SameBlockOverwriteShapeKind::OverwriteAtLoopUpdate
                {
                    if let (Some(consumer_block_addr), Some(consumer_op_seq), _) = provenance {
                        self.trace_loop_carried_overwrite_provenance(
                            block,
                            op_idx,
                            output,
                            &redef,
                            consumer_block_addr,
                            consumer_op_seq,
                        );
                    }
                }
            }
        }
    }
}
