//! Replacement-plan construction for materialized RHS candidates.
//!
//! This module owns the proof-driven decision to inline a lowered RHS or keep
//! a stable materialization binding.  It consumes the surrounding builder's
//! def-use, CFG, telemetry, and trace contracts without owning those stores.

use super::contracts::*;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn try_lower_materialized_output_rhs(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
    ) -> Result<Option<PreHirExpr>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let active_key = MaterializedVarnodeKey::new(output, op);
        if !self.active_materialized_rhs_keys.insert(active_key.clone()) {
            return Ok(None);
        }

        let result = (|| {
            let rhs = match self.lower_def_op(op, &mut HashSet::default()) {
                Ok(rhs) => rhs,
                Err(err)
                    if matches!(
                        err,
                        MlilPreviewError::LoweringFailed
                            | MlilPreviewError::UnsupportedExprVarnodeLowering
                            | MlilPreviewError::UnsupportedExprAddressMaterialization
                            | MlilPreviewError::UnsupportedExprIndirectValueSource
                            | MlilPreviewError::UnsupportedExprPieceShape
                            | MlilPreviewError::UnsupportedExprPtrArithmetic
                            | MlilPreviewError::UnsupportedExprMemoryBackedVarnode
                            | MlilPreviewError::UnsupportedExprMultiequal
                    ) =>
                {
                    self.debug_lowering_error(
                        "materialize_output_skip",
                        block_addr,
                        u64::from(op.seq_num),
                        op.opcode,
                        &err,
                    );
                    return Ok(None);
                }
                Err(err) => {
                    self.debug_lowering_error(
                        "materialize_output",
                        block_addr,
                        u64::from(op.seq_num),
                        op.opcode,
                        &err,
                    );
                    return Err(err);
                }
            };
            let rhs = self.rewrite_block_entry_accumulator_rhs_with_live_gpr(block_addr, op, rhs);
            Ok(Some(rhs))
        })();

        let removed = self.active_materialized_rhs_keys.remove(&active_key);
        debug_assert!(removed, "active materialized RHS key must remain balanced");
        result
    }

    pub(super) fn output_replacement_is_complete(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> bool {
        // ABI primary return storage is live-out past RET even when no same-block
        // p-code op reads it (Return varnode is the return *address*).
        if self.register_namer().is_primary_return_register(output) {
            return false;
        }
        // Guarded cmov body (strictly between same-block-forward CBranch and its
        // skip target) is conditional. Complete replacement would make later
        // uses always see the taken-path RHS (x64 clamp: cmovle into R8 then
        // cmovge from R8 collapsed to max(lo, value)).
        if self.op_is_inside_same_block_forward_cmov_body(block, op_idx) {
            return false;
        }
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1
            && Self::expr_is_low_cost_builder_inline_candidate(rhs)
            && if Self::expr_requires_passthrough_single_use_inline(rhs) {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(uses[0].1.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(uses[0].1.opcode)
            }
    }

    /// True when `op_idx` is strictly inside a same-block-forward CBranch skip
    /// range (the guarded cmov / instruction-local body).
    /// The `(branch_idx, target_idx)` spans of this block's same-block forward
    /// branches, computed once per block.
    ///
    /// Resolving them is a scan of the block, and the question "is this op
    /// inside one of them" was asked from inside a loop over the block's
    /// suffix, which was itself run once per op -- so the block was walked
    /// cubically to answer a question whose answer never changes. Measured on
    /// `bzip2`'s `sendMTFValues` (112 blocks): a `sample` profile put
    /// essentially all of that function's 55 seconds here.
    pub(super) fn same_block_forward_cmov_spans(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> std::rc::Rc<Vec<(usize, usize)>> {
        let key = self.lowering_block_index(block);
        if let Some(cached) = self.cmov_body_spans.borrow().get(&key).cloned() {
            return cached;
        }
        let mut spans = Vec::new();
        for (branch_idx, op) in block.ops.iter().enumerate() {
            if op.opcode != PcodeOpcode::CBranch || op.inputs.len() < 2 {
                continue;
            }
            if let Some(target) = crate::midend::cfg::same_block_forward_branch_target_op_idx(
                block,
                branch_idx,
                block.ops.len(),
                op,
                &op.inputs[0],
            ) {
                spans.push((branch_idx, target));
            }
        }
        let spans = std::rc::Rc::new(spans);
        self.cmov_body_spans
            .borrow_mut()
            .insert(key, std::rc::Rc::clone(&spans));
        spans
    }

    pub(super) fn op_is_inside_same_block_forward_cmov_body(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        if op_idx == 0 || op_idx >= block.ops.len() {
            return false;
        }
        self.same_block_forward_cmov_spans(block)
            .iter()
            .any(|&(branch_idx, target)| branch_idx < op_idx && op_idx < target)
    }

    pub(super) fn build_replacement_value_plan(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> ReplacementValuePlan {
        self.telemetry
            .materialization
            .replacement_plan_candidate_count += 1;
        // Guarded cmov body must keep a materialization binding; complete plans
        // would unconditionalize the taken-path value for later uses.
        if self.op_is_inside_same_block_forward_cmov_body(block, op_idx) {
            return ReplacementValuePlan::incomplete(
                ReplacementReadClass::SameBlockData,
                MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
            );
        }
        let legacy_inline_candidate =
            self.output_replacement_is_complete(block, op_idx, output, rhs);
        if Self::parity_chain_materialization_enabled()
            && let Some(result) = self.describe_parity_chain_proof(block, op_idx, output, rhs)
        {
            match result {
                Ok(proof) => {
                    let fallback_plan = self.preview_replacement_value_plan_without_parity(
                        block,
                        op_idx,
                        terminator_index,
                        output,
                        rhs,
                    );
                    self.trace_parity_chain_regression_attribution(
                        block,
                        op_idx,
                        output,
                        rhs,
                        &proof,
                        legacy_inline_candidate,
                        fallback_plan,
                    );
                    self.trace_parity_chain_materialized(block, op_idx, output, &proof);
                    self.telemetry
                        .materialization
                        .replacement_plan_completed_count += 1;
                    return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
                }
                Err(reason) => {
                    self.trace_parity_chain_kept(block, op_idx, output, reason);
                }
            }
        }
        self.build_replacement_value_plan_without_parity(
            block,
            op_idx,
            terminator_index,
            output,
            rhs,
        )
    }

    fn preview_replacement_value_plan_without_parity(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> ReplacementValuePlan {
        if self.output_has_nonlocal_use(block, op_idx, output) {
            let rejection_reason =
                self.classify_nonlocal_materialization_rejection_reason(block, op_idx, output, rhs);
            self.trace_missing_merge_binding_proof(block, op_idx, output, rhs);
            return ReplacementValuePlan::incomplete(ReplacementReadClass::Merge, rejection_reason);
        }
        if let Some(read_class) =
            self.classify_terminator_sensitive_output_use(block, op_idx, terminator_index, output)
        {
            if Self::replacement_read_requires_stable_representative(read_class, rhs) {
                self.trace_stable_representative_owner_proof(
                    block,
                    op_idx,
                    terminator_index,
                    output,
                    rhs,
                );
                return ReplacementValuePlan::incomplete(
                    read_class,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            return ReplacementValuePlan::complete(read_class);
        }
        if self.output_replacement_is_complete(block, op_idx, output, rhs) {
            if Self::same_block_replacement_requires_stable_representative(rhs) {
                if Self::stack_addr_frame_stable_replacement_enabled() {
                    match self.describe_stack_addr_frame_stable_trial(
                        block,
                        op_idx,
                        terminator_index,
                        output,
                        rhs,
                    ) {
                        Ok(proof) => {
                            self.trace_stack_address_frame_stable_trial(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                                Some(&proof),
                                true,
                                false,
                                StackAddrFrameStableTrialReason::StackAddrFrameStableReplaced,
                            );
                            return ReplacementValuePlan::complete(
                                ReplacementReadClass::SameBlockData,
                            );
                        }
                        Err(reason) => {
                            let proof = self.describe_stack_address_stability_proof(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                            );
                            self.trace_stack_address_frame_stable_trial(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                                proof.as_ref(),
                                false,
                                true,
                                reason,
                            );
                        }
                    }
                }
                self.trace_stable_representative_owner_proof(
                    block,
                    op_idx,
                    terminator_index,
                    output,
                    rhs,
                );
                return ReplacementValuePlan::incomplete(
                    ReplacementReadClass::SameBlockData,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
        }
        ReplacementValuePlan::incomplete(
            ReplacementReadClass::SameBlockData,
            MaterializationRejectionReason::AliasUnsafe,
        )
    }

    fn build_replacement_value_plan_without_parity(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> ReplacementValuePlan {
        if Self::copy_overwrite_restart_enabled() {
            if let Some(proof) = self.can_restart_def_window_at_copy_overwrite(
                block,
                op_idx,
                terminator_index,
                output,
            ) {
                self.emit_ready_trace(format!(
                        "def-window-restarted-at-copy-overwrite output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} consumer_block=0x{:x} consumer_op_seq={} relation={:?} redef_rhs={} same_value={} redef_dominates_consumer={} old_def_has_pre_redef_use={}",
                        output.space_id,
                        output.offset,
                        output.size,
                        block.start_address,
                        block.ops[op_idx].seq_num,
                        proof.redef_op_seq,
                        proof.consumer_block_addr,
                        proof.consumer_op_seq,
                        proof.consumer_relation,
                        proof.redef_rhs,
                        proof.same_value,
                        proof.redef_dominates_consumer,
                        proof.old_def_has_pre_redef_use,
                    ));
                self.telemetry
                    .materialization
                    .replacement_plan_completed_count += 1;
                return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
            }
        }
        if Self::predicate_refresh_restart_enabled() {
            if let Some(proof) = self.can_restart_def_window_at_predicate_refresh(
                block,
                op_idx,
                terminator_index,
                output,
            ) {
                self.emit_ready_trace(format!(
                        "def-window-restarted-at-predicate-refresh output=space:{} off:0x{:x} size:{} def_block=0x{:x} def_op_seq={} redef_op_seq={} predicate_consumer_block=0x{:x} predicate_consumer_op_seq={} relation={:?} redef_rhs={} predicate_rhs={} same_guard_family={} old_def_has_pre_redef_use={} redef_dominates_predicate={}",
                        output.space_id,
                        output.offset,
                        output.size,
                        block.start_address,
                        block.ops[op_idx].seq_num,
                        proof.redef_op_seq,
                        proof.predicate_consumer_block_addr,
                        proof.predicate_consumer_op_seq,
                        proof.consumer_relation,
                        proof.redef_rhs,
                        proof.predicate_rhs,
                        proof.same_guard_family,
                        proof.old_def_has_pre_redef_use,
                        proof.redef_dominates_predicate,
                    ));
                self.telemetry
                    .materialization
                    .replacement_plan_completed_count += 1;
                return ReplacementValuePlan::complete(ReplacementReadClass::PredicateSensitive);
            }
        }
        if self.output_has_nonlocal_use(block, op_idx, output) {
            let rejection_reason =
                self.classify_nonlocal_materialization_rejection_reason(block, op_idx, output, rhs);
            let duplicate_start_merge_candidate = || {
                self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)
                    .is_some_and(|proof| {
                        self.duplicate_start_merge_block(proof.merge_block)
                            && proof.can_synthesize_phi_like_binding
                            && proof.predecessor_count == 2
                            && proof.missing_incoming_count == 0
                            && proof.conflicting_incoming_count == 1
                            && proof.consumer_kind
                                == DisallowedSingleConsumerConsumerKind::OtherData
                    })
            };
            if rejection_reason == MaterializationRejectionReason::MissingMergeBinding
                && (Self::explicit_merge_binding_enabled() || duplicate_start_merge_candidate())
            {
                match self.describe_explicit_merge_binding_trial(block, op_idx, output, rhs) {
                    Ok(proof) => {
                        self.trace_explicit_merge_binding_trial(
                            proof.merge_block,
                            output,
                            &[],
                            &[],
                            &proof.incoming_value_kinds,
                            proof.rhs_kind,
                            "pending",
                            false,
                            ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
                        );
                        self.telemetry
                            .materialization
                            .replacement_plan_completed_count += 1;
                        return ReplacementValuePlan::complete(ReplacementReadClass::Merge);
                    }
                    Err(reason) => {
                        self.trace_explicit_merge_binding_trial(
                            block.start_address,
                            output,
                            &[],
                            &[],
                            &[],
                            Self::classify_disallowed_single_consumer_rhs_kind(rhs),
                            "none",
                            false,
                            reason,
                        );
                    }
                }
            }
            self.record_materialize_rejection_reason(rejection_reason);
            self.trace_missing_merge_binding_proof(block, op_idx, output, rhs);
            self.trace_loop_boundary_binding_correlation(block, op_idx, output, rejection_reason);
            match rejection_reason {
                MaterializationRejectionReason::MissingMergeBinding => {
                    self.telemetry
                        .materialization
                        .replacement_plan_rejected_missing_merge_count += 1;
                }
                MaterializationRejectionReason::RepresentativeRootAttribution => {
                    self.telemetry
                        .materialization
                        .replacement_plan_rejected_representative_root_attribution_count += 1;
                }
                MaterializationRejectionReason::TempOnlyRepresentativeLifecycle => {
                    self.telemetry
                        .materialization
                        .replacement_plan_rejected_temp_only_representative_lifecycle_count += 1;
                }
                MaterializationRejectionReason::DeadTempRepresentative => {
                    self.telemetry
                        .materialization
                        .replacement_plan_rejected_dead_temp_representative_count += 1;
                }
                MaterializationRejectionReason::AliasUnsafe
                | MaterializationRejectionReason::ConsumerRequiresStableRepresentative => {}
            }
            return ReplacementValuePlan::incomplete(ReplacementReadClass::Merge, rejection_reason);
        }
        if let Some(read_class) =
            self.classify_terminator_sensitive_output_use(block, op_idx, terminator_index, output)
        {
            if Self::replacement_read_requires_stable_representative(read_class, rhs) {
                self.trace_stable_representative_owner_proof(
                    block,
                    op_idx,
                    terminator_index,
                    output,
                    rhs,
                );
                self.record_materialize_rejection_reason(
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
                self.trace_loop_boundary_binding_correlation(
                    block,
                    op_idx,
                    output,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
                self.telemetry
                    .materialization
                    .replacement_plan_rejected_alias_unsafe_count += 1;
                return ReplacementValuePlan::incomplete(
                    read_class,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            self.telemetry
                .materialization
                .replacement_plan_completed_count += 1;
            return ReplacementValuePlan::complete(read_class);
        }
        if self.output_replacement_is_complete(block, op_idx, output, rhs) {
            if Self::same_block_replacement_requires_stable_representative(rhs) {
                if Self::stack_addr_frame_stable_replacement_enabled() {
                    match self.describe_stack_addr_frame_stable_trial(
                        block,
                        op_idx,
                        terminator_index,
                        output,
                        rhs,
                    ) {
                        Ok(proof) => {
                            self.trace_stack_address_frame_stable_trial(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                                Some(&proof),
                                true,
                                false,
                                StackAddrFrameStableTrialReason::StackAddrFrameStableReplaced,
                            );
                            self.telemetry
                                .materialization
                                .replacement_plan_completed_count += 1;
                            return ReplacementValuePlan::complete(
                                ReplacementReadClass::SameBlockData,
                            );
                        }
                        Err(reason) => {
                            let proof = self.describe_stack_address_stability_proof(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                            );
                            self.trace_stack_address_frame_stable_trial(
                                block,
                                op_idx,
                                terminator_index,
                                output,
                                rhs,
                                proof.as_ref(),
                                false,
                                true,
                                reason,
                            );
                        }
                    }
                }
                self.trace_stable_representative_owner_proof(
                    block,
                    op_idx,
                    terminator_index,
                    output,
                    rhs,
                );
                self.record_materialize_rejection_reason(
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
                self.trace_loop_boundary_binding_correlation(
                    block,
                    op_idx,
                    output,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
                self.telemetry
                    .materialization
                    .replacement_plan_rejected_alias_unsafe_count += 1;
                return ReplacementValuePlan::incomplete(
                    ReplacementReadClass::SameBlockData,
                    MaterializationRejectionReason::ConsumerRequiresStableRepresentative,
                );
            }
            self.telemetry
                .materialization
                .replacement_plan_completed_count += 1;
            return ReplacementValuePlan::complete(ReplacementReadClass::SameBlockData);
        }
        self.telemetry
            .materialization
            .replacement_plan_rejected_alias_unsafe_count += 1;
        self.record_materialize_rejection_reason(MaterializationRejectionReason::AliasUnsafe);
        let hazard =
            Self::classify_alias_unsafe_hazard(block, op_idx, terminator_index, output, rhs);
        self.trace_alias_unsafe_hazard(
            block.start_address,
            block.ops[op_idx].seq_num,
            output,
            rhs,
            hazard,
        );
        ReplacementValuePlan::incomplete(
            ReplacementReadClass::SameBlockData,
            MaterializationRejectionReason::AliasUnsafe,
        )
    }
}
