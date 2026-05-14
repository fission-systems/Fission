use super::*;

mod contracts;
mod cross_block;
mod loop_carried;
mod no_consumer;
mod same_block;
mod scans;
#[cfg(test)]
pub(super) mod test_support;
mod trace;

pub(in crate::nir::builder) use self::contracts::MaterializeOwnerRepartition;
use self::contracts::*;

impl<'a> PreviewBuilder<'a> {
    fn is_callee_saved_push_store(&self, op: &PcodeOp) -> bool {
        let Some(asm) = op.asm_mnemonic.as_deref() else {
            return false;
        };
        let asm = asm.trim().to_ascii_uppercase();
        asm.starts_with("PUSH RSI")
            || asm.starts_with("PUSH RDI")
            || asm.starts_with("PUSH RBX")
            || asm.starts_with("PUSH RBP")
            || asm.starts_with("PUSH R12")
            || asm.starts_with("PUSH R13")
            || asm.starts_with("PUSH R14")
            || asm.starts_with("PUSH R15")
    }

    fn is_call_return_scaffold_store(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
    ) -> bool {
        if op.inputs.len() < 3 || !op.inputs[2].is_constant {
            return false;
        }
        let Some((next_idx, next_call)) =
            block
                .ops
                .iter()
                .enumerate()
                .skip(op_idx + 1)
                .find(|(_, candidate)| {
                    matches!(
                        candidate.opcode,
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                    )
                })
        else {
            return false;
        };
        if next_idx != op_idx + 1 {
            return false;
        }
        let ret_addr = op.inputs[2].constant_val as u64;
        ret_addr > next_call.address && ret_addr.saturating_sub(next_call.address) <= 0x10
    }

    fn call_result_registers(&self) -> Vec<Varnode> {
        if !self.options.is_64bit
            && !matches!(
                self.options.calling_convention,
                CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
            )
        {
            return Vec::new();
        }
        primary_return_registers(self.options.pointer_size, self.options.calling_convention)
    }

    fn callother_is_same_instruction_call_marker(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        op.opcode == PcodeOpcode::CallOther
            && op.output.is_none()
            && op.inputs.len() == 1
            && block
                .ops
                .iter()
                .skip(op_idx + 1)
                .take_while(|candidate| candidate.address == op.address)
                .any(|candidate| {
                    matches!(candidate.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
                })
    }

    fn call_result_is_observed(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let ret_regs = self.call_result_registers();
        if ret_regs.is_empty() {
            return false;
        }
        for candidate in block.ops.iter().skip(op_idx + 1) {
            if candidate.inputs.iter().any(|input| {
                ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, input))
            }) {
                return true;
            }
            if let Some(output) = candidate.output.as_ref()
                && ret_regs
                    .iter()
                    .any(|ret_reg| self.varnode_aliases_value(ret_reg, output))
            {
                return false;
            }
        }
        false
    }

    fn ensure_call_result_binding(&mut self, site: LoweringSite, op: &PcodeOp) -> String {
        if let Some(name) = self.call_result_bindings.get(&site) {
            return name.clone();
        }
        let ret_regs = self.call_result_registers();
        let Some(ret_reg) = ret_regs.first() else {
            return self
                .ensure_temp_binding_for_output(
                    op,
                    &Varnode {
                        space_id: UNIQUE_SPACE_ID,
                        offset: u64::from(op.seq_num),
                        size: self.options.pointer_size,
                        is_constant: false,
                        constant_val: 0,
                    },
                    false,
                )
                .name;
        };
        let name = next_temp_name(&type_from_size(ret_reg.size, false), &mut self.temp_next_id);
        self.temps.insert(
            name.clone(),
            NirBinding {
                name: name.clone(),
                ty: type_from_size(ret_reg.size, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            },
        );
        self.call_result_bindings.insert(site, name.clone());
        name
    }

    pub(in crate::nir) fn lower_block_stmts(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let terminator_index = self.block_terminator_index(block);
        let block_idx = self.lowering_block_index(block);
        body.extend(self.synthesize_explicit_merge_bindings_for_block(block)?);
        for (op_idx, op) in block.ops.iter().enumerate() {
            if Some(op_idx) == terminator_index {
                continue;
            }
            let site = LoweringSite { block_idx, op_idx };
            let maybe_stmt = self.with_lowering_site(
                site,
                |this| -> Result<Option<HirStmt>, MlilPreviewError> {
                    let mut visiting = HashSet::new();
                    match op.opcode {
                        PcodeOpcode::Store => {
                            if op.inputs.len() < 3 {
                                this.debug_lowering_error(
                                    "store_malformed_skip",
                                    block.start_address,
                                    u64::from(op.seq_num),
                                    op.opcode,
                                    &MlilPreviewError::UnsupportedExprMemoryBackedVarnode,
                                );
                                return Ok(None);
                            }
                            if this.is_callee_saved_push_store(op)
                                || this.is_call_return_scaffold_store(block, op_idx, op)
                            {
                                return Ok(None);
                            }
                            let store_ty = type_from_size(op.inputs[2].size, false);
                            let lhs = if let Some((slot_name, _slot_ty)) = this
                                .try_stack_slot_lvalue_for_memory_op(
                                    op,
                                    &op.inputs[1],
                                    store_ty.clone(),
                                ) {
                                HirLValue::Var(slot_name)
                            } else if let Some(global_lvalue) =
                                this.try_global_memory_lvalue(op, &op.inputs[1], store_ty.clone())
                            {
                                global_lvalue
                            } else {
                                HirLValue::Deref {
                                    ptr: Box::new(
                                        this.lower_varnode(&op.inputs[1], &mut HashSet::new())
                                            .map_err(|err| {
                                                this.debug_lowering_error(
                                                    "store_ptr",
                                                    block.start_address,
                                                    u64::from(op.seq_num),
                                                    op.opcode,
                                                    &err,
                                                );
                                                err
                                            })?,
                                    ),
                                    ty: store_ty,
                                }
                            };
                            let rhs = if let Some(expr) = this
                                .recover_aggregate_store_rhs_from_block(
                                    block,
                                    op_idx,
                                    &op.inputs[2],
                                )? {
                                expr
                            } else {
                                this.lower_varnode(&op.inputs[2], &mut HashSet::new())
                                    .map_err(|err| {
                                        this.debug_lowering_error(
                                            "store_rhs",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    })?
                            };
                            Ok(Some(HirStmt::Assign { lhs, rhs }))
                        }
                        PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                            if this.call_is_return_target_artifact(block, op_idx)
                                || this.call_is_terminal_branchind_artifact(block, op_idx)
                                || this.callother_is_same_instruction_call_marker(block, op_idx)
                            {
                                return Ok(None);
                            }
                            if op.output.is_none() {
                                let recovered_args =
                                    if op.opcode == PcodeOpcode::CallOther || op.inputs.len() > 1 {
                                        None
                                    } else {
                                        this.recover_call_args_from_block(block, op_idx)?
                                    };
                                let expr = this
                                    .lower_call(op, recovered_args, &mut visiting)
                                    .map_err(|err| {
                                        this.debug_lowering_error(
                                            "call_expr",
                                            block.start_address,
                                            u64::from(op.seq_num),
                                            op.opcode,
                                            &err,
                                        );
                                        err
                                    })?;
                                if op.opcode != PcodeOpcode::CallOther
                                    && this.call_result_is_observed(block, op_idx)
                                {
                                    let lhs =
                                        HirLValue::Var(this.ensure_call_result_binding(site, op));
                                    Ok(Some(HirStmt::Assign { lhs, rhs: expr }))
                                } else {
                                    Ok(Some(HirStmt::Expr(expr)))
                                }
                            } else {
                                this.maybe_materialize_output_stmt(
                                    block.start_address,
                                    block,
                                    op_idx,
                                    terminator_index,
                                    op,
                                )
                            }
                        }
                        _ => this.maybe_materialize_output_stmt(
                            block.start_address,
                            block,
                            op_idx,
                            terminator_index,
                            op,
                        ),
                    }
                },
            )?;
            if let Some(stmt) = maybe_stmt {
                body.push(stmt);
            }
        }
        Ok(body)
    }

    fn lowering_block_index(&self, block: &crate::pcode::PcodeBasicBlock) -> usize {
        let indexed = block.index as usize;
        if self.pcode.blocks.get(indexed).is_some_and(|candidate| {
            candidate.start_address == block.start_address && candidate.ops.len() == block.ops.len()
        }) {
            return indexed;
        }
        self.address_to_index
            .get(&block.start_address)
            .copied()
            .unwrap_or(0)
    }

    fn maybe_materialize_output_stmt(
        &mut self,
        block_addr: u64,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
    ) -> Result<Option<HirStmt>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if self.output_used_only_as_stack_return_target(block, op_idx, terminator_index, op, output)
        {
            return Ok(None);
        }
        if self.output_is_stack_pointer_register(output) {
            return Ok(None);
        }
        if self.output_used_only_by_single_store(block, op_idx, output) {
            return Ok(None);
        }
        if Self::is_predicate_passthrough_to_terminator(op)
            && self.output_used_only_by_block_terminator(block, op_idx, terminator_index, output)
        {
            return Ok(None);
        }
        let loop_carried_lhs_name = self
            .loop_carried_output_binding_name(block, op_idx, op, output)
            .or_else(|| {
                self.loop_carried_passthrough_output_binding_name(block, op_idx, op, output)
            });
        if loop_carried_lhs_name.is_none()
            && self.output_used_only_by_passthrough_chain(block, op_idx, output)
        {
            return Ok(None);
        }
        let block_idx_for_rhs = self.address_to_index.get(&block.start_address).copied();
        let scoped_loop_keys = loop_carried_lhs_name.as_ref().and_then(|name| {
            let block_idx = block_idx_for_rhs?;
            let output_key = VarnodeKey::from(output);
            let mut keys = vec![output_key.clone()];
            for input in &op.inputs {
                if input.is_constant {
                    continue;
                }
                let input_key = VarnodeKey::from(input);
                if input_key != output_key
                    && Self::varnode_key_may_alias_output(&input_key, &output_key)
                {
                    keys.push(input_key);
                }
            }
            keys.sort_by_key(|key| (key.space_id, key.offset, key.size));
            keys.dedup();
            let previous = keys
                .into_iter()
                .map(|key| {
                    let scoped_key = (block_idx, key);
                    let previous = self
                        .explicit_merge_bindings
                        .insert(scoped_key.clone(), name.clone());
                    (scoped_key, previous)
                })
                .collect::<Vec<_>>();
            self.invalidate_materialization_dependent_caches();
            Some(previous)
        });
        let rhs = if scoped_loop_keys.is_some()
            && let Some(block_idx) = block_idx_for_rhs
        {
            self.with_lowering_site(LoweringSite { block_idx, op_idx }, |this| {
                this.try_lower_materialized_output_rhs(block_addr, op)
            })
        } else {
            self.try_lower_materialized_output_rhs(block_addr, op)
        };
        if let Some(previous_bindings) = scoped_loop_keys {
            for (key, previous) in previous_bindings {
                if let Some(previous) = previous {
                    self.explicit_merge_bindings.insert(key, previous);
                } else {
                    self.explicit_merge_bindings.remove(&key);
                }
            }
            self.invalidate_materialization_dependent_caches();
        }
        let Some(rhs) = rhs? else {
            return Ok(None);
        };
        let legacy_inline_candidate =
            self.output_replacement_is_complete(block, op_idx, output, &rhs);
        let replacement_plan =
            self.build_replacement_value_plan(block, op_idx, terminator_index, output, &rhs);
        let merge_lhs_name = if loop_carried_lhs_name.is_none() {
            self.merge_binding_name_for_materialized_output(block, op_idx, output, &rhs)
        } else {
            None
        };
        if replacement_plan.is_complete()
            && loop_carried_lhs_name.is_none()
            && merge_lhs_name.is_none()
        {
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "representative_downgrade",
            );
            self.telemetry
                .materialization
                .representative_downgrade_count += 1;
            return Ok(None);
        }
        let no_consumer_profile =
            self.analyze_no_consumer_materialization_profile(block, op_idx, output, &rhs);
        let no_consumer_hazard = if replacement_plan.rejection_reason()
            == Some(MaterializationRejectionReason::AliasUnsafe)
        {
            Some(Self::classify_alias_unsafe_hazard(
                block,
                op_idx,
                terminator_index,
                output,
                &rhs,
            ))
        } else {
            None
        };
        let no_consumer_decision = Self::classify_no_consumer_materialization_decision(
            output,
            &rhs,
            legacy_inline_candidate,
            replacement_plan,
            no_consumer_hazard,
            no_consumer_profile,
        );
        match no_consumer_decision {
            NoConsumerMaterializationDecision::Suppress
            | NoConsumerMaterializationDecision::SuppressAlways => {
                let suppression_enabled = matches!(
                    no_consumer_decision,
                    NoConsumerMaterializationDecision::SuppressAlways
                ) || Self::no_consumer_suppression_enabled();
                self.trace_no_consumer_materialization(
                    block_addr,
                    op.seq_num,
                    if suppression_enabled {
                        "suppressed"
                    } else {
                        "suppression_candidate"
                    },
                    output,
                    &rhs,
                    Self::should_preserve_materialized_expr(&rhs),
                    legacy_inline_candidate,
                    no_consumer_profile,
                );
                self.trace_no_consumer_suppression_detail(
                    block,
                    op_idx,
                    output,
                    &rhs,
                    suppression_enabled,
                );
                if suppression_enabled && merge_lhs_name.is_none() {
                    self.trace_no_consumer_suppressed(block_addr, op.seq_num, output, &rhs);
                    return Ok(None);
                }
                self.trace_no_consumer_kept(
                    block_addr,
                    op.seq_num,
                    output,
                    &rhs,
                    NoConsumerMaterializationKeepReason::SuppressionDisabled,
                );
            }
            NoConsumerMaterializationDecision::Keep(reason) => {
                if reason != NoConsumerMaterializationKeepReason::NotUnknownNoConsumerFound {
                    self.trace_no_consumer_materialization(
                        block_addr,
                        op.seq_num,
                        "kept",
                        output,
                        &rhs,
                        Self::should_preserve_materialized_expr(&rhs),
                        legacy_inline_candidate,
                        no_consumer_profile,
                    );
                    self.trace_no_consumer_kept(block_addr, op.seq_num, output, &rhs, reason);
                }
            }
        }
        if legacy_inline_candidate {
            self.telemetry
                .materialization
                .materialization_inline_suppressed_count += 1;
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "inline_suppressed",
            );
        } else {
            self.trace_materialization_plan(
                block_addr,
                op,
                output,
                &rhs,
                replacement_plan,
                "materialized_binding",
            );
        }
        let preserve_materialization = Self::should_preserve_materialized_expr(&rhs);
        let lhs_name = if let Some(name) = loop_carried_lhs_name {
            self.seed_loop_carried_binding_initializer_from_edge_zero(block, output, &name);
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some(name) = merge_lhs_name {
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else {
            self.ensure_temp_binding_for_output(op, output, preserve_materialization)
                .name
        };
        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn merge_binding_name_for_materialized_output(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<String> {
        let block_idx = self.lowering_block_index(block);
        let key = VarnodeKey::from(output);
        for succ_idx in self.successors.get(block_idx)? {
            if let Some(name) = self.explicit_merge_bindings.get(&(*succ_idx, key.clone())) {
                return Some(name.clone());
            }
        }

        let proof = self.describe_merge_binding_candidate_proof(block, op_idx, output, rhs)?;
        let duplicate_start = self.duplicate_start_merge_block(proof.merge_block);
        if !self.merge_binding_proof_allows_predecessor_assignment(&proof, duplicate_start) {
            return None;
        }
        let (merge_idx, merge_addr, _, _) =
            self.first_output_use_site_outside_block_by_index(block_idx, output)?;
        if merge_addr == proof.merge_block
            && let Some(name) = self.explicit_merge_bindings.get(&(merge_idx, key.clone()))
        {
            return Some(name.clone());
        }
        if merge_addr != proof.merge_block
            || !self
                .successors
                .get(block_idx)
                .is_some_and(|succs| succs.contains(&merge_idx))
        {
            return None;
        }
        let binding = self.ensure_explicit_merge_binding_for_block(merge_idx, output);
        self.trace_explicit_merge_binding_trial(
            proof.merge_block,
            output,
            &[],
            &[],
            &proof.incoming_value_kinds,
            proof.rhs_kind,
            &binding.name,
            true,
            ExplicitMergeBindingTrialReason::PhiLikeBindingMaterialized,
        );
        Some(binding.name)
    }

    fn merge_binding_proof_allows_predecessor_assignment(
        &self,
        proof: &MergeBindingCandidateProof,
        duplicate_start: bool,
    ) -> bool {
        proof.can_synthesize_phi_like_binding
            && (proof.predecessor_count > 2 || (duplicate_start && proof.predecessor_count == 2))
            && proof.missing_incoming_count == 0
            && proof.conflicting_incoming_count >= 1
            && matches!(
                proof.consumer_kind,
                DisallowedSingleConsumerConsumerKind::OtherData
                    | DisallowedSingleConsumerConsumerKind::Predicate
            )
            && proof.incoming_value_kinds.iter().all(|kind| {
                matches!(
                    kind,
                    MergeBindingCandidateIncomingKind::VarOrConst
                        | MergeBindingCandidateIncomingKind::Arithmetic
                )
            })
    }

    fn duplicate_start_merge_block(&self, merge_block: u64) -> bool {
        self.pcode
            .blocks
            .iter()
            .filter(|block| block.start_address == merge_block)
            .take(2)
            .count()
            >= 2
    }

    fn output_used_only_as_stack_return_target(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        if !self
            .stack_pointer_register_name(&op.inputs[1])
            .is_some_and(|name| matches!(name, "rsp" | "esp" | "sp"))
        {
            return false;
        }
        let Some(term_idx) = terminator_index else {
            return false;
        };
        let Some(term) = block.ops.get(term_idx) else {
            return false;
        };
        term.opcode == PcodeOpcode::Return
            && term.inputs.last().is_some_and(|input| input == output)
            && self
                .output_use_sites_in_block(block, op_idx, output)
                .into_iter()
                .all(|(use_idx, _)| use_idx == term_idx)
    }

    fn output_is_stack_pointer_register(&self, output: &Varnode) -> bool {
        self.stack_pointer_register_name(output)
            .is_some_and(|name| matches!(name, "rsp" | "esp" | "sp"))
    }

    fn is_predicate_passthrough_to_terminator(op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::BoolXor
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
        )
    }

    pub(in crate::nir::builder) fn try_lower_materialized_output_rhs(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
    ) -> Result<Option<HirExpr>, MlilPreviewError> {
        let Some(output) = &op.output else {
            return Ok(None);
        };
        if !is_materializable_output_opcode(op.opcode) {
            return Ok(None);
        }
        let rhs = match self.lower_def_op(op, &mut HashSet::new()) {
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
        let _ = output;
        Ok(Some(rhs))
    }

    fn output_replacement_is_complete(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> bool {
        let uses = self.output_use_sites_in_block(block, op_idx, output);
        uses.len() == 1
            && Self::expr_is_low_cost_builder_inline_candidate(rhs)
            && if Self::expr_requires_passthrough_single_use_inline(rhs) {
                Self::use_opcode_allows_passthrough_single_use_builder_inline(uses[0].1.opcode)
            } else {
                Self::use_opcode_allows_single_use_builder_inline(uses[0].1.opcode)
            }
    }

    fn build_replacement_value_plan(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> ReplacementValuePlan {
        self.telemetry
            .materialization
            .replacement_plan_candidate_count += 1;
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
        rhs: &HirExpr,
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
        rhs: &HirExpr,
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

    pub(in crate::nir::builder) fn block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
        block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PcodeBasicBlock;
    use crate::nir::builder::materialize::test_support::{
        block, block_at, constant, op, pcode_function,
    };
    use crate::nir::render_mlil_preview;

    fn register(space_id: u64, offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    #[test]
    fn call_result_observation_accepts_partial_return_register_reads() {
        let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
        let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
        let block = block(vec![
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(2, PcodeOpcode::IntAdd, Some(out), vec![ebx, ret_eax]),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(builder.call_result_is_observed(&block, 0));
    }

    #[test]
    fn predecessor_assignment_accepts_predicate_merge_consumers() {
        let pcode = pcode_function(vec![block(Vec::new())]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let proof = MergeBindingCandidateProof {
            merge_block: 0x2000,
            predecessor_count: 3,
            missing_incoming_count: 0,
            conflicting_incoming_count: 1,
            incoming_value_kinds: vec![
                MergeBindingCandidateIncomingKind::VarOrConst,
                MergeBindingCandidateIncomingKind::Arithmetic,
            ],
            consumer_kind: DisallowedSingleConsumerConsumerKind::Predicate,
            rhs_kind: DisallowedSingleConsumerRhsKind::VarOrConst,
            can_synthesize_phi_like_binding: true,
            result: MergeBindingCandidateResult::PhiLikeBindingCandidate,
        };

        assert!(builder.merge_binding_proof_allows_predecessor_assignment(
            &proof, false,
        ));
    }

    #[test]
    fn call_result_observation_stops_at_partial_return_register_clobber() {
        let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
        let block = block(vec![
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::Copy,
                Some(ret_eax.clone()),
                vec![constant(1)],
            ),
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(out),
                vec![ret_eax, constant(2)],
            ),
        ]);
        let pcode = pcode_function(vec![block.clone()]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(!builder.call_result_is_observed(&block, 0));
    }

    #[test]
    fn partial_return_register_reads_resolve_to_live_call_result_binding() {
        let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
        let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
        let block = block(vec![
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(out),
                vec![ebx, ret_eax.clone()],
            ),
        ]);
        let pcode = pcode_function(vec![block]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.call_result_bindings.insert(
            LoweringSite {
                block_idx: 0,
                op_idx: 0,
            },
            "xVarCall".to_string(),
        );
        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 0,
            op_idx: 1,
        });

        assert_eq!(
            builder.live_call_result_binding_for_return_register(&ret_eax),
            Some("xVarCall".to_string())
        );
    }

    #[test]
    fn same_instruction_callother_does_not_steal_arm_call_args_or_result() {
        fn op_at(
            seq_num: u32,
            address: u64,
            opcode: PcodeOpcode,
            output: Option<Varnode>,
            inputs: Vec<Varnode>,
        ) -> PcodeOp {
            PcodeOp {
                seq_num,
                opcode,
                address,
                output,
                inputs,
                asm_mnemonic: None,
            }
        }

        let r0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 32, 4);
        let r1 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 36, 4);
        let out = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x4000, 4);
        let block = block_at(
            0x1000,
            0,
            vec![
                op_at(
                    0,
                    0x1000,
                    PcodeOpcode::Copy,
                    Some(r0.clone()),
                    vec![Varnode::constant(7, 4)],
                ),
                op_at(
                    1,
                    0x1002,
                    PcodeOpcode::CallOther,
                    None,
                    vec![Varnode::constant(62, 4)],
                ),
                op_at(
                    2,
                    0x1002,
                    PcodeOpcode::Call,
                    None,
                    vec![Varnode::constant(0x2000, 4)],
                ),
                op_at(3, 0x1004, PcodeOpcode::IntAdd, Some(out), vec![r1, r0]),
            ],
        );
        let pcode = pcode_function(vec![block.clone()]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.is_64bit = false;
        options.pointer_size = 4;
        options.calling_convention = CallingConvention::Arm32;
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let stmts = builder
            .lower_block_stmts(&block)
            .expect("lower ARM call block");

        assert!(
            matches!(
                &stmts[0],
                HirStmt::Assign {
                    rhs: HirExpr::Call { args, .. },
                    ..
                } if matches!(args.as_slice(), [HirExpr::Const(7, _)])
            ),
            "{stmts:?}"
        );
    }

    #[test]
    fn lower_block_stmts_uses_block_index_for_duplicate_start_addresses() {
        let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let ptr = Varnode::constant(0x3000, 8);
        let first_duplicate = block_at(
            0x2000,
            1,
            vec![op(
                1,
                PcodeOpcode::Copy,
                Some(x0.clone()),
                vec![constant(3)],
            )],
        );
        let second_duplicate = block_at(
            0x2000,
            2,
            vec![
                op(2, PcodeOpcode::Copy, Some(x0), vec![constant(7)]),
                op(3, PcodeOpcode::Store, None, vec![constant(3), ptr, w0]),
            ],
        );
        let pcode = pcode_function(vec![
            block_at(0x1000, 0, Vec::new()),
            first_duplicate,
            second_duplicate.clone(),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let stmts = builder
            .lower_block_stmts(&second_duplicate)
            .expect("lower duplicate block");

        assert!(
            matches!(
                stmts.as_slice(),
                [HirStmt::Assign {
                    rhs: HirExpr::Cast {
                        expr,
                        ..
                    },
                ..
            }] if matches!(expr.as_ref(), HirExpr::Const(7, _))
            ),
            "{stmts:?}"
        );
    }

    #[test]
    fn lookup_def_site_allows_unique_low_view_of_wide_temp() {
        let wide = Varnode {
            space_id: RUST_SLEIGH_UNIQUE_SPACE_ID,
            offset: 0x40b00,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let low = Varnode {
            size: 4,
            ..wide.clone()
        };
        let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40, 8);
        let pcode = pcode_function(vec![block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(wide), vec![constant(7)]),
                op(1, PcodeOpcode::IntZExt, Some(x8), vec![low.clone()]),
            ],
        )]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 0,
            op_idx: 1,
        });

        let (site, producer) = builder
            .lookup_def_site(&low)
            .expect("wide unique def covers low view");

        assert_eq!(site.block_idx, 0);
        assert_eq!(site.op_idx, 0);
        assert_eq!(producer.seq_num, 0);
    }

    #[test]
    fn duplicate_start_join_uses_shared_merge_binding_for_conflicting_defs() {
        fn op_at(
            seq_num: u32,
            address: u64,
            opcode: PcodeOpcode,
            output: Option<Varnode>,
            inputs: Vec<Varnode>,
        ) -> PcodeOp {
            PcodeOp {
                seq_num,
                opcode,
                address,
                output,
                inputs,
                asm_mnemonic: None,
            }
        }

        let merge = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82b00, 4);
        let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let denom = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
        let cond = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82c00, 1);
        let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let x30 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40f0, 8);
        let ret_target = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2, 1],
                ops: vec![
                    op_at(
                        0,
                        0x1000,
                        PcodeOpcode::Copy,
                        Some(merge.clone()),
                        vec![Varnode::constant(0, 4)],
                    ),
                    op_at(
                        1,
                        0x1000,
                        PcodeOpcode::CBranch,
                        None,
                        vec![Varnode::constant(2, 8), cond],
                    ),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![op_at(
                    2,
                    0x1010,
                    PcodeOpcode::IntDiv,
                    Some(merge.clone()),
                    vec![param, denom],
                )],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1010,
                successors: Vec::new(),
                ops: vec![
                    op_at(
                        3,
                        0x1010,
                        PcodeOpcode::IntAdd,
                        Some(w0),
                        vec![merge, Varnode::constant(5, 4)],
                    ),
                    op_at(
                        4,
                        0x1014,
                        PcodeOpcode::Copy,
                        Some(ret_target),
                        vec![x30.clone()],
                    ),
                    op_at(5, 0x1014, PcodeOpcode::Return, None, vec![x30]),
                ],
            },
        ]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;

        let code =
            render_mlil_preview(&pcode, "duplicate_merge", 0x1000, &options).expect("render");
        assert!(code.contains("if ("), "{code}");
        assert!(code.contains(" / "), "{code}");
        assert!(code.contains(" + 5"), "{code}");
    }

    #[test]
    fn duplicate_start_join_preserves_register_addend_after_zero_extend() {
        fn op_at(
            seq_num: u32,
            address: u64,
            opcode: PcodeOpcode,
            output: Option<Varnode>,
            inputs: Vec<Varnode>,
        ) -> PcodeOp {
            PcodeOp {
                seq_num,
                opcode,
                address,
                output,
                inputs,
                asm_mnemonic: None,
            }
        }

        let merge = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82b00, 4);
        let cond = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82c00, 1);
        let dividend = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4048, 4);
        let denom = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
        let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let factor = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4050, 4);
        let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
        let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
        let product = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x51200, 4);
        let madd_sum = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x51400, 4);
        let ret = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let x30 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40f0, 8);
        let ret_target = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2, 1],
                ops: vec![
                    op_at(
                        0,
                        0x1000,
                        PcodeOpcode::Copy,
                        Some(merge.clone()),
                        vec![Varnode::constant(0, 4)],
                    ),
                    op_at(
                        2,
                        0x1010,
                        PcodeOpcode::CBranch,
                        None,
                        vec![Varnode::constant(2, 8), cond],
                    ),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![op_at(
                    3,
                    0x1010,
                    PcodeOpcode::IntDiv,
                    Some(merge.clone()),
                    vec![dividend, denom],
                )],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1010,
                successors: Vec::new(),
                ops: vec![
                    op_at(
                        4,
                        0x1010,
                        PcodeOpcode::IntZExt,
                        Some(x8.clone()),
                        vec![merge],
                    ),
                    op_at(
                        5,
                        0x1014,
                        PcodeOpcode::IntMult,
                        Some(product.clone()),
                        vec![param.clone(), factor],
                    ),
                    op_at(
                        6,
                        0x1014,
                        PcodeOpcode::IntAdd,
                        Some(madd_sum.clone()),
                        vec![w8, product],
                    ),
                    op_at(7, 0x1014, PcodeOpcode::IntZExt, Some(x8), vec![madd_sum]),
                    op_at(
                        8,
                        0x1018,
                        PcodeOpcode::IntXor,
                        Some(ret),
                        vec![param, register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4)],
                    ),
                    op_at(
                        9,
                        0x101c,
                        PcodeOpcode::Copy,
                        Some(ret_target),
                        vec![x30.clone()],
                    ),
                    op_at(10, 0x101c, PcodeOpcode::Return, None, vec![x30]),
                ],
            },
        ]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;

        let code = render_mlil_preview(&pcode, "madd_addend", 0x1000, &options).expect("render");
        assert!(code.contains(" * "), "{code}");
        assert!(code.contains(" + "), "{code}");
        assert!(code.contains(" / "), "{code}");
        assert!(!code.contains("{\n    }"), "{code}");
    }
}
