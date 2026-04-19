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
                "unknown_consumer_kind_reason",
                Self::format_materialize_owner_histogram(&summary.unknown_consumer_kind_reason),
            ),
            (
                "unknown_consumer_kind_opcode",
                Self::format_materialize_owner_histogram(&summary.unknown_consumer_kind_opcode),
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
        if proof.reason == DisallowedSingleConsumerReason::ConsumerIsPredicate {
            self.trace_single_consumer_predicate_proof(block, op_idx, output, rhs);
        } else if proof.reason == DisallowedSingleConsumerReason::UnknownConsumerKind {
            self.trace_unknown_consumer_kind(block, op_idx, output, rhs);
        }
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
