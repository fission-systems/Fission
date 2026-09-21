use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn stack_home_accumulator_store_rhs(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        _op_idx: usize,
        op: &PcodeOp,
        slot_name: &str,
        value: &Varnode,
    ) -> Option<PreHirExpr> {
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
    ) -> Option<PreHirExpr> {
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

        self.trace_stack_home_accumulator_store_merge_accepted(
            block.start_address,
            op.seq_num,
            slot_name,
            value,
            live_name,
        );

        self.ensure_live_register_binding(live_name, self.options.pointer_size);
        if let Some(binding) = self.temps.get_mut(live_name)
            && binding.initializer.is_none()
        {
            binding.initializer = Some(PreHirExpr::Const(
                0,
                type_from_size(self.options.pointer_size, false),
            ));
        }
        Some(PreHirExpr::Var(live_name.to_string()))
    }

    pub(super) fn loop_header_external_predecessors_seed_zero(
        &self,
        header_idx: usize,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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
                let mut visiting = HashSet::default();
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
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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
                let mut visiting = HashSet::default();
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

    pub(super) fn pred_path_has_zero_accumulator_seed(
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
    ) -> Option<(crate::midend::structuring::loop_analysis::LoopBody, bool)> {
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
}
