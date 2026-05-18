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
                    | CallingConvention::Mips32
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

    fn callother_is_guarded_trap_marker(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
    ) -> bool {
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        if op.opcode != PcodeOpcode::CallOther || op.output.is_some() {
            return false;
        }
        let block_idx = self.lowering_block_index(block);
        let Some(preds) = self.predecessors.get(block_idx) else {
            return false;
        };
        preds.iter().any(|pred_idx| {
            let pred = self.pcode_block(*pred_idx);
            let Some(term_idx) = self.block_terminator_index(pred) else {
                return false;
            };
            let term = &pred.ops[term_idx];
            if term.opcode != PcodeOpcode::CBranch || term.address != op.address {
                return false;
            }
            let Some(target_seq) = term
                .inputs
                .first()
                .and_then(|target| instruction_local_branch_target_seq(term, target))
            else {
                return false;
            };
            block
                .ops
                .iter()
                .enumerate()
                .any(|(target_op_idx, candidate)| {
                    target_op_idx > op_idx && candidate.seq_num == target_seq
                })
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
                                || this.x86_32_store_is_recovered_call_arg(block, op_idx)
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
                            } else if let HirLValue::Var(slot_name) = &lhs
                                && let Some(expr) = this.stack_home_accumulator_store_rhs(
                                    block,
                                    op_idx,
                                    op,
                                    slot_name,
                                    &op.inputs[2],
                                )
                            {
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
                                || this.callother_is_guarded_trap_marker(block, op_idx)
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
        let direct_successor_merge_lhs_name =
            self.merge_binding_name_for_direct_successor_accumulator(block, output, &rhs);
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
                if suppression_enabled
                    && merge_lhs_name.is_none()
                    && direct_successor_merge_lhs_name.is_none()
                {
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
        let lhs_name = if let Some(name) = direct_successor_merge_lhs_name {
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some(name) = loop_carried_lhs_name {
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
        } else if let Some((name, binding_size)) = self
            .live_register_lhs_name_for_passthrough_join_store_producer(block, op_idx, output, &rhs)
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
            name
        } else if let Some((name, binding_size)) = self
            .live_register_lhs_name_for_safe_missing_merge(
                block,
                op_idx,
                op,
                output,
                &rhs,
                replacement_plan,
            )
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
            name
        } else {
            self.ensure_temp_binding_for_output(op, output, preserve_materialization)
                .name
        };
        let lhs = HirLValue::Var(lhs_name);
        Ok(Some(HirStmt::Assign { lhs, rhs }))
    }

    fn live_register_lhs_name_for_passthrough_join_store_producer(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<(String, u32)> {
        if output.is_constant
            || !is_unique_space_id(output.space_id)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
        {
            return None;
        }
        for (_, consumer_op) in self.output_use_sites_in_block(block, op_idx, output) {
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
            let consumer_rhs = HirExpr::Var("producer".to_string());
            let Some((name, binding_size)) = self.live_register_lhs_name_for_safe_missing_merge(
                block,
                op_idx,
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

    fn live_register_lhs_name_for_safe_missing_merge(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
        rhs: &HirExpr,
        replacement_plan: ReplacementValuePlan,
    ) -> Option<(String, u32)> {
        if replacement_plan.rejection_reason()
            != Some(MaterializationRejectionReason::MissingMergeBinding)
            || output.is_constant
            || !is_register_space_id(output.space_id)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
        {
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
        let output_key = VarnodeKey::from(output);
        if !live_register_loop_carried {
            self.gpr_family_index_for_key(&output_key)?;
        }
        if self.options.calling_convention == CallingConvention::AArch64
            && output.size == 8
            && matches!(op.opcode, PcodeOpcode::IntZExt | PcodeOpcode::Cast)
            && op.inputs.first().is_some_and(|input| input.size <= 4)
        {
            return aarch64_ghidra_reg_name(output.offset, 4).map(|name| (name.to_string(), 4));
        }
        if live_register_loop_carried {
            let name = register_hardware_name_for_abi(
                output.offset,
                output.size,
                self.options.calling_convention,
            )?;
            if crate::arch::x86::x86_gpr_family_index(name).is_none()
                && self.gpr_family_index_for_key(&output_key).is_none()
            {
                return None;
            }
            self.trace_path_sensitive_register_merge(
                block.start_address,
                op.seq_num,
                output,
                proof.relation,
                proof.consumer_kind,
                name,
            );
            return Some((name.to_string(), output.size));
        }
        None
    }

    fn rhs_is_safe_scalar_live_register_merge(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(..) => true,
            HirExpr::Cast { ty, expr } | HirExpr::Unary { ty, expr, .. } => {
                Self::type_is_scalar_live_register_merge(ty)
                    && Self::rhs_is_safe_scalar_live_register_merge(expr)
            }
            HirExpr::Binary { ty, lhs, rhs, .. } => {
                Self::type_is_scalar_live_register_merge(ty)
                    && Self::rhs_is_safe_scalar_live_register_merge(lhs)
                    && Self::rhs_is_safe_scalar_live_register_merge(rhs)
            }
            HirExpr::Call { .. }
            | HirExpr::Load { .. }
            | HirExpr::PtrOffset { .. }
            | HirExpr::Index { .. }
            | HirExpr::AggregateCopy { .. }
            | HirExpr::Select { .. } => false,
        }
    }

    fn type_is_scalar_live_register_merge(ty: &NirType) -> bool {
        matches!(ty, NirType::Bool | NirType::Int { .. })
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
        if let Some(name) =
            self.merge_binding_name_for_direct_successor_accumulator(block, output, rhs)
        {
            return Some(name);
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

    fn merge_binding_name_for_direct_successor_accumulator(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<String> {
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || output.size != self.options.pointer_size
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
        {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "shape_or_abi",
            );
            return None;
        }
        let output_key = VarnodeKey::from(output);
        if self.gpr_family_index_for_key(&output_key).is_none()
            && !is_primary_return_register_for_abi(output, self.options.calling_convention)
        {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_gpr_family",
            );
            return None;
        }
        let block_idx = self.lowering_block_index(block);
        let Some(succ_idx) = self.single_successor_index(block_idx) else {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_single_successor",
            );
            return None;
        };
        let Some(predecessor_idxs) = self.predecessors.get(succ_idx) else {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "missing_predecessors",
            );
            return None;
        };
        let predecessor_idxs = predecessor_idxs.clone();
        if predecessor_idxs.len() != 2 || !predecessor_idxs.contains(&block_idx) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_binary_predecessor_join",
            );
            return None;
        }
        let succ_block = self.pcode.blocks.get(succ_idx)?;
        if !self.block_reads_merge_input_before_redefinition(succ_block, output) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "successor_does_not_read_before_redefine",
            );
            return None;
        }
        for pred_idx in &predecessor_idxs {
            let pred_block = self.pcode.blocks.get(*pred_idx)?;
            let Some(def_idx) = self.last_redefinition_index_before_terminator(pred_block, output)
            else {
                self.trace_direct_successor_accumulator_merge_rejected(
                    block.start_address,
                    output,
                    "missing_pred_definition",
                );
                return None;
            };
            if !Self::output_def_is_safe_direct_successor_merge(&pred_block.ops[def_idx]) {
                self.trace_direct_successor_accumulator_merge_rejected(
                    block.start_address,
                    output,
                    "unsafe_pred_definition",
                );
                return None;
            }
            if Self::has_side_effect_between_ops(pred_block, def_idx + 1, pred_block.ops.len()) {
                self.trace_direct_successor_accumulator_merge_rejected(
                    block.start_address,
                    output,
                    "side_effect_after_pred_definition",
                );
                return None;
            }
        }
        let binding = self.ensure_explicit_merge_binding_for_block(succ_idx, output);
        let predecessor_addrs = predecessor_idxs
            .iter()
            .filter_map(|idx| self.pcode.blocks.get(*idx).map(|block| block.start_address))
            .collect::<Vec<_>>();
        self.trace_direct_successor_accumulator_merge_accepted(
            block.start_address,
            succ_block.start_address,
            output,
            &predecessor_addrs,
            &binding.name,
        );
        Some(binding.name)
    }

    fn rewrite_block_entry_accumulator_rhs_with_live_gpr(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
        rhs: HirExpr,
    ) -> HirExpr {
        if !matches!(
            self.options.calling_convention,
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
        ) || !self.options.is_64bit
            || !Self::output_def_is_safe_direct_successor_merge(op)
        {
            return rhs;
        }
        let Some(site) = self.current_lowering_site else {
            return rhs;
        };
        let Some(block) = self.pcode.blocks.get(site.block_idx) else {
            return rhs;
        };
        if block.start_address != block_addr {
            return rhs;
        }
        match rhs {
            HirExpr::Binary {
                op: binary_op,
                lhs,
                rhs,
                ty,
            } => {
                let lhs = self.rewrite_block_entry_accumulator_input_expr(
                    site.block_idx,
                    site.op_idx,
                    op.seq_num,
                    op.inputs.first(),
                    *lhs,
                );
                let rhs = self.rewrite_block_entry_accumulator_input_expr(
                    site.block_idx,
                    site.op_idx,
                    op.seq_num,
                    op.inputs.get(1),
                    *rhs,
                );
                HirExpr::Binary {
                    op: binary_op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    ty,
                }
            }
            other => other,
        }
    }

    fn rewrite_block_entry_accumulator_input_expr(
        &mut self,
        block_idx: usize,
        op_idx: usize,
        op_seq: u32,
        input: Option<&Varnode>,
        expr: HirExpr,
    ) -> HirExpr {
        let Some(input) = input else {
            return expr;
        };
        if input.size != self.options.pointer_size {
            self.trace_block_entry_accumulator_read_merge_rejected(
                block_idx,
                op_seq,
                input,
                "partial_width_input",
            );
            return expr;
        }
        let Some((live_name, family_idx)) = self.canonical_x86_gpr64_name_for_value(input) else {
            return expr;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            self.trace_block_entry_accumulator_read_merge_rejected(
                block_idx,
                op_seq,
                input,
                "stack_pointer_or_abi_param",
            );
            return expr;
        }
        if matches!(&expr, HirExpr::Var(name) if name == live_name) {
            return expr;
        }
        if let Err(join_reason) =
            self.block_entry_incoming_accumulator_read_is_proven(block_idx, op_idx, family_idx)
        {
            if let Err(exit_reason) =
                self.loop_exit_accumulator_read_is_proven(block_idx, op_idx, family_idx, live_name)
            {
                let reason = if join_reason == "not_loop_local_join" {
                    exit_reason
                } else {
                    join_reason
                };
                self.trace_block_entry_accumulator_read_merge_rejected(
                    block_idx, op_seq, input, reason,
                );
                return expr;
            }
        }
        self.ensure_live_register_binding(live_name, self.options.pointer_size);
        self.trace_block_entry_accumulator_read_merge_accepted(block_idx, op_seq, input, live_name);
        HirExpr::Var(live_name.to_string())
    }

    fn block_entry_incoming_accumulator_read_is_proven(
        &self,
        block_idx: usize,
        op_idx: usize,
        family_idx: usize,
    ) -> Result<(), &'static str> {
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return Err("missing_block");
        };
        if Self::has_aliasing_side_effect_between_ops(block, 0, op_idx) {
            return Err("side_effect_before_read");
        }
        if block
            .ops
            .iter()
            .take(op_idx)
            .any(|candidate| self.op_defines_x86_gpr_family(candidate, family_idx))
        {
            return Err("local_redefinition_before_read");
        }
        let Some(predecessors) = self.predecessors.get(block_idx) else {
            return Err("missing_predecessors");
        };
        if predecessors.len() == 1 {
            return Err("not_join_block");
        }
        if predecessors.len() < 2 {
            return Err("missing_predecessors");
        }
        let Some(loop_body) = self
            .loop_bodies
            .iter()
            .filter(|loop_body| loop_body.body.contains(&block_idx))
            .find(|loop_body| {
                predecessors
                    .iter()
                    .all(|pred| loop_body.body.contains(pred))
            })
        else {
            return Err("not_loop_local_join");
        };
        if self.loop_body_has_side_entry_or_irreducible_edge(loop_body) {
            return Err("side_entry_or_irreducible");
        }
        if predecessors.iter().all(|pred| {
            self.pred_path_has_live_accumulator_def(*pred, block_idx, loop_body, family_idx)
        }) {
            Ok(())
        } else {
            Err("missing_predecessor_live_def")
        }
    }

    fn loop_exit_accumulator_read_is_proven(
        &self,
        block_idx: usize,
        op_idx: usize,
        family_idx: usize,
        live_name: &str,
    ) -> Result<(), &'static str> {
        if !self.temps.contains_key(live_name) {
            return Err("missing_existing_live_binding");
        }
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return Err("missing_block");
        };
        if Self::has_aliasing_side_effect_between_ops(block, 0, op_idx) {
            return Err("side_effect_before_read");
        }
        if block
            .ops
            .iter()
            .take(op_idx)
            .any(|candidate| self.op_defines_x86_gpr_family(candidate, family_idx))
        {
            return Err("local_redefinition_before_read");
        }
        if !self
            .single_successor_index(block_idx)
            .and_then(|succ| self.pcode.blocks.get(succ))
            .is_some_and(|succ| succ.ops.iter().any(|op| op.opcode == PcodeOpcode::Return))
        {
            return Err("not_return_exit_block");
        }
        let Some(predecessors) = self.predecessors.get(block_idx) else {
            return Err("missing_predecessors");
        };
        if predecessors.len() != 2 {
            return Err("not_binary_exit_join");
        }
        let Some((loop_body, loop_pred, external_pred)) =
            self.loop_exit_accumulator_context(block_idx, predecessors)
        else {
            return Err("not_loop_exit_join");
        };
        if self.loop_body_has_side_entry_or_irreducible_edge(&loop_body) {
            return Err("side_entry_or_irreducible");
        }
        if !self.pred_path_has_live_accumulator_def(loop_pred, block_idx, &loop_body, family_idx) {
            return Err("missing_loop_exit_live_def");
        }
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let mut visiting = HashSet::new();
        if !self.pred_path_has_zero_accumulator_seed(
            external_pred,
            block_idx,
            &body,
            family_idx,
            0,
            &mut visiting,
        ) {
            return Err("missing_external_zero_seed");
        }
        Ok(())
    }

    fn loop_exit_accumulator_context(
        &self,
        block_idx: usize,
        predecessors: &[usize],
    ) -> Option<(
        crate::nir::structuring::loop_analysis::LoopBody,
        usize,
        usize,
    )> {
        self.loop_bodies.iter().find_map(|loop_body| {
            if loop_body.body.contains(&block_idx) {
                return None;
            }
            let loop_preds = predecessors
                .iter()
                .copied()
                .filter(|pred| loop_body.body.contains(pred))
                .collect::<Vec<_>>();
            let external_preds = predecessors
                .iter()
                .copied()
                .filter(|pred| !loop_body.body.contains(pred))
                .collect::<Vec<_>>();
            if loop_preds.len() == 1
                && external_preds.len() == 1
                && (loop_body.exit_idx == Some(block_idx)
                    || loop_body.all_exits.contains(&block_idx)
                    || self
                        .successors
                        .get(loop_preds[0])
                        .is_some_and(|succs| succs.contains(&block_idx)))
            {
                Some((loop_body.clone(), loop_preds[0], external_preds[0]))
            } else {
                None
            }
        })
    }

    fn op_defines_x86_gpr_family(&self, op: &PcodeOp, family_idx: usize) -> bool {
        op.output
            .as_ref()
            .and_then(|output| self.canonical_x86_gpr64_name_for_value(output))
            .is_some_and(|(_, output_family)| output_family == family_idx)
    }

    fn single_successor_index(&self, block_idx: usize) -> Option<usize> {
        let successors = self.successors.get(block_idx)?;
        if successors.len() == 1 {
            Some(successors[0])
        } else {
            None
        }
    }

    fn block_reads_merge_input_before_redefinition(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> bool {
        for op in &block.ops {
            if op
                .inputs
                .iter()
                .any(|input| self.varnode_aliases_value(input, output))
            {
                return true;
            }
            if op
                .output
                .as_ref()
                .is_some_and(|candidate| self.varnode_aliases_value(candidate, output))
            {
                return false;
            }
        }
        false
    }

    fn last_redefinition_index_before_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> Option<usize> {
        block.ops.iter().enumerate().rev().find_map(|(idx, op)| {
            op.output
                .as_ref()
                .is_some_and(|candidate| self.varnode_aliases_value(candidate, output))
                .then_some(idx)
        })
    }

    fn output_def_is_safe_direct_successor_merge(op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::SubPiece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::Cast
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntNegate
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
        )
    }

    fn has_side_effect_between_ops(
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
            )
        })
    }

    fn stack_home_accumulator_store_rhs(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        _op_idx: usize,
        op: &PcodeOp,
        slot_name: &str,
        value: &Varnode,
    ) -> Option<HirExpr> {
        if op.opcode != PcodeOpcode::Store
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
            || !self.options.is_64bit
            || !matches!(value.size, 4 | 8)
        {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "shape_or_abi",
            );
            return None;
        }
        let Some((live_name, family_idx)) =
            self.canonical_x86_gpr64_name_for_store_value(op, value)
        else {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "not_x86_gpr",
            );
            return None;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "stack_pointer_or_abi_param",
            );
            return None;
        }
        if self.resolve_stack_address_from_memory_op(op).is_none()
            && op
                .inputs
                .get(1)
                .and_then(|ptr| self.resolve_stack_address(ptr))
                .is_none()
        {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "not_stable_stack_slot",
            );
            return None;
        }
        let block_idx = self.lowering_block_index(block);
        let Some((loop_body, store_is_loop_header)) =
            self.stack_home_accumulator_loop_context(block_idx)
        else {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "not_loop_header",
            );
            return None;
        };
        if self.loop_body_has_side_entry_or_irreducible_edge(&loop_body) {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "side_entry_or_irreducible",
            );
            return None;
        }
        if store_is_loop_header
            && !self
                .predecessors
                .get(block_idx)
                .is_some_and(|preds| preds.iter().any(|pred| loop_body.body.contains(pred)))
        {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "missing_loop_predecessor",
            );
            return None;
        }
        let live_backedge_def = if store_is_loop_header {
            self.predecessors
                .get(block_idx)
                .into_iter()
                .flatten()
                .filter(|pred| loop_body.body.contains(pred))
                .any(|pred| {
                    self.pred_path_has_live_accumulator_def(
                        *pred, block_idx, &loop_body, family_idx,
                    )
                })
        } else {
            self.loop_body_has_live_accumulator_def(&loop_body, family_idx)
        };
        if !live_backedge_def {
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                "missing_safe_backedge_definition",
            );
            return None;
        }
        let value_is_zero = self.varnode_known_const_zero(value, 8);
        let external_zero_seed = store_is_loop_header
            && self.loop_header_external_predecessors_seed_zero(block_idx, &loop_body, family_idx);
        let zero_entry_default = value_is_zero || external_zero_seed;
        if !zero_entry_default {
            let external_preds = self
                .predecessors
                .get(block_idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|pred| !loop_body.body.contains(pred))
                .collect::<Vec<_>>();
            let reason = format!(
                "missing_zero_entry_default:value_zero={} external_zero_seed={} external_preds={:?}",
                value_is_zero, external_zero_seed, external_preds
            );
            self.trace_stack_home_accumulator_store_merge_rejected(
                block.start_address,
                op.seq_num,
                slot_name,
                value,
                &reason,
            );
            return None;
        }

        self.ensure_live_register_binding(live_name, self.options.pointer_size);
        if let Some(binding) = self.temps.get_mut(live_name)
            && binding.initializer.is_none()
        {
            binding.initializer = Some(HirExpr::Const(
                0,
                type_from_size(self.options.pointer_size, false),
            ));
        }
        self.trace_stack_home_accumulator_store_merge_accepted(
            block.start_address,
            op.seq_num,
            slot_name,
            value,
            live_name,
        );
        Some(HirExpr::Var(live_name.to_string()))
    }

    fn loop_header_external_predecessors_seed_zero(
        &self,
        header_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        family_idx: usize,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let incoming = self
            .predecessors
            .get(header_idx)
            .into_iter()
            .flatten()
            .copied()
            .filter(|pred| !body.contains(pred))
            .collect::<Vec<_>>();
        !incoming.is_empty()
            && incoming.into_iter().all(|pred| {
                let mut visiting = HashSet::new();
                self.pred_path_has_zero_accumulator_seed(
                    pred,
                    header_idx,
                    &body,
                    family_idx,
                    0,
                    &mut visiting,
                )
            })
    }

    fn pred_path_has_zero_accumulator_seed(
        &self,
        idx: usize,
        header_idx: usize,
        loop_body: &HashSet<usize>,
        family_idx: usize,
        depth: usize,
        visiting: &mut HashSet<usize>,
    ) -> bool {
        if depth > 8 || idx == header_idx || loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return self.x86_gpr_definition_is_zero_in_block(block, def_idx, 4)
                    && !Self::has_aliasing_side_effect_between_ops(
                        block,
                        def_idx + 1,
                        block.ops.len(),
                    );
            }
            if Self::has_aliasing_side_effect_between_ops(block, 0, block.ops.len()) {
                return false;
            }
            let incoming = self
                .predecessors
                .get(idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|pred| *pred != header_idx && !loop_body.contains(pred))
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming.into_iter().all(|pred| {
                    self.pred_path_has_zero_accumulator_seed(
                        pred,
                        header_idx,
                        loop_body,
                        family_idx,
                        depth + 1,
                        visiting,
                    )
                })
        });
        visiting.remove(&idx);
        result
    }

    fn stack_home_accumulator_loop_context(
        &self,
        block_idx: usize,
    ) -> Option<(crate::nir::structuring::loop_analysis::LoopBody, bool)> {
        if let Some(loop_body) = self
            .loop_bodies
            .iter()
            .find(|loop_body| loop_body.head == block_idx && loop_body.body.contains(&block_idx))
        {
            return Some((loop_body.clone(), true));
        }
        self.successors.get(block_idx)?.iter().find_map(|succ| {
            self.loop_bodies
                .iter()
                .find(|loop_body| loop_body.head == *succ && !loop_body.body.contains(&block_idx))
                .cloned()
                .map(|loop_body| (loop_body, false))
        })
    }

    fn canonical_x86_gpr64_name_for_store_value(
        &self,
        op: &PcodeOp,
        value: &Varnode,
    ) -> Option<(&'static str, usize)> {
        self.canonical_x86_gpr64_name_for_value(value)
            .or_else(|| self.canonical_x86_gpr64_name_for_value_source(value, 4))
            .or_else(|| {
                let raw_name = Self::x86_store_source_register_name_from_asm(op)?;
                Self::canonical_x86_gpr64_name_for_raw_name(&raw_name)
            })
    }

    fn canonical_x86_gpr64_name_for_value(&self, value: &Varnode) -> Option<(&'static str, usize)> {
        let raw_name = register_hardware_name_for_abi(
            value.offset,
            value.size,
            self.options.calling_convention,
        )
        .or_else(|| register_name_32(value.offset, value.size))
        .or_else(|| unique_register_name(value.offset, value.size))?;
        Self::canonical_x86_gpr64_name_for_raw_name(raw_name)
    }

    fn canonical_x86_gpr64_name_for_value_source(
        &self,
        value: &Varnode,
        budget: usize,
    ) -> Option<(&'static str, usize)> {
        if budget == 0 {
            return None;
        }
        let Some((_, op)) = self.lookup_def_site(value) else {
            return None;
        };
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => {
                let input = op.inputs.first()?;
                self.canonical_x86_gpr64_name_for_value(input)
                    .or_else(|| self.canonical_x86_gpr64_name_for_value_source(input, budget - 1))
            }
            _ => None,
        }
    }

    fn canonical_x86_gpr64_name_for_raw_name(raw_name: &str) -> Option<(&'static str, usize)> {
        let family_idx = crate::arch::x86::x86_gpr_family_index(raw_name)?;
        const GPR64: [&str; 16] = [
            "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
            "r12", "r13", "r14", "r15",
        ];
        GPR64
            .get(family_idx)
            .copied()
            .map(|name| (name, family_idx))
    }

    fn x86_store_source_register_name_from_asm(op: &PcodeOp) -> Option<String> {
        let asm = op.asm_mnemonic.as_deref()?.trim();
        let source = asm.rsplit_once(',')?.1.trim();
        let source = source
            .split_whitespace()
            .next()
            .unwrap_or(source)
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
            .to_ascii_lowercase();
        crate::arch::x86::x86_gpr_family_index(&source).map(|_| source)
    }

    fn loop_body_has_side_entry_or_irreducible_edge(
        &self,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        for block_idx in &loop_body.body {
            if self
                .predecessors
                .get(*block_idx)
                .into_iter()
                .flatten()
                .any(|pred| !body.contains(pred) && *block_idx != loop_body.head)
            {
                return true;
            }
        }
        self.irreducible_edges
            .iter()
            .any(|(from, to)| body.contains(from) || body.contains(to))
    }

    fn pred_path_has_live_accumulator_def(
        &self,
        pred_idx: usize,
        target_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        family_idx: usize,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let mut visiting = HashSet::new();
        self.pred_path_has_live_accumulator_def_inner(
            pred_idx,
            target_idx,
            &body,
            family_idx,
            0,
            &mut visiting,
        )
    }

    fn pred_path_has_live_accumulator_def_inner(
        &self,
        idx: usize,
        target_idx: usize,
        loop_body: &HashSet<usize>,
        family_idx: usize,
        depth: usize,
        visiting: &mut HashSet<usize>,
    ) -> bool {
        if depth > 8 || idx == target_idx || !loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return !Self::has_aliasing_side_effect_between_ops(
                    block,
                    def_idx + 1,
                    block.ops.len(),
                );
            }
            if Self::has_aliasing_side_effect_between_ops(block, 0, block.ops.len()) {
                return false;
            }
            let incoming = self
                .predecessors
                .get(idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|pred| *pred != target_idx && loop_body.contains(pred))
                .collect::<Vec<_>>();
            !incoming.is_empty()
                && incoming.into_iter().all(|pred| {
                    self.pred_path_has_live_accumulator_def_inner(
                        pred,
                        target_idx,
                        loop_body,
                        family_idx,
                        depth + 1,
                        visiting,
                    )
                })
        });
        visiting.remove(&idx);
        result
    }

    fn loop_body_has_live_accumulator_def(
        &self,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        family_idx: usize,
    ) -> bool {
        loop_body.body.iter().any(|idx| {
            self.pcode
                .blocks
                .get(*idx)
                .and_then(|block| self.last_x86_gpr_family_definition(block, family_idx))
                .is_some()
        })
    }

    fn last_x86_gpr_family_definition(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        family_idx: usize,
    ) -> Option<usize> {
        block.ops.iter().enumerate().rev().find_map(|(idx, op)| {
            let output = op.output.as_ref()?;
            let (_, output_family) = self.canonical_x86_gpr64_name_for_value(output)?;
            (output_family == family_idx && Self::output_def_is_safe_direct_successor_merge(op))
                .then_some(idx)
        })
    }

    fn x86_gpr_definition_is_zero_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        budget: usize,
    ) -> bool {
        if budget == 0 {
            return false;
        }
        let Some(op) = block.ops.get(op_idx) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy => op
                .inputs
                .first()
                .is_some_and(|input| input.is_constant && input.constant_val == 0),
            PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => op.inputs.first().is_some_and(|input| {
                input.is_constant && input.constant_val == 0
                    || self.value_has_prior_zero_def_in_block(block, op_idx, input, budget - 1)
            }),
            PcodeOpcode::IntXor if op.inputs.len() >= 2 => {
                self.varnode_aliases_value(&op.inputs[0], &op.inputs[1])
            }
            _ => false,
        }
    }

    fn value_has_prior_zero_def_in_block(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        before_idx: usize,
        value: &Varnode,
        budget: usize,
    ) -> bool {
        if budget == 0 {
            return false;
        }
        block.ops[..before_idx.min(block.ops.len())]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, candidate)| {
                candidate
                    .output
                    .as_ref()
                    .is_some_and(|output| self.varnode_aliases_value(output, value))
                    .then_some(idx)
            })
            .is_some_and(|idx| self.x86_gpr_definition_is_zero_in_block(block, idx, budget - 1))
    }

    fn has_aliasing_side_effect_between_ops(
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Load
                    | PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
            )
        })
    }

    fn varnode_known_const_zero(&self, value: &Varnode, budget: usize) -> bool {
        if value.is_constant {
            return value.constant_val == 0;
        }
        if budget == 0 {
            return false;
        }
        let Some((_, op)) = self.lookup_def_site(value) else {
            return false;
        };
        match op.opcode {
            PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::SubPiece => op
                .inputs
                .first()
                .is_some_and(|input| self.varnode_known_const_zero(input, budget - 1)),
            PcodeOpcode::IntXor if op.inputs.len() >= 2 => {
                self.varnode_aliases_value(&op.inputs[0], &op.inputs[1])
            }
            _ => false,
        }
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
        let rhs = self.rewrite_block_entry_accumulator_rhs_with_live_gpr(block_addr, op, rhs);
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
        block, block_at, constant, int, op, pcode_function,
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

        assert!(builder.merge_binding_proof_allows_predecessor_assignment(&proof, false,));
    }

    #[test]
    fn direct_successor_return_register_merge_uses_shared_edge_binding() {
        let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2],
                ops: vec![
                    op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(5)]),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    op(3, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(7)]),
                    op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: Vec::new(),
                ops: vec![op(
                    5,
                    PcodeOpcode::IntAdd,
                    Some(r12.clone()),
                    vec![r12, rax.clone()],
                )],
            },
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Const(5, type_from_size(8, false));

        let name = builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &rax, &rhs)
            .expect("shared return register merge binding");

        assert!(
            builder
                .explicit_merge_bindings
                .contains_key(&(2, VarnodeKey::from(&rax)))
        );
        assert_eq!(
            builder
                .explicit_merge_bindings
                .get(&(2, VarnodeKey::from(&rax))),
            Some(&name)
        );
    }

    #[test]
    fn direct_successor_return_register_merge_rejects_side_effect_after_def() {
        let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
        let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2],
                ops: vec![
                    op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(5)]),
                    op(
                        2,
                        PcodeOpcode::Store,
                        None,
                        vec![constant(3), ptr, constant(0)],
                    ),
                    op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    op(4, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(7)]),
                    op(5, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: Vec::new(),
                ops: vec![op(
                    6,
                    PcodeOpcode::IntAdd,
                    Some(r12.clone()),
                    vec![r12, rax.clone()],
                )],
            },
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Const(5, type_from_size(8, false));

        assert!(
            builder
                .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &rax, &rhs)
                .is_none()
        );
    }

    #[test]
    fn direct_successor_accumulator_merge_uses_shared_gpr_edge_binding() {
        let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 8);
        let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2],
                ops: vec![
                    op(1, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(5)]),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    op(3, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(7)]),
                    op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: Vec::new(),
                ops: vec![op(
                    5,
                    PcodeOpcode::IntAdd,
                    Some(rax),
                    vec![r12.clone(), constant(1)],
                )],
            },
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Const(5, type_from_size(8, false));

        let name = builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &r12, &rhs)
            .expect("shared accumulator merge binding");

        assert_eq!(
            builder
                .explicit_merge_bindings
                .get(&(2, VarnodeKey::from(&r12))),
            Some(&name)
        );
    }

    #[test]
    fn direct_successor_accumulator_merge_rejects_partial_register_output() {
        let r12d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
        let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
        let pcode = pcode_function(vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![2],
                ops: vec![
                    op(1, PcodeOpcode::Copy, Some(r12d.clone()), vec![constant(5)]),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    op(3, PcodeOpcode::Copy, Some(r12d.clone()), vec![constant(7)]),
                    op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: Vec::new(),
                ops: vec![op(
                    5,
                    PcodeOpcode::IntAdd,
                    Some(rax),
                    vec![r12d.clone(), constant(1)],
                )],
            },
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Const(5, type_from_size(4, false));

        assert!(
            builder
                .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &r12d, &rhs,)
                .is_none()
        );
    }

    #[test]
    fn stack_home_accumulator_store_uses_seeded_live_gpr_binding() {
        let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 4);
        let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
        let mut store = op(
            2,
            PcodeOpcode::Store,
            None,
            vec![constant(0), rsp_addr, ebp.clone()],
        );
        store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                    op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    store.clone(),
                    op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(
                        4,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(1)],
                    ),
                    op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![op(6, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let rhs = builder
            .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &ebp)
            .expect("stack-home accumulator merge");

        assert_eq!(rhs, HirExpr::Var("rbp".to_string()));
        assert!(builder.params.is_empty(), "must not promote rbp to a param");
        assert_eq!(
            builder
                .temps
                .get("rbp")
                .and_then(|binding| binding.initializer.as_ref()),
            Some(&HirExpr::Const(0, type_from_size(8, false)))
        );
    }

    #[test]
    fn stack_home_accumulator_store_accepts_joined_backedge_defs() {
        let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 4);
        let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
        let store_value = register(UNIQUE_SPACE_ID, 0xd400, 4);
        let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
        let mut store = op(
            3,
            PcodeOpcode::Store,
            None,
            vec![constant(0), rsp_addr, store_value.clone()],
        );
        store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                    op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    op(
                        2,
                        PcodeOpcode::Copy,
                        Some(store_value.clone()),
                        vec![ebp.clone()],
                    ),
                    store.clone(),
                    op(
                        4,
                        PcodeOpcode::CBranch,
                        None,
                        vec![constant(0x1060), cond.clone()],
                    ),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![op(
                    5,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1040), cond.clone()],
                )],
            ),
            block_at(
                0x1030,
                3,
                vec![
                    op(
                        6,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(1)],
                    ),
                    op(7, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
                ],
            ),
            block_at(
                0x1040,
                4,
                vec![
                    op(
                        8,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(2)],
                    ),
                    op(9, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
                ],
            ),
            block_at(
                0x1050,
                5,
                vec![op(11, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1060,
                6,
                vec![op(12, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let rhs = builder.with_lowering_site(
            LoweringSite {
                block_idx: 1,
                op_idx: 1,
            },
            |builder| {
                builder
                    .stack_home_accumulator_store_rhs(
                        &pcode.blocks[1],
                        1,
                        &store,
                        "home_4c",
                        &store_value,
                    )
                    .expect("stack-home accumulator merge across joined backedge")
            },
        );

        assert_eq!(rhs, HirExpr::Var("rbp".to_string()));
        assert!(builder.params.is_empty(), "must not promote rbp to a param");
    }

    #[test]
    fn block_entry_accumulator_read_uses_joined_live_gpr_binding() {
        let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let tmp = register(UNIQUE_SPACE_ID, 0x8f00, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
        let read_op = op(
            10,
            PcodeOpcode::IntAdd,
            Some(tmp),
            vec![rbp.clone(), constant(1)],
        );
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(rbp.clone()), vec![constant(0)]),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(
                    3,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1060), cond.clone()],
                )],
            ),
            block_at(
                0x1020,
                2,
                vec![op(
                    4,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1030), cond.clone()],
                )],
            ),
            block_at(
                0x1030,
                3,
                vec![
                    op(
                        5,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(1)],
                    ),
                    op(6, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
                ],
            ),
            block_at(
                0x1040,
                4,
                vec![
                    op(
                        7,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(2)],
                    ),
                    op(8, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
                ],
            ),
            block_at(
                0x1050,
                5,
                vec![
                    read_op.clone(),
                    op(11, PcodeOpcode::Branch, None, vec![constant(0x1060)]),
                ],
            ),
            block_at(
                0x1060,
                6,
                vec![op(12, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.predecessors[5] = vec![3, 4];
        builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
            head: 1,
            tails: vec![5],
            body: vec![1, 2, 3, 4, 5],
            exit_idx: Some(6),
            all_exits: vec![6],
        }];
        let stale_rhs = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("xVar53".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(64))),
            ty: int(64),
        };

        let rewritten = builder.with_lowering_site(
            LoweringSite {
                block_idx: 5,
                op_idx: 0,
            },
            |builder| {
                builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                    pcode.blocks[5].start_address,
                    &read_op,
                    stale_rhs,
                )
            },
        );

        assert_eq!(
            rewritten,
            HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("rbp".to_string())),
                rhs: Box::new(HirExpr::Const(1, int(64))),
                ty: int(64),
            }
        );
        assert!(builder.params.is_empty(), "must not promote rbp to a param");
    }

    #[test]
    fn block_entry_accumulator_read_accepts_loop_exit_zero_seed() {
        let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let tmp = register(UNIQUE_SPACE_ID, 0x9300, 8);
        let read_op = op(
            20,
            PcodeOpcode::IntMult,
            Some(tmp),
            vec![rbp.clone(), constant(1)],
        );
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(rbp.clone()), vec![constant(0)]),
                    op(2, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)])],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(
                        4,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(1)],
                    ),
                    op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![
                    read_op.clone(),
                    op(21, PcodeOpcode::Branch, None, vec![constant(0x1040)]),
                ],
            ),
            block_at(
                0x1040,
                4,
                vec![op(22, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.ensure_live_register_binding("rbp", 8);
        builder.predecessors[3] = vec![0, 2];
        builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
            head: 1,
            tails: vec![2],
            body: vec![1, 2],
            exit_idx: Some(3),
            all_exits: vec![3],
        }];
        let stale_rhs = HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(HirExpr::Var("xVar53".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(64))),
            ty: int(64),
        };

        let rewritten = builder.with_lowering_site(
            LoweringSite {
                block_idx: 3,
                op_idx: 0,
            },
            |builder| {
                builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                    pcode.blocks[3].start_address,
                    &read_op,
                    stale_rhs,
                )
            },
        );

        assert_eq!(
            rewritten,
            HirExpr::Binary {
                op: HirBinaryOp::Mul,
                lhs: Box::new(HirExpr::Var("rbp".to_string())),
                rhs: Box::new(HirExpr::Const(1, int(64))),
                ty: int(64),
            }
        );
        assert!(builder.params.is_empty(), "must not promote rbp to a param");
    }

    #[test]
    fn stack_home_accumulator_store_rejects_side_effect_after_live_def() {
        let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 4);
        let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
        let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
        let load_tmp = register(UNIQUE_SPACE_ID, 0x208, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
        let mut store = op(
            2,
            PcodeOpcode::Store,
            None,
            vec![constant(0), rsp_addr.clone(), ebp.clone()],
        );
        store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                    op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    store.clone(),
                    op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![
                    op(
                        4,
                        PcodeOpcode::IntAdd,
                        Some(rbp.clone()),
                        vec![rbp.clone(), constant(1)],
                    ),
                    op(
                        5,
                        PcodeOpcode::Load,
                        Some(load_tmp),
                        vec![constant(0), rsp_addr],
                    ),
                    op(6, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1030,
                3,
                vec![op(7, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &ebp)
                .is_none()
        );
    }

    #[test]
    fn stack_home_accumulator_store_rejects_partial_register_value() {
        let bp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 2);
        let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
        let mut store = op(
            2,
            PcodeOpcode::Store,
            None,
            vec![constant(0), rsp_addr, bp.clone()],
        );
        store.asm_mnemonic = Some("MOV word ptr [RSP+0x4c], BP".to_string());
        let pcode = pcode_function(vec![
            block_at(
                0x1000,
                0,
                vec![
                    op(1, PcodeOpcode::Copy, Some(bp.clone()), vec![constant(0)]),
                    op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
                ],
            ),
            block_at(
                0x1010,
                1,
                vec![
                    store.clone(),
                    op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
                ],
            ),
            block_at(
                0x1020,
                2,
                vec![op(4, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
            ),
            block_at(
                0x1030,
                3,
                vec![op(5, PcodeOpcode::Return, None, vec![constant(0)])],
            ),
        ]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        assert!(
            builder
                .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &bp)
                .is_none()
        );
    }

    #[test]
    fn explicit_merge_select_materializes_store_value_diamond() {
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

        let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let lhs = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4008, 4);
        let rhs = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4010, 4);
        let merge_value = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4028, 4);
        let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4030, 8);
        let first = PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2, 1],
            ops: vec![
                op_at(
                    0,
                    0x1000,
                    PcodeOpcode::IntSub,
                    Some(merge_value.clone()),
                    vec![lhs.clone(), rhs.clone()],
                ),
                op_at(
                    1,
                    0x1004,
                    PcodeOpcode::CBranch,
                    None,
                    vec![Varnode::constant(0x1020, 8), param],
                ),
            ],
        };
        let alternate = PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![op_at(
                2,
                0x1010,
                PcodeOpcode::IntSub,
                Some(merge_value.clone()),
                vec![rhs, lhs],
            )],
        };
        let merge = PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op_at(
                3,
                0x1020,
                PcodeOpcode::Store,
                None,
                vec![Varnode::constant(3, 8), ptr, merge_value.clone()],
            )],
        };
        let pcode = pcode_function(vec![first.clone(), alternate.clone(), merge.clone()]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let stmts = builder
            .synthesize_explicit_merge_bindings_for_block(&merge)
            .expect("synthesize merge binding");

        assert!(
            matches!(
                stmts.as_slice(),
                [HirStmt::Assign {
                    rhs: HirExpr::Select { .. },
                    ..
                }]
            ),
            "{stmts:?}"
        );
    }

    #[test]
    fn missing_merge_aarch64_zero_extend_uses_low_live_register_binding_for_safe_rhs() {
        let x12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4060, 8);
        let w12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4060, 4);
        let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
        let def_op = op(1, PcodeOpcode::IntZExt, Some(x12.clone()), vec![w8]);
        let mut def_block = block_at(0x1000, 0, vec![def_op.clone()]);
        def_block.successors = vec![1];
        let merge_block = block_at(
            0x2000,
            1,
            vec![op(
                2,
                PcodeOpcode::IntEqual,
                Some(register(UNIQUE_SPACE_ID, 0x100, 1)),
                vec![w12, constant(0)],
            )],
        );
        let pcode = pcode_function(vec![def_block.clone(), merge_block]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Cast {
            ty: int(64),
            expr: Box::new(HirExpr::Cast {
                ty: int(32),
                expr: Box::new(HirExpr::Var("xVar7".to_string())),
            }),
        };

        assert_eq!(
            builder.live_register_lhs_name_for_safe_missing_merge(
                &def_block,
                0,
                &def_op,
                &x12,
                &rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ),
            Some(("w12".to_string(), 4))
        );
    }

    #[test]
    fn missing_join_store_value_uses_low_live_register_binding_for_safe_rhs() {
        let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 8);
        let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
        let value = register(UNIQUE_SPACE_ID, 0x100, 4);
        let def_op = op(1, PcodeOpcode::IntZExt, Some(x0.clone()), vec![value]);
        let def_block = block_at(
            0x1000,
            0,
            vec![
                def_op.clone(),
                op(
                    4,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x2000), register(UNIQUE_SPACE_ID, 0x200, 1)],
                ),
            ],
        );
        let other_pred = block_at(
            0x1800,
            1,
            vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x2000)])],
        );
        let merge_block = block_at(
            0x2000,
            2,
            vec![op(2, PcodeOpcode::Store, None, vec![constant(0), ptr, w0])],
        );
        let pcode = pcode_function(vec![def_block.clone(), other_pred, merge_block]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Cast {
            ty: int(64),
            expr: Box::new(HirExpr::Var("uVar1".to_string())),
        };

        assert_eq!(
            builder.live_register_lhs_name_for_safe_missing_merge(
                &def_block,
                0,
                &def_op,
                &x0,
                &rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ),
            Some(("w0".to_string(), 4))
        );
    }

    #[test]
    fn passthrough_join_store_producer_uses_low_live_register_binding() {
        let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 8);
        let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
        let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
        let add_out = register(UNIQUE_SPACE_ID, 0x100, 4);
        let add_op = op(
            1,
            PcodeOpcode::IntAdd,
            Some(add_out.clone()),
            vec![w0.clone(), constant(1)],
        );
        let zext_op = op(
            2,
            PcodeOpcode::IntZExt,
            Some(x0.clone()),
            vec![add_out.clone()],
        );
        let def_block = block_at(
            0x1000,
            0,
            vec![
                add_op,
                zext_op,
                op(
                    4,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x2000), register(UNIQUE_SPACE_ID, 0x200, 1)],
                ),
            ],
        );
        let other_pred = block_at(
            0x1800,
            1,
            vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x2000)])],
        );
        let merge_block = block_at(
            0x2000,
            2,
            vec![op(5, PcodeOpcode::Store, None, vec![constant(0), ptr, w0])],
        );
        let pcode = pcode_function(vec![def_block.clone(), other_pred, merge_block]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("w0".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(32))),
            ty: int(32),
        };

        assert_eq!(
            builder.live_register_lhs_name_for_passthrough_join_store_producer(
                &def_block, 0, &add_out, &rhs,
            ),
            Some(("w0".to_string(), 4))
        );
    }

    #[test]
    fn loop_header_missing_merge_uses_x64_live_register_binding() {
        let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
        let r15d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb8, 4);
        let store_ptr = register(UNIQUE_SPACE_ID, 0x100, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
        let def_op = op(1, PcodeOpcode::Copy, Some(r15d.clone()), vec![r14d.clone()]);
        let mut entry = block_at(
            0x1000,
            0,
            vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
        );
        entry.successors = vec![1];
        let mut header = block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::Store,
                    None,
                    vec![constant(0), store_ptr, r15d.clone()],
                ),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
            ],
        );
        header.successors = vec![3, 2];
        let mut body = block_at(
            0x1020,
            2,
            vec![
                def_op.clone(),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        );
        body.successors = vec![1];
        let exit = block_at(
            0x1030,
            3,
            vec![op(
                4,
                PcodeOpcode::Return,
                None,
                vec![constant(0), r15d.clone()],
            )],
        );
        let pcode = pcode_function(vec![entry, header, body.clone(), exit]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::WindowsX64;
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Var("r14".to_string());
        let proof = builder
            .describe_missing_merge_binding_proof(&body, 0, &r15d, &rhs)
            .expect("missing merge proof");
        assert_eq!(
            proof.relation,
            MissingMergeBindingRelation::LoopHeaderMergeMissing
        );
        assert_eq!(
            proof.consumer_kind,
            DisallowedSingleConsumerConsumerKind::StoreValue
        );
        assert_eq!(
            register_hardware_name_for_abi(r15d.offset, r15d.size, options.calling_convention),
            Some("r15")
        );

        assert_eq!(
            builder.live_register_lhs_name_for_safe_missing_merge(
                &body,
                0,
                &def_op,
                &r15d,
                &rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ),
            Some(("r15".to_string(), 4))
        );
    }

    #[test]
    fn loop_header_missing_merge_rejects_side_effect_rhs() {
        let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
        let r15d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb8, 4);
        let store_ptr = register(UNIQUE_SPACE_ID, 0x100, 8);
        let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
        let def_op = op(1, PcodeOpcode::Copy, Some(r15d.clone()), vec![r14d]);
        let mut entry = block_at(
            0x1000,
            0,
            vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
        );
        entry.successors = vec![1];
        let mut header = block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::Store,
                    None,
                    vec![constant(0), store_ptr, r15d.clone()],
                ),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
            ],
        );
        header.successors = vec![3, 2];
        let mut body = block_at(
            0x1020,
            2,
            vec![
                def_op.clone(),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        );
        body.successors = vec![1];
        let exit = block_at(
            0x1030,
            3,
            vec![op(
                4,
                PcodeOpcode::Return,
                None,
                vec![constant(0), r15d.clone()],
            )],
        );
        let pcode = pcode_function(vec![entry, header, body.clone(), exit]);
        let options = crate::nir::builder::materialize::test_support::test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Call {
            target: "may_call".to_string(),
            args: vec![HirExpr::Var("r14".to_string())],
            ty: int(32),
        };

        assert_eq!(
            builder.live_register_lhs_name_for_safe_missing_merge(
                &body,
                0,
                &def_op,
                &r15d,
                &rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ),
            None
        );
    }

    #[test]
    fn missing_merge_live_register_binding_rejects_call_or_aggregate_rhs() {
        let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
        let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
        let input = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x5020, 16);
        let def_op = op(1, PcodeOpcode::IntZExt, Some(x8.clone()), vec![input]);
        let mut def_block = block_at(0x1000, 0, vec![def_op.clone()]);
        def_block.successors = vec![1];
        let merge_block = block_at(
            0x2000,
            1,
            vec![op(
                2,
                PcodeOpcode::IntEqual,
                Some(register(UNIQUE_SPACE_ID, 0x100, 1)),
                vec![w8, constant(0)],
            )],
        );
        let pcode = pcode_function(vec![def_block.clone(), merge_block]);
        let mut options = crate::nir::builder::materialize::test_support::test_options();
        options.calling_convention = CallingConvention::AArch64;
        options.format = "ELF64".to_string();
        options.pe_x64_only = false;
        let builder = PreviewBuilder::new(&pcode, &options, None);
        let rhs = HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Call {
                target: "__pcodeop_294".to_string(),
                args: vec![HirExpr::Var("reg".to_string())],
                ty: NirType::Aggregate {
                    size: 16,
                    fields: Vec::new(),
                },
            }),
            rhs: Box::new(HirExpr::Const(4, int(32))),
            ty: int(32),
        };

        assert_eq!(
            builder.live_register_lhs_name_for_safe_missing_merge(
                &def_block,
                0,
                &def_op,
                &x8,
                &rhs,
                ReplacementValuePlan::incomplete(
                    ReplacementReadClass::Merge,
                    MaterializationRejectionReason::MissingMergeBinding,
                ),
            ),
            None
        );
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
    fn cross_block_return_register_reads_resolve_to_live_call_result_binding() {
        let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
        let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
        let mut call_block = block_at(
            0x1000,
            0,
            vec![op(1, PcodeOpcode::Call, None, vec![constant(0x2000)])],
        );
        call_block.successors = vec![1];
        let use_block = block_at(
            0x1010,
            1,
            vec![op(
                2,
                PcodeOpcode::IntAdd,
                Some(out),
                vec![ebx, ret_eax.clone()],
            )],
        );
        let pcode = pcode_function(vec![call_block, use_block]);
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
            block_idx: 1,
            op_idx: 0,
        });

        assert_eq!(
            builder.live_call_result_binding_for_return_register(&ret_eax),
            Some("xVarCall".to_string())
        );
    }

    #[test]
    fn cross_block_return_register_binding_stops_at_redefinition() {
        let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
        let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
        let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
        let mut call_block = block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
                op(
                    2,
                    PcodeOpcode::IntAdd,
                    Some(ret_eax.clone()),
                    vec![ret_eax.clone(), constant(1)],
                ),
            ],
        );
        call_block.successors = vec![1];
        let use_block = block_at(
            0x1010,
            1,
            vec![op(
                3,
                PcodeOpcode::IntAdd,
                Some(out),
                vec![ebx, ret_eax.clone()],
            )],
        );
        let pcode = pcode_function(vec![call_block, use_block]);
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
            block_idx: 1,
            op_idx: 0,
        });

        assert_eq!(
            builder.live_call_result_binding_for_return_register(&ret_eax),
            None
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
