use super::*;

impl<'a> PreviewBuilder<'a> {
    /// Reuse a prior materialization of the same register in this block.
    ///
    /// Critical for x86 cmov: the default `eax = hi` and the guarded
    /// `eax = value` / `eax = lo` overrides must share one C variable so
    /// `return eax` sees the composed result.
    pub(super) fn same_block_prior_register_binding_name(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        let proof = self.prove_same_block_register_join(block, op_idx, output)?;
        debug_assert!(proof.prior_op_idx < op_idx);
        Some(proof.binding_name)
    }

    /// Reuse an entry-owned register parameter as the carrier for a guarded
    /// same-block cmov write when the register has no earlier definition in the
    /// block.
    ///
    /// A same-block-forward CBranch is not represented as a CFG join, so the
    /// ordinary same-block register-join proof can only see prior *writes*.
    /// For a conditional write to an incoming register, the prior value is the
    /// ABI entry value instead. Keeping the parameter name makes both arms of
    /// the conditional write the same sequential C carrier, and later reads of
    /// the register therefore observe the selected value.
    pub(super) fn same_block_cmov_entry_register_binding_name(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        self.same_block_cmov_entry_register_binding_name_at(
            self.lowering_block_index(block),
            op_idx,
            output,
        )
    }

    /// The def-site form of [`Self::same_block_cmov_entry_register_binding_name`].
    ///
    /// Cross-block lowering can reach a definition before its block is being
    /// materialized, so callers there have a block index rather than a borrowed
    /// block. Keeping the proof here makes both paths use the same cmov and
    /// entry-register conditions.
    pub(in crate::midend::builder) fn same_block_cmov_entry_register_binding_name_at(
        &mut self,
        block_idx: usize,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || !self
                .pcode
                .blocks
                .get(block_idx)
                .is_some_and(|block| self.op_is_inside_same_block_forward_cmov_body(block, op_idx))
        {
            return None;
        }
        // If this register was already written in the block, its current value
        // is not the entry parameter. A prior same-block materialization should
        // have claimed that value; do not alias an unrelated rewrite to the
        // formal parameter as a fallback.
        if self.pcode.blocks[block_idx].ops[..op_idx]
            .iter()
            .any(|prior_op| {
                prior_op
                    .output
                    .as_ref()
                    .is_some_and(|prior_output| self.varnode_aliases_value(prior_output, output))
            })
        {
            return None;
        }
        self.register_param(output)
    }

    pub(super) fn primary_return_name_from_live_out_proof(
        &self,
        output: &Varnode,
        definition_block_idx: usize,
        definition_op_idx: usize,
        proof: DefinitionReachesReturnProof,
    ) -> Option<String> {
        if !self.register_namer().is_primary_return_register(output)
            || proof.definition_site() != (definition_block_idx, definition_op_idx)
        {
            return None;
        }
        // Size-exact HW name (eax for size-4). On x64, full-width RAX writes use
        // `full_width_primary_return_surface_name` so cmov can family-join onto
        // `rax`; forcing every live-out EAX onto `rax` collapses
        // `mov eax, imm; zext rax` into identity `rax = rax` and blocks
        // `return 7` narrowing.
        self.sla_hw_name(output.offset, output.size)
    }

    /// Freeze a primary-return low half into the full register when the full
    /// surface is required by a later same-block cmov body or by a
    /// cross-block use after an observed call carrier was overwritten.
    ///
    /// Shape: `IntZExt`/`IntSExt` writing pointer-size primary return from a
    /// narrower same-offset input (`IntZExt rax ← eax`), and a later op inside
    /// a same-block-forward CBranch skip that writes the primary return family
    /// (cmovl into EAX). That freeze must bind as `rax` so the cmov body
    /// family-joins onto the surface used by epilogue `return rax`.
    ///
    /// The latter case is deliberately limited to an observed call carrier and
    /// a non-local use. A local extension such as the RC4 index's freeze then
    /// `movzx al` keeps its temporary/low-byte identity.
    pub(super) fn full_width_primary_return_surface_name(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> Option<String> {
        if !self.options.is_64bit || self.options.pointer_size < 8 {
            return None;
        }
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || output.size != self.options.pointer_size
            || !self.register_namer().is_primary_return_register(output)
        {
            return None;
        }
        if !matches!(op.opcode, PcodeOpcode::IntZExt | PcodeOpcode::IntSExt) {
            return None;
        }
        let input = op.inputs.first()?;
        if input.is_constant
            || !is_register_space_id(input.space_id)
            || input.offset != output.offset
            || input.size >= output.size
            || !self.register_namer().is_primary_return_register(input)
        {
            return None;
        }
        if self
            .register_namer()
            .register_name_with_param_owned(output.offset, output.size)
            .is_some_and(|(_, idx)| idx.is_some())
        {
            return None;
        }
        if !self.later_same_block_cmov_writes_primary_return_family(block, op_idx, output)
            && !self.full_width_return_extension_overwrites_observed_call(block, op_idx, output)
        {
            return None;
        }
        self.sla_hw_name(output.offset, self.options.pointer_size)
            .or_else(|| self.sla_hw_name(output.offset, output.size))
    }

    /// A call's result is represented by the primary-return surface until a
    /// later definition proves otherwise. A narrower write followed by a
    /// widening extension is such a definition, but reusing the narrow
    /// materialization name does not update the full-width call carrier. If
    /// that extended value crosses a block boundary, preserve the full-width
    /// ABI surface so successor reads cannot recover the stale call result.
    fn full_width_return_extension_overwrites_observed_call(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if !self.output_has_nonlocal_use(block, op_idx, output) {
            return false;
        }
        let block_idx = self.lowering_block_index(block);
        let mut saw_partial_redefinition = false;
        for prior_idx in (0..op_idx).rev() {
            let prior_op = &block.ops[prior_idx];
            if let Some(prior_output) = prior_op.output.as_ref()
                && self.varnode_aliases_value(prior_output, output)
            {
                if prior_output.size >= output.size {
                    return false;
                }
                saw_partial_redefinition = true;
            }
            if saw_partial_redefinition
                && matches!(prior_op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                && self.call_result_bindings.contains_key(&LoweringSite {
                    block_idx,
                    op_idx: prior_idx,
                })
            {
                return true;
            }
        }
        false
    }

    /// True when some later op in this block is inside a same-block-forward
    /// cmov body and writes the primary-return register family of `output`.
    fn later_same_block_cmov_writes_primary_return_family(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        // Built once. `register_namer()` reconstructs the whole register
        // model on every call -- two fresh hash maps of every register in the
        // architecture -- and this loop used to ask for a new one per op. A
        // `sample` profile of `bzip2`'s `sendMTFValues` (112 blocks) put
        // essentially all of that function's 55 seconds inside this method.
        let namer = self.register_namer();
        if !namer.is_primary_return_register(output) {
            return false;
        }
        for later_idx in (op_idx + 1)..block.ops.len() {
            if !self.op_is_inside_same_block_forward_cmov_body(block, later_idx) {
                continue;
            }
            let Some(later_out) = block.ops[later_idx].output.as_ref() else {
                continue;
            };
            if later_out.offset == output.offset && namer.is_primary_return_register(later_out) {
                return true;
            }
        }
        false
    }

    fn prove_same_block_register_join(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<SameBlockRegisterJoinProof> {
        if output.is_constant || !is_register_space_id(output.space_id) {
            return None;
        }
        let current_definition_reads_prior_value = block.ops[op_idx]
            .inputs
            .iter()
            .any(|input| self.varnode_aliases_value(input, output));
        // A plain same-block register rewrite is not a value join. In
        // particular, call argument registers are consumed by the ABI even
        // though the P-code CALL itself does not list those registers as
        // inputs. Reusing the prior binding across such a rewrite aliases
        // independent call carriers. The join proof is still valid for an
        // explicit self-update and for a write inside the guarded body of a
        // same-block cmov, where the two definitions represent one logical
        // value.
        if !current_definition_reads_prior_value
            && !self.op_is_inside_same_block_forward_cmov_body(block, op_idx)
        {
            return None;
        }
        if Self::output_has_consumed_interval_before_redefinition(block, op_idx, output)
            && !current_definition_reads_prior_value
        {
            return None;
        }
        let block_idx = self.lowering_block_index(block);
        let output_key = VarnodeKey::from(output);
        for prior_idx in (0..op_idx).rev() {
            let prior_op = &block.ops[prior_idx];
            let Some(prior_out) = prior_op.output.as_ref() else {
                continue;
            };
            let prior_key = VarnodeKey::from(prior_out);
            if prior_key != output_key
                && !Self::varnode_key_may_alias_output(&prior_key, &output_key)
                && !self.varnode_aliases_value(prior_out, output)
            {
                continue;
            }
            // This heuristic exists for cmov-style same-block register
            // redefinition chains (a default value, then a conditional
            // override -- both representing one logical value). Without a
            // Cover check it also fires when a register is simply reused as
            // an unrelated scratch value later in the same block (e.g.
            // three sequential, independent lookup-table reads that each
            // pass through EAX for a different purpose). Ghidra's Cover
            // system would never merge those into one HighVariable; skip
            // this candidate the same way when the already-computed SSA
            // Cover positively proves the two definitions' live ranges
            // interfere, and keep searching further back for one that
            // doesn't.
            if self.cover_proves_distinct_and_interfering(
                block_idx, op_idx, output, prior_idx, prior_out,
            ) {
                continue;
            }
            let name = self
                .materialized_vns
                .get(&MaterializedVarnodeKey::new(prior_out, prior_op))
                .cloned()?;
            return Some(SameBlockRegisterJoinProof {
                binding_name: name,
                prior_op_idx: prior_idx,
            });
        }
        None
    }

    pub(super) fn output_has_consumed_interval_before_redefinition(
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        !Self::collect_output_use_sites_in_block(block, op_idx, output).is_empty()
            && Self::first_output_redefinition_in_block(block, op_idx, output).is_some()
    }

    pub(super) fn live_register_lhs_name_for_passthrough_join_store_producer(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> Option<(String, u32)> {
        if output.is_constant
            || !is_unique_space_id(output.space_id)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
        {
            return None;
        }
        for (consumer_idx, consumer_op) in self.output_use_sites_in_block(block, op_idx, output) {
            if !matches!(
                consumer_op.opcode,
                PcodeOpcode::Copy | PcodeOpcode::IntZExt | PcodeOpcode::Cast
            ) {
                continue;
            }
            if !consumer_op
                .inputs
                .iter()
                .any(|input| self.varnode_aliases_value(input, output))
            {
                continue;
            }
            let Some(consumer_output) = consumer_op.output.as_ref() else {
                continue;
            };
            let consumer_rhs = PreHirExpr::Var("producer".to_string());
            let Some((name, binding_size)) = self.live_register_lhs_name_for_safe_missing_merge(
                block,
                consumer_idx,
                consumer_op,
                consumer_output,
                &consumer_rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ) else {
                continue;
            };
            return Some((name, binding_size.min(output.size)));
        }
        None
    }

    pub(super) fn live_register_lhs_name_for_safe_missing_merge(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
        rhs: &PreHirExpr,
        replacement_plan: ReplacementValuePlan,
    ) -> Option<(String, u32)> {
        if replacement_plan.rejection_reason()
            != Some(MaterializationRejectionReason::MissingMergeBinding)
            || output.is_constant
            || !is_register_space_id(output.space_id)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
        {
            if is_register_space_id(output.space_id) {}
            return None;
        }
        let proof = self.describe_missing_merge_binding_proof(block, op_idx, output, rhs)?;
        let live_register_join = proof.relation
            == MissingMergeBindingRelation::PredicateMergeMissing
            || (proof.consumer_kind == DisallowedSingleConsumerConsumerKind::StoreValue
                && proof.relation == MissingMergeBindingRelation::JoinMergeMissing);
        let live_register_loop_carried = proof.relation
            == MissingMergeBindingRelation::LoopHeaderMergeMissing
            && matches!(
                proof.consumer_kind,
                DisallowedSingleConsumerConsumerKind::OtherData
                    | DisallowedSingleConsumerConsumerKind::Predicate
                    | DisallowedSingleConsumerConsumerKind::StoreValue
            );
        if !live_register_join && !live_register_loop_carried {
            return None;
        }
        let definition_block_idx = self.lowering_block_index(block);
        let merge_block_idx = self.address_to_index.get(&proof.merge_block).copied()?;
        let reach_proof = self.prove_definition_reaches_block_entry(
            definition_block_idx,
            op_idx,
            output,
            merge_block_idx,
        )?;
        debug_assert_eq!(
            reach_proof.definition_site(),
            (definition_block_idx, op_idx)
        );
        debug_assert_eq!(reach_proof.target_block(), merge_block_idx);
        let output_key = VarnodeKey::from(output);
        if !live_register_loop_carried {
            self.gpr_family_index_for_key(&output_key)?;
        }
        if self.options.calling_convention == CallingConvention::AArch64
            && output.size == 8
            && matches!(op.opcode, PcodeOpcode::IntZExt | PcodeOpcode::Cast)
            && op.inputs.first().is_some_and(|input| input.size <= 4)
        {
            return self.sla_hw_name(output.offset, 4).map(|name| (name, 4));
        }
        if live_register_loop_carried {
            // A missing loop-header incoming value can be the ABI-owned entry
            // state rather than an unowned scratch register. When entry-use
            // inference has already proved this slot is a formal parameter,
            // keep the carrier on that formal so the first loop iteration is
            // initialized from the caller's value. Unproven slots continue to
            // use their hardware identity below; prior-definition cases are
            // likewise handled by the existing loop-carrier proof before this
            // missing-merge fallback.
            if let Some(param_index) = self.abi_state().param_slot_for_varnode(output)
                && param_index < self.entry_arity
            {
                let name = self.abi_state().param_name(param_index);
                self.trace_path_sensitive_register_merge(
                    block.start_address,
                    op.seq_num,
                    output,
                    proof.relation,
                    proof.consumer_kind,
                    name.as_str(),
                );
                return Some((name, output.size));
            }
            let name = self.sla_hw_name(output.offset, output.size)?;
            if crate::arch::x86::x86_gpr_family_index(name.as_str()).is_none()
                && self.gpr_family_index_for_key(&output_key).is_none()
            {
                return None;
            }
            if self.cover_proves_block_entry_reuse_unsafe(
                definition_block_idx,
                op_idx,
                output,
                merge_block_idx,
            ) {
                return None;
            }
            self.trace_path_sensitive_register_merge(
                block.start_address,
                op.seq_num,
                output,
                proof.relation,
                proof.consumer_kind,
                name.as_str(),
            );
            return Some((name, output.size));
        }
        None
    }

    pub(super) fn rhs_is_safe_scalar_live_register_merge(expr: &PreHirExpr) -> bool {
        match expr {
            PreHirExpr::Var(_)
            | PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(..) => true,
            PreHirExpr::Cast { ty, expr } | PreHirExpr::Unary { ty, expr, .. } => {
                Self::type_is_scalar_live_register_merge(ty)
                    && Self::rhs_is_safe_scalar_live_register_merge(expr)
            }
            PreHirExpr::Binary { ty, lhs, rhs, .. } => {
                Self::type_is_scalar_live_register_merge(ty)
                    && Self::rhs_is_safe_scalar_live_register_merge(lhs)
                    && Self::rhs_is_safe_scalar_live_register_merge(rhs)
            }
            PreHirExpr::Call { .. }
            | PreHirExpr::Load { .. }
            | PreHirExpr::PtrOffset { .. }
            | PreHirExpr::Index { .. }
            | PreHirExpr::AggregateCopy { .. }
            | PreHirExpr::FieldAccess { .. }
            | PreHirExpr::Select { .. } => false,
        }
    }

    fn type_is_scalar_live_register_merge(ty: &NirType) -> bool {
        matches!(ty, NirType::Bool | NirType::Int { .. })
    }
}
