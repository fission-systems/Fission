use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn loop_body_has_side_entry_or_irreducible_edge(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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

    pub(super) fn pred_path_has_live_accumulator_def(
        &self,
        pred_idx: usize,
        target_idx: usize,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        family_idx: usize,
        conservative_mem_check: bool,
    ) -> bool {
        let body = loop_body.body.iter().copied().collect::<HashSet<_>>();
        let mut visiting = HashSet::default();
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

    pub(super) fn loop_body_has_live_accumulator_def(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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

    pub(super) fn last_x86_gpr_family_definition(
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

    pub(super) fn x86_gpr_definition_is_zero_in_block(
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

    pub(super) fn has_aliasing_side_effect_between_ops(
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

    pub(super) fn has_call_between_ops(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        let res = block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                let mut target_name = None;
                if op.opcode == PcodeOpcode::Call {
                    if let Some(name) = self
                        .options
                        .relocation_names
                        .get(&op.address)
                        .filter(|name| !name.is_empty())
                    {
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

    pub(super) fn varnode_known_const_zero(&self, value: &Varnode, budget: usize) -> bool {
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
}
