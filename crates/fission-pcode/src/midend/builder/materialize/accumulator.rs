use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn merge_binding_name_for_direct_successor_accumulator(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
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
        // A guarded write in an instruction-local forward-CBranch body is not
        // an unconditional predecessor value. The skipped path reaches the
        // successor with the prior register value, so assigning this def to a
        // successor merge binding would create a carrier that is initialized
        // only on the fall-through path. Leave CMOV bodies to the register
        // carrier materialization rules instead.
        if self.op_is_inside_same_block_forward_cmov_body(block, op_idx) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "instruction_local_conditional_definition",
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
            if let Some(name) = self.merge_binding_name_for_conditional_loop_exit_accumulator(
                block, op_idx, output, rhs,
            ) {
                return Some(name);
            }
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_single_successor",
            );
            return None;
        };
        let reach_proof =
            self.prove_definition_reaches_block_entry(block_idx, op_idx, output, succ_idx)?;
        debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
        debug_assert_eq!(reach_proof.target_block(), succ_idx);
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
        if predecessor_idxs.len() < 2 || !predecessor_idxs.contains(&block_idx) {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "not_multi_predecessor_join",
            );
            return None;
        }
        let succ_block = self.pcode.blocks.get(succ_idx)?;
        // A shared return block has its own edge-sensitive recovery: each
        // predecessor's live primary-return expression is lowered before the
        // common epilogue is emitted.  A direct merge carrier here would
        // collapse those edge values into one name and can let an earlier
        // call result outrank a later return-register definition on one arm.
        // Keep this accumulator proof for data joins and let return recovery
        // own primary-return joins.
        let successor_is_return_join = self.register_namer().is_primary_return_register(output)
            && self.block_returns_without_redefining_output(succ_block, output)
            && self.return_join_has_primary_return_evidence(succ_idx);
        if successor_is_return_join {
            self.trace_direct_successor_accumulator_merge_rejected(
                block.start_address,
                output,
                "return_join_owned_by_return_recovery",
            );
            return None;
        }
        let successor_reads_merge = self
            .block_reads_merge_input_before_redefinition(succ_block, output)
            || successor_is_return_join;
        if !successor_reads_merge {
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
            // A predecessor's last storage write may itself be the guarded
            // body of an instruction-local CMOV.  Although the opcode is a
            // safe scalar Copy, it is not the value that unconditionally
            // reaches the successor: the fall-through arm retains the
            // register value from before the guarded write.  Treating this
            // predecessor as a normal incoming edge would make another
            // predecessor synthesize a shared join binding from the wrong
            // value and leave the join carrier undefined on the skipped arm.
            if self.op_is_inside_same_block_forward_cmov_body(pred_block, def_idx) {
                self.trace_direct_successor_accumulator_merge_rejected(
                    block.start_address,
                    output,
                    "conditional_pred_definition",
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

    pub(super) fn merge_binding_name_for_conditional_loop_exit_accumulator(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
        rhs: &PreHirExpr,
    ) -> Option<String> {
        if !self.is_conditional_loop_exit_accumulator_shape_candidate(block, output)
            || !Self::rhs_is_safe_scalar_live_register_merge(rhs)
        {
            return None;
        }
        let Some((live_name, family_idx)) = self.canonical_x86_gpr64_name_for_value(output) else {
            return None;
        };
        let block_idx = self.lowering_block_index(block);
        let succs = self.successors.get(block_idx)?.clone();
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
        let reach_proof =
            self.prove_definition_reaches_block_entry(block_idx, op_idx, output, *read_succ_idx)?;
        debug_assert_eq!(reach_proof.definition_site(), (block_idx, op_idx));
        debug_assert_eq!(reach_proof.target_block(), *read_succ_idx);
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
            binding.initializer = Some(PreHirExpr::Const(0, type_from_size(output.size, false)));
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

    fn is_conditional_loop_exit_accumulator_shape_candidate(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> bool {
        // This proof is intentionally expression-free. Callers use it before
        // lowering an output's RHS so non-candidates cannot trigger expensive
        // recursive materialization during speculative join recovery.
        let is_32bit_return_reg = self.options.is_64bit
            && output.size == 4
            && self.register_namer().is_primary_return_register(output);
        if output.is_constant
            || !is_register_space_id(output.space_id)
            || (output.size != self.options.pointer_size && !is_32bit_return_reg)
            || !matches!(
                self.options.calling_convention,
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
            )
        {
            return false;
        }
        let Some((live_name, _)) = self.canonical_x86_gpr64_name_for_value(output) else {
            return false;
        };
        if live_name == "rsp" || self.abi_state().param_slot_for_name(live_name).is_some() {
            return false;
        }
        let block_idx = self.lowering_block_index(block);
        self.successors
            .get(block_idx)
            .is_some_and(|successors| successors.len() == 2)
    }

    pub(super) fn is_conditional_loop_exit_accumulator_site_candidate(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        output: &Varnode,
    ) -> bool {
        if !self.is_conditional_loop_exit_accumulator_shape_candidate(block, output)
            || self.last_redefinition_index_before_terminator(block, output) != Some(op_idx)
            || !Self::output_def_is_safe_direct_successor_merge(&block.ops[op_idx])
            || self.has_call_between_ops(block, op_idx + 1, block.ops.len())
        {
            return false;
        }

        let block_idx = self.lowering_block_index(block);
        let Some(successors) = self.successors.get(block_idx) else {
            return false;
        };
        let read_successors = successors
            .iter()
            .copied()
            .filter(|successor_idx| {
                self.pcode
                    .blocks
                    .get(*successor_idx)
                    .is_some_and(|successor| {
                        self.block_reads_merge_input_before_redefinition(successor, output)
                    })
            })
            .collect::<Vec<_>>();
        let [read_successor_idx] = read_successors.as_slice() else {
            return false;
        };
        let Some(non_read_successor_idx) = successors
            .iter()
            .copied()
            .find(|successor_idx| successor_idx != read_successor_idx)
        else {
            return false;
        };
        self.loop_bodies.iter().any(|loop_body| {
            loop_body.head == non_read_successor_idx
                && !loop_body.body.contains(read_successor_idx)
                && (loop_body.all_exits.contains(read_successor_idx)
                    || loop_body.exit_idx == Some(*read_successor_idx)
                    || successors.contains(read_successor_idx))
                && !self.loop_body_has_side_entry_or_irreducible_edge(loop_body)
                && (loop_body.body.contains(&block_idx) || successors.contains(&loop_body.head))
        })
    }

    fn current_site_matches_block_op(&self, block_idx: usize, op_idx: usize) -> bool {
        self.current_lowering_site
            .is_some_and(|site| site.block_idx == block_idx && site.op_idx == op_idx)
    }

    fn conditional_loop_exit_external_seed_shape(
        &self,
        block_idx: usize,
        read_succ_idx: usize,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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
}
