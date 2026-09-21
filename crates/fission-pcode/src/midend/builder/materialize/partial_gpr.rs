use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn block_entry_partial_gpr_incoming_expr(
        &mut self,
        block_idx: usize,
        op_idx: usize,
        op_seq: u32,
        input: &Varnode,
    ) -> Option<PreHirExpr> {
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
            let mut visiting = HashSet::default();
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
                PreHirExpr::Var(live_name.to_string()),
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

    pub(super) fn live_register_lhs_name_for_partial_gpr_join_family(
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
    ) -> Result<PreHirExpr, &'static str> {
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
    ) -> Option<PreHirExpr> {
        let block_addr = self.pcode.blocks.get(pred_idx)?.start_address;
        let op = self.pcode.blocks.get(pred_idx)?.ops.get(def_idx)?.clone();
        let output = op.output.as_ref()?;
        if !self.varnode_aliases_value(output, requested) {
            return None;
        }
        let expr = self
            .materialized_vns
            .get(&MaterializedVarnodeKey::new(output, &op))
            .map(|name| PreHirExpr::Var(name.clone()))
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
        expr: PreHirExpr,
    ) -> PreHirExpr {
        if VarnodeKey::from(output) == VarnodeKey::from(requested) {
            return expr;
        }
        PreHirExpr::Cast {
            ty: type_from_size(requested.size, false),
            expr: Box::new(expr),
        }
    }
}
