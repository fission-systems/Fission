use super::*;

mod contracts;
mod cross_block;
mod incremental;
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
        self.register_namer().primary_return_registers()
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
        let block_idx = self.pcode_block_idx(block.index as usize);
        if let Some(cached) = self.lowered_block_stmts_cache.get(&block_idx) {
            return Ok(cached.clone());
        }

        let terminator_index = self.block_terminator_index(block);
        let mut body = self.synthesize_explicit_merge_bindings_for_block(block)?;
        body.extend(self.lower_block_ops_range(block, 0, block.ops.len(), terminator_index)?);

        self.lowered_block_stmts_cache
            .insert(block_idx, body.clone());
        Ok(body)
    }

    fn lower_block_ops_range(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        start_idx: usize,
        end_idx: usize,
        terminator_index: Option<usize>,
    ) -> Result<Vec<HirStmt>, MlilPreviewError> {
        let mut body = Vec::new();
        let block_idx = self.lowering_block_index(block);
        let mut op_idx = start_idx;
        while op_idx < end_idx {
            let op = &block.ops[op_idx];
            if Some(op_idx) == terminator_index {
                op_idx += 1;
                continue;
            }

            // Handle intra-block control flow (e.g. from cmov)
            if op.opcode == PcodeOpcode::CBranch && op.inputs.len() >= 2 && op.inputs[0].is_constant
            {
                if let Some(target_seq) =
                    crate::nir::cfg::instruction_local_branch_target_seq(op, &op.inputs[0])
                {
                    let target_op_idx_opt = block.ops[op_idx + 1..end_idx]
                        .iter()
                        .position(|candidate| candidate.seq_num == target_seq)
                        .map(|pos| op_idx + 1 + pos);
                    if let Some(target_op_idx) = target_op_idx_opt {
                        let cond = self.lower_varnode(&op.inputs[1], &mut HashSet::new())?;
                        let inverted_cond = HirExpr::Unary {
                            op: HirUnaryOp::Not,
                            expr: Box::new(cond),
                            ty: NirType::Bool,
                        };
                        let nested_body = self.lower_block_ops_range(
                            block,
                            op_idx + 1,
                            target_op_idx,
                            terminator_index,
                        )?;
                        body.push(HirStmt::If {
                            cond: inverted_cond,
                            then_body: nested_body,
                            else_body: Vec::new(),
                        });
                        op_idx = target_op_idx;
                        continue;
                    }
                }
            } else if op.opcode == PcodeOpcode::Branch
                && !op.inputs.is_empty()
                && op.inputs[0].is_constant
            {
                if let Some(target_seq) =
                    crate::nir::cfg::instruction_local_branch_target_seq(op, &op.inputs[0])
                {
                    let target_op_idx_opt = block.ops[op_idx + 1..end_idx]
                        .iter()
                        .position(|candidate| candidate.seq_num == target_seq)
                        .map(|pos| op_idx + 1 + pos);
                    if let Some(target_op_idx) = target_op_idx_opt {
                        op_idx = target_op_idx;
                        continue;
                    }
                }
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
            op_idx += 1;
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
        let lhs_name = if let Some(name) = loop_carried_lhs_name {
            self.seed_loop_carried_binding_initializer_from_edge_zero(block, output, &name);
            self.bind_materialized_output_to_existing_name(
                op,
                output,
                &name,
                preserve_materialization,
            );
            name
        } else if let Some(name) = direct_successor_merge_lhs_name {
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
        } else if let Some((name, binding_size)) =
            self.live_register_lhs_name_for_partial_gpr_join_family(output)
        {
            self.ensure_live_register_binding(&name, binding_size);
            self.bind_materialized_output_to_existing_name(op, output, &name, true);
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
            let fallback_name = self
                .ensure_temp_binding_for_output(op, output, preserve_materialization)
                .name;
            fallback_name
        };
        if self.emit_ready_trace_enabled_for_current_fn() {
            self.emit_ready_trace(format!(
                "materialized-output-binding block=0x{:x} op_seq={} output=space:{} off:0x{:x} size:{} lhs={} rhs={:?}",
                block_addr,
                op.seq_num,
                output.space_id,
                output.offset,
                output.size,
                lhs_name,
                rhs,
            ));
        }
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
            let name = self.sla_hw_name(output.offset, output.size)?;
            if crate::arch::x86::x86_gpr_family_index(name.as_str()).is_none()
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
                name.as_str(),
            );
            return Some((name, output.size));
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
            | HirExpr::FieldAccess { .. }
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
            self.first_output_use_site_outside_block_by_index(block_idx, op_idx, output)?;
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
        // Allow full-width registers (size == pointer_size) and also the 32-bit primary
        // return register (e.g. EAX in x86-64, size=4, offset=0). In x86-64, a 32-bit
        // write zero-extends to the full 64-bit register, so EAX and RAX are semantically
        // equivalent for accumulation. Other partial registers (r12d, etc.) remain rejected.
        let is_32bit_return_reg = self.options.is_64bit
            && output.size == 4
            && self.register_namer().is_primary_return_register(output);
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || (output.size != self.options.pointer_size && !is_32bit_return_reg)
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
            && !self.register_namer().is_primary_return_register(output)
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
            if let Some(name) =
                self.merge_binding_name_for_conditional_loop_exit_accumulator(block, output, rhs)
            {
                return Some(name);
            }
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_single_successor",
            );
            return None;
        };
        // The 32-bit return-register exception is ONLY valid for the conditional-exit
        // (multi-successor) path handled by merge_binding_name_for_conditional_loop_exit_accumulator.
        // For single-successor blocks (self-loops, simple backedge loops) the loop_carried
        // mechanism is the correct owner; reject here so it reaches that path unchanged.
        if is_32bit_return_reg {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "shape_or_abi_single_successor",
            );
            return None;
        }
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

    fn merge_binding_name_for_conditional_loop_exit_accumulator(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
        rhs: &HirExpr,
    ) -> Option<String> {
        // Allow the 32-bit primary return register (EAX in x86-64) in addition to full-width
        // registers. See the corresponding guard in merge_binding_name_for_direct_successor_accumulator.
        let is_32bit_return_reg = self.options.is_64bit
            && output.size == 4
            && self.register_namer().is_primary_return_register(output);
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || (output.size != self.options.pointer_size && !is_32bit_return_reg)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
        {
            return None;
        }
        let Some((live_name, family_idx)) = self.canonical_x86_gpr64_name_for_value(output) else {
            return None;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "stack_pointer_or_abi_param",
            );
            return None;
        }
        let block_idx = self.lowering_block_index(block);
        let succs = self.successors.get(block_idx)?.clone();
        if succs.len() != 2 {
            return None;
        }
        let read_succs = succs
            .iter()
            .copied()
            .filter(|succ_idx| {
                self.pcode.blocks.get(*succ_idx).is_some_and(|succ_block| {
                    self.block_reads_merge_input_before_redefinition(succ_block, output)
                })
            })
            .collect::<Vec<_>>();
        let [read_succ_idx] = read_succs.as_slice() else {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_single_read_successor",
            );
            return None;
        };
        let non_read_succ_idx = succs
            .iter()
            .copied()
            .find(|succ_idx| succ_idx != read_succ_idx)?;
        let loop_body = self.loop_bodies.iter().find(|loop_body| {
            loop_body.head == non_read_succ_idx
                && !loop_body.body.contains(read_succ_idx)
                && (loop_body.all_exits.contains(read_succ_idx)
                    || loop_body.exit_idx == Some(*read_succ_idx)
                    || self
                        .successors
                        .get(block_idx)
                        .is_some_and(|succs| succs.contains(read_succ_idx)))
        })?;
        if self.loop_body_has_side_entry_or_irreducible_edge(loop_body) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "side_entry_or_irreducible",
            );
            return None;
        }
        let block_is_loop_body = loop_body.body.contains(&block_idx);
        let block_is_external_seed = !block_is_loop_body
            && self
                .successors
                .get(block_idx)
                .is_some_and(|succs| succs.contains(&loop_body.head));
        if !block_is_loop_body && !block_is_external_seed {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_loop_latch_or_external_seed",
            );
            return None;
        }
        let Some(preds) = self.predecessors.get(*read_succ_idx) else {
            return None;
        };
        let preds = preds.clone();
        if !preds.contains(&block_idx) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "exit_predecessor_shape",
            );
            return None;
        }
        let Some(def_idx) = self.last_redefinition_index_before_terminator(block, output) else {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "missing_loop_latch_definition",
            );
            return None;
        };
        if !self.current_site_matches_block_op(block_idx, def_idx) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_current_last_definition",
            );
            return None;
        }
        if !Self::output_def_is_safe_direct_successor_merge(&block.ops[def_idx]) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "unsafe_loop_latch_definition",
            );
            return None;
        }
        if self.has_call_between_ops(block, def_idx + 1, block.ops.len()) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "side_effect_after_loop_latch_definition",
            );
            return None;
        }
        let old_zero_seed_shape = block_is_loop_body
            && preds.contains(&non_read_succ_idx)
            && self.loop_header_external_predecessors_seed_zero(
                non_read_succ_idx,
                loop_body,
                family_idx,
                false,
            );
        let external_seed_shape = self
            .conditional_loop_exit_external_seed_shape(
                block_idx,
                *read_succ_idx,
                loop_body,
                output,
                block_is_loop_body,
            )
            .is_some();
        if !old_zero_seed_shape && !external_seed_shape {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "missing_loop_header_seed",
            );
            return None;
        }

        let binding = self.ensure_explicit_merge_binding_for_block(*read_succ_idx, output);
        if old_zero_seed_shape
            && let Some(binding) = self.temps.get_mut(&binding.name)
            && binding.initializer.is_none()
        {
            binding.initializer = Some(HirExpr::Const(0, type_from_size(output.size, false)));
        }
        let predecessor_addrs = preds
            .iter()
            .filter_map(|idx| self.pcode.blocks.get(*idx).map(|block| block.start_address))
            .collect::<Vec<_>>();
        let read_succ_addr = self
            .pcode
            .blocks
            .get(*read_succ_idx)
            .map(|block| block.start_address)
            .unwrap_or_default();
        self.trace_direct_successor_accumulator_merge_accepted(
            block.start_address,
            read_succ_addr,
            output,
            &predecessor_addrs,
            &binding.name,
        );
        Some(binding.name)
    }

    fn current_site_matches_block_op(&self, block_idx: usize, op_idx: usize) -> bool {
        self.current_lowering_site
            .is_some_and(|site| site.block_idx == block_idx && site.op_idx == op_idx)
    }

    fn conditional_loop_exit_external_seed_shape(
        &self,
        block_idx: usize,
        read_succ_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        output: &Varnode,
        block_is_loop_body: bool,
    ) -> Option<(usize, usize)> {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let preds = self.predecessors.get(read_succ_idx)?;
        if preds.len() != 2 {
            return None;
        }
        let loop_pred = preds.iter().copied().find(|pred| body.contains(pred))?;
        let external_pred = preds.iter().copied().find(|pred| !body.contains(pred))?;
        if block_is_loop_body {
            if block_idx != loop_pred {
                return None;
            }
        } else if block_idx != external_pred {
            return None;
        }
        if !self
            .successors
            .get(external_pred)
            .is_some_and(|succs| succs.contains(&loop_body.head) && succs.contains(&read_succ_idx))
        {
            return None;
        }
        if !self.conditional_loop_exit_pred_def_is_safe(external_pred, output)
            || !self.conditional_loop_exit_pred_def_is_safe(loop_pred, output)
        {
            return None;
        }
        Some((external_pred, loop_pred))
    }

    fn conditional_loop_exit_pred_def_is_safe(&self, pred_idx: usize, output: &Varnode) -> bool {
        let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
            return false;
        };
        let Some(def_idx) = self.last_redefinition_index_before_terminator(pred_block, output)
        else {
            return false;
        };
        Self::output_def_is_safe_direct_successor_merge(&pred_block.ops[def_idx])
            && !self.has_call_between_ops(pred_block, def_idx + 1, pred_block.ops.len())
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
        if let Some(explicit_expr) = self.current_explicit_merge_binding_expr(block_idx, input) {
            return explicit_expr;
        }
        if input.size != self.options.pointer_size {
            if let Some(incoming_expr) =
                self.block_entry_partial_gpr_incoming_expr(block_idx, op_idx, op_seq, input)
            {
                return incoming_expr;
            }
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

    fn block_entry_partial_gpr_incoming_expr(
        &mut self,
        block_idx: usize,
        op_idx: usize,
        op_seq: u32,
        input: &Varnode,
    ) -> Option<HirExpr> {
        if input.is_constant
            || input.size >= self.options.pointer_size
            || !is_register_space_id(input.space_id)
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
            || !self.options.is_64bit
        {
            return None;
        }
        let Some((live_name, family_idx)) = self.canonical_x86_gpr64_name_for_value(input) else {
            return None;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            self.trace_block_entry_partial_gpr_merge_rejected(
                block_idx,
                op_seq,
                input,
                family_idx,
                &[],
                "stack_pointer_or_abi_param",
            );
            return None;
        }
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return None;
        };
        if self.has_call_between_ops(block, 0, op_idx) {
            self.trace_block_entry_partial_gpr_merge_rejected(
                block_idx,
                op_seq,
                input,
                family_idx,
                &[],
                "side_effect_before_read",
            );
            return None;
        }
        if block
            .ops
            .iter()
            .take(op_idx)
            .any(|candidate| self.op_defines_x86_gpr_family(candidate, family_idx))
        {
            self.trace_block_entry_partial_gpr_merge_rejected(
                block_idx,
                op_seq,
                input,
                family_idx,
                &[],
                "local_redefinition_before_read",
            );
            return None;
        }
        let predecessors = self
            .predecessors
            .get(block_idx)
            .cloned()
            .unwrap_or_default();
        let predecessor_addrs = predecessors
            .iter()
            .filter_map(|idx| self.pcode.blocks.get(*idx).map(|block| block.start_address))
            .collect::<Vec<_>>();
        if predecessors.len() < 2 {
            self.trace_block_entry_partial_gpr_merge_rejected(
                block_idx,
                op_seq,
                input,
                family_idx,
                &predecessor_addrs,
                "not_join_block",
            );
            return None;
        }
        if !self.block_entry_partial_gpr_loop_context_is_safe(block_idx, &predecessors) {
            self.trace_block_entry_partial_gpr_merge_rejected(
                block_idx,
                op_seq,
                input,
                family_idx,
                &predecessor_addrs,
                "side_entry_or_irreducible",
            );
            return None;
        }

        let mut incoming = Vec::new();
        for pred in predecessors {
            let mut visiting = HashSet::new();
            match self.partial_gpr_incoming_expr_from_pred_path(
                pred,
                block_idx,
                family_idx,
                input,
                0,
                &mut visiting,
            ) {
                Ok(expr) => incoming.push(expr),
                Err(reason) => {
                    self.trace_block_entry_partial_gpr_merge_rejected(
                        block_idx,
                        op_seq,
                        input,
                        family_idx,
                        &predecessor_addrs,
                        reason,
                    );
                    return None;
                }
            }
        }
        let Some(first) = incoming.first().cloned() else {
            return None;
        };
        if incoming.iter().all(|expr| expr == &first) {
            self.trace_block_entry_partial_gpr_merge_accepted(
                block_idx,
                op_seq,
                input,
                family_idx,
                &predecessor_addrs,
                &first,
            );
            return Some(first);
        }
        self.trace_block_entry_partial_gpr_merge_rejected(
            block_idx,
            op_seq,
            input,
            family_idx,
            &predecessor_addrs,
            "ambiguous_incoming_expr",
        );
        self.trace_block_entry_partial_gpr_merge_incoming_values(
            block_idx,
            op_seq,
            input,
            family_idx,
            &predecessor_addrs,
            &incoming,
        );
        if self.partial_gpr_join_family_needs_live_binding(family_idx) {
            self.ensure_live_register_binding(live_name, self.options.pointer_size);
            let expr = self.project_partial_gpr_incoming_expr(
                &Varnode {
                    space_id: input.space_id,
                    offset: input.offset,
                    size: self.options.pointer_size,
                    is_constant: false,
                    constant_val: 0,
                },
                input,
                HirExpr::Var(live_name.to_string()),
            );
            self.trace_block_entry_partial_gpr_merge_accepted(
                block_idx,
                op_seq,
                input,
                family_idx,
                &predecessor_addrs,
                &expr,
            );
            return Some(expr);
        }
        None
    }

    fn live_register_lhs_name_for_partial_gpr_join_family(
        &mut self,
        output: &Varnode,
    ) -> Option<(String, u32)> {
        if output.is_constant
            || !self.options.is_64bit
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
            || !is_register_space_id(output.space_id)
            || output.size > self.options.pointer_size
        {
            return None;
        }
        let Some((live_name, family_idx)) = self.canonical_x86_gpr64_name_for_value(output) else {
            return None;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            return None;
        }

        self.partial_gpr_join_family_needs_live_binding(family_idx)
            .then(|| (live_name.to_string(), self.options.pointer_size))
    }

    fn partial_gpr_join_family_needs_live_binding(&mut self, family_idx: usize) -> bool {
        if !self.options.is_64bit
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
        {
            return false;
        }

        if let Some(&needs_binding) = self.partial_gpr_live_binding_cache.get(&family_idx) {
            return needs_binding;
        }

        let needs_binding = self
            .pcode
            .blocks
            .iter()
            .enumerate()
            .any(|(block_idx, block)| {
                self.predecessors
                    .get(block_idx)
                    .is_some_and(|preds| preds.len() >= 2)
                    && block.ops.iter().enumerate().any(|(op_idx, op)| {
                        !self.has_call_between_ops(block, 0, op_idx)
                            && !block.ops.iter().take(op_idx).any(|candidate| {
                                self.op_defines_x86_gpr_family(candidate, family_idx)
                            })
                            && op.inputs.iter().any(|input| {
                                !input.is_constant
                                    && input.size < self.options.pointer_size
                                    && is_register_space_id(input.space_id)
                                    && self
                                        .canonical_x86_gpr64_name_for_value(input)
                                        .is_some_and(|(_, input_family)| input_family == family_idx)
                            })
                    })
            });

        self.partial_gpr_live_binding_cache
            .insert(family_idx, needs_binding);
        needs_binding
    }

    fn block_entry_partial_gpr_loop_context_is_safe(
        &self,
        block_idx: usize,
        predecessors: &[usize],
    ) -> bool {
        self.loop_bodies
            .iter()
            .filter(|loop_body| {
                loop_body.body.contains(&block_idx)
                    || loop_body.exit_idx == Some(block_idx)
                    || loop_body.all_exits.contains(&block_idx)
                    || predecessors
                        .iter()
                        .any(|pred| loop_body.body.contains(pred))
            })
            .all(|loop_body| !self.loop_body_has_side_entry_or_irreducible_edge(loop_body))
    }

    fn partial_gpr_incoming_expr_from_pred_path(
        &mut self,
        pred_idx: usize,
        target_idx: usize,
        family_idx: usize,
        requested: &Varnode,
        depth: usize,
        visiting: &mut HashSet<usize>,
    ) -> Result<HirExpr, &'static str> {
        if depth > 8 || (pred_idx == target_idx && depth > 0) || !visiting.insert(pred_idx) {
            return Err("ambiguous_predecessor_path");
        }
        let Some(block) = self.pcode.blocks.get(pred_idx) else {
            visiting.remove(&pred_idx);
            return Err("missing_predecessor_block");
        };
        let def_idx = self.last_x86_gpr_family_definition(block, family_idx);
        if let Some(def_idx) = def_idx {
            let has_materialized_def = block
                .ops
                .get(def_idx)
                .and_then(|op| {
                    op.output.as_ref().map(|output| {
                        self.materialized_vns
                            .contains_key(&MaterializedVarnodeKey::new(output, op))
                    })
                })
                .unwrap_or(false);
            if self.has_call_between_ops(block, def_idx + 1, block.ops.len()) {
                visiting.remove(&pred_idx);
                return Err("side_effect_after_pred_definition");
            }
            if self.has_call_between_ops(block, def_idx + 1, block.ops.len())
                && !has_materialized_def
            {
                visiting.remove(&pred_idx);
                return Err("side_effect_after_unmaterialized_pred_definition");
            }
            let result = self.partial_gpr_incoming_expr_for_pred_def(pred_idx, def_idx, requested);
            visiting.remove(&pred_idx);
            return result.ok_or("missing_materialized_pred_definition");
        }
        if block
            .ops
            .iter()
            .any(|op| self.op_defines_x86_gpr_family(op, family_idx))
        {
            visiting.remove(&pred_idx);
            return Err("unsafe_pred_definition");
        }
        if self.has_call_between_ops(block, 0, block.ops.len()) {
            visiting.remove(&pred_idx);
            return Err("side_effect_on_passthrough_path");
        }
        let incoming_preds = self
            .predecessors
            .get(pred_idx)
            .into_iter()
            .flatten()
            .copied()
            .filter(|idx| *idx != target_idx)
            .collect::<Vec<_>>();
        if incoming_preds.is_empty() {
            visiting.remove(&pred_idx);
            return Err("missing_incoming_predecessor");
        }
        let mut incoming_exprs = Vec::new();
        for incoming_pred in incoming_preds {
            incoming_exprs.push(self.partial_gpr_incoming_expr_from_pred_path(
                incoming_pred,
                target_idx,
                family_idx,
                requested,
                depth + 1,
                visiting,
            )?);
        }
        visiting.remove(&pred_idx);
        let Some(first) = incoming_exprs.first().cloned() else {
            return Err("missing_incoming_expr");
        };
        if incoming_exprs.iter().all(|expr| expr == &first) {
            Ok(first)
        } else {
            Err("ambiguous_incoming_expr")
        }
    }

    fn partial_gpr_incoming_expr_for_pred_def(
        &mut self,
        pred_idx: usize,
        def_idx: usize,
        requested: &Varnode,
    ) -> Option<HirExpr> {
        let block_addr = self.pcode.blocks.get(pred_idx)?.start_address;
        let op = self.pcode.blocks.get(pred_idx)?.ops.get(def_idx)?.clone();
        let output = op.output.as_ref()?;
        if !self.varnode_aliases_value(output, requested) {
            return None;
        }
        let expr = self
            .materialized_vns
            .get(&MaterializedVarnodeKey::new(output, &op))
            .map(|name| HirExpr::Var(name.clone()))
            .or_else(|| {
                self.with_lowering_site(
                    LoweringSite {
                        block_idx: pred_idx,
                        op_idx: def_idx,
                    },
                    |this| this.try_lower_materialized_output_rhs(block_addr, &op),
                )
                .ok()
                .flatten()
            })?;
        Some(self.project_partial_gpr_incoming_expr(output, requested, expr))
    }

    fn project_partial_gpr_incoming_expr(
        &self,
        output: &Varnode,
        requested: &Varnode,
        expr: HirExpr,
    ) -> HirExpr {
        if VarnodeKey::from(output) == VarnodeKey::from(requested) {
            return expr;
        }
        HirExpr::Cast {
            ty: type_from_size(requested.size, false),
            expr: Box::new(expr),
        }
    }

    fn current_explicit_merge_binding_expr(
        &self,
        block_idx: usize,
        input: &Varnode,
    ) -> Option<HirExpr> {
        let key = VarnodeKey::from(input);
        let binding = self
            .explicit_merge_bindings
            .get(&(block_idx, key.clone()))
            .map(|name| (key.clone(), name))
            .or_else(|| {
                self.explicit_merge_bindings.iter().find_map(
                    |((candidate_block_idx, candidate_key), name)| {
                        (*candidate_block_idx == block_idx
                            && (Self::register_key_covers(candidate_key, &key)
                                || self.register_key_zero_extends(candidate_key, &key)
                                || self.register_key_cross_space_covers(candidate_key, &key)
                                || self.register_key_cross_space_zero_extends(candidate_key, &key)))
                        .then_some((candidate_key.clone(), name))
                    },
                )
            })?;
        let (candidate_key, binding_name) = binding;
        let expr = HirExpr::Var(binding_name.clone());
        if candidate_key.size == key.size {
            Some(expr)
        } else {
            Some(HirExpr::Cast {
                ty: type_from_size(input.size, false),
                expr: Box::new(expr),
            })
        }
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
        if self.has_call_between_ops(block, 0, op_idx) {
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
            self.pred_path_has_live_accumulator_def(*pred, block_idx, loop_body, family_idx, false)
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
        if self.has_call_between_ops(block, 0, op_idx) {
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
        if !self
            .pred_path_has_live_accumulator_def(loop_pred, block_idx, &loop_body, family_idx, false)
        {
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
            false,
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
                | PcodeOpcode::Load
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
        self.current_stack_home_ptr = op.inputs.get(1).cloned();
        let res = self.stack_home_accumulator_store_rhs_inner(
            block, op, slot_name, value, &live_name, family_idx,
        );
        self.current_stack_home_ptr = None;
        res
    }

    fn stack_home_accumulator_store_rhs_inner(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op: &PcodeOp,
        slot_name: &str,
        value: &Varnode,
        live_name: &str,
        family_idx: usize,
    ) -> Option<HirExpr> {
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
                        *pred, block_idx, &loop_body, family_idx, true,
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
            && self.loop_header_external_predecessors_seed_zero(
                block_idx, &loop_body, family_idx, true,
            );
        let zero_entry_default = value_is_zero || external_zero_seed;
        let all_external_preds_have_live_def = store_is_loop_header
            && self.loop_header_external_predecessors_have_live_accumulator_def(
                block_idx, &loop_body, family_idx, true,
            );
        if !zero_entry_default && !all_external_preds_have_live_def {
            let external_preds = self
                .predecessors
                .get(block_idx)
                .into_iter()
                .flatten()
                .copied()
                .filter(|pred| !loop_body.body.contains(pred))
                .collect::<Vec<_>>();
            let reason = format!(
                "missing_entry_state:value_zero={} external_zero_seed={} external_live_def={} external_preds={:?}",
                value_is_zero, external_zero_seed, all_external_preds_have_live_def, external_preds
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
        conservative_mem_check: bool,
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
                    conservative_mem_check,
                )
            })
    }

    fn loop_header_external_predecessors_have_live_accumulator_def(
        &self,
        header_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        family_idx: usize,
        conservative_mem_check: bool,
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
                self.pred_path_has_external_live_accumulator_def(
                    pred,
                    header_idx,
                    &body,
                    family_idx,
                    0,
                    &mut visiting,
                    conservative_mem_check,
                )
            })
    }

    fn pred_path_has_external_live_accumulator_def(
        &self,
        idx: usize,
        header_idx: usize,
        loop_body: &HashSet<usize>,
        family_idx: usize,
        depth: usize,
        visiting: &mut HashSet<usize>,
        conservative_mem_check: bool,
    ) -> bool {
        if depth > 8 || idx == header_idx || loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            let has_side_effect =
                |block: &crate::pcode::PcodeBasicBlock, start: usize, end: usize| {
                    if conservative_mem_check {
                        self.has_aliasing_side_effect_between_ops(block, start, end)
                    } else {
                        self.has_call_between_ops(block, start, end)
                    }
                };
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return !has_side_effect(block, def_idx + 1, block.ops.len());
            }
            if block.ops.iter().any(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Store
                        | PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::CallOther
                )
            }) {
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
                    self.pred_path_has_external_live_accumulator_def(
                        pred,
                        header_idx,
                        loop_body,
                        family_idx,
                        depth + 1,
                        visiting,
                        conservative_mem_check,
                    )
                })
        });
        visiting.remove(&idx);
        result
    }

    fn pred_path_has_zero_accumulator_seed(
        &self,
        idx: usize,
        header_idx: usize,
        loop_body: &HashSet<usize>,
        family_idx: usize,
        depth: usize,
        visiting: &mut HashSet<usize>,
        conservative_mem_check: bool,
    ) -> bool {
        if depth > 8 || idx == header_idx || loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            let has_side_effect =
                |block: &crate::pcode::PcodeBasicBlock, start: usize, end: usize| {
                    if conservative_mem_check {
                        self.has_aliasing_side_effect_between_ops(block, start, end)
                    } else {
                        self.has_call_between_ops(block, start, end)
                    }
                };
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return self.x86_gpr_definition_is_zero_in_block(block, def_idx, 4)
                    && !has_side_effect(block, def_idx + 1, block.ops.len());
            }
            if has_side_effect(block, 0, block.ops.len()) {
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
                        conservative_mem_check,
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

    pub(super) fn canonical_x86_gpr64_name_for_value(
        &self,
        value: &Varnode,
    ) -> Option<(&'static str, usize)> {
        let raw_name = self.sla_hw_name(value.offset, value.size).or_else(|| {
            crate::arch::x86::unique_x86_register_name(value.offset, value.size).map(str::to_string)
        })?;
        Self::canonical_x86_gpr64_name_for_raw_name(raw_name.as_str())
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
        conservative_mem_check: bool,
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
            conservative_mem_check,
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
        conservative_mem_check: bool,
    ) -> bool {
        if depth > 8 || idx == target_idx || !loop_body.contains(&idx) || !visiting.insert(idx) {
            return false;
        }
        let result = self.pcode.blocks.get(idx).is_some_and(|block| {
            let has_side_effect =
                |block: &crate::pcode::PcodeBasicBlock, start: usize, end: usize| {
                    if conservative_mem_check {
                        self.has_aliasing_side_effect_between_ops(block, start, end)
                    } else {
                        self.has_call_between_ops(block, start, end)
                    }
                };
            if let Some(def_idx) = self.last_x86_gpr_family_definition(block, family_idx) {
                return !has_side_effect(block, def_idx + 1, block.ops.len());
            }
            if has_side_effect(block, 0, block.ops.len()) {
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
                        conservative_mem_check,
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
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            if matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store) {
                if let Some(ptr) = op.inputs.get(1) {
                    if let Some(sh_ptr) = &self.current_stack_home_ptr {
                        if self.memory_ops_may_alias(ptr, sh_ptr) {
                            return true;
                        }
                    } else {
                        return false;
                    }
                }
                false
            } else {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
            }
        })
    }

    fn memory_ops_may_alias(&self, ptr1: &Varnode, ptr2: &Varnode) -> bool {
        if VarnodeKey::from(ptr1) == VarnodeKey::from(ptr2) {
            return true;
        }
        let addr1 = self.resolve_stack_address(ptr1);
        let addr2 = self.resolve_stack_address(ptr2);
        match (addr1, addr2) {
            (Some((base1, offset1)), Some((base2, offset2))) => {
                base1 == base2 && offset1 == offset2
            }
            _ => false,
        }
    }

    fn has_call_between_ops(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        let res = block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                let mut target_name = None;
                if op.opcode == PcodeOpcode::Call {
                    if let Some(name) = self.options.relocation_names.get(&op.address) {
                        target_name = Some(name.as_str());
                    }
                }
                if target_name.is_none() {
                    if let Some(target_vn) = op.inputs.first() {
                        if target_vn.is_constant {
                            let addr = if target_vn.offset != 0 {
                                target_vn.offset
                            } else {
                                target_vn.constant_val as u64
                            };
                            if let Some(ctx) = self.type_context {
                                if let Some(target_ref) = ctx.call_target_refs.get(&addr) {
                                    target_name = Some(target_ref.symbol.as_str());
                                }
                            }
                        }
                    }
                }
                if let Some(name) = target_name {
                    if Self::materialize_call_target_is_known_pure_intrinsic(name) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        });
        res
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
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
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
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
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
#[path = "mod_tests.rs"]
mod materialize_tests;

pub(super) fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    self::incremental::refine_partitions(accesses)
}
