use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn rewrite_block_entry_accumulator_rhs_with_live_gpr(
        &mut self,
        block_addr: u64,
        op: &PcodeOp,
        rhs: PreHirExpr,
    ) -> PreHirExpr {
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
            PreHirExpr::Binary {
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
                PreHirExpr::Binary {
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
        expr: PreHirExpr,
    ) -> PreHirExpr {
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
        if matches!(&expr, PreHirExpr::Var(name) if name == live_name) {
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
        PreHirExpr::Var(live_name.to_string())
    }

    fn current_explicit_merge_binding_expr(
        &self,
        block_idx: usize,
        input: &Varnode,
    ) -> Option<PreHirExpr> {
        let key = VarnodeKey::from(input);
        let binding = self
            .explicit_merge_bindings
            .get(&(block_idx, key.clone()))
            .map(|name| (key.clone(), name))
            .or_else(|| {
                // `explicit_merge_bindings` is a HashMap: iteration order is
                // randomized per-process. When more than one covering/
                // zero-extending candidate exists for `block_idx`, an
                // unsorted `.find_map` pick made this method (and therefore
                // materialize order and synthetic temp naming) non-
                // deterministic across separate fission_cli runs on the same
                // binary. Collect all matches and pick the smallest key by a
                // stable, well-defined order instead of "whichever the
                // hasher visits first".
                let mut candidates: Vec<(&VarnodeKey, &String)> = self
                    .explicit_merge_bindings
                    .iter()
                    .filter_map(|((candidate_block_idx, candidate_key), name)| {
                        (*candidate_block_idx == block_idx
                            && (Self::register_key_covers(candidate_key, &key)
                                || self.register_key_zero_extends(candidate_key, &key)
                                || self.register_key_cross_space_covers(candidate_key, &key)
                                || self.register_key_cross_space_zero_extends(candidate_key, &key)))
                        .then_some((candidate_key, name))
                    })
                    .collect();
                candidates.sort_unstable_by_key(|(candidate_key, name)| {
                    (
                        candidate_key.space_id,
                        candidate_key.offset,
                        candidate_key.size,
                        candidate_key.is_constant,
                        candidate_key.constant_val,
                        (*name).clone(),
                    )
                });
                candidates
                    .into_iter()
                    .next()
                    .map(|(candidate_key, name)| (candidate_key.clone(), name))
            })?;
        let (candidate_key, binding_name) = binding;
        let expr = PreHirExpr::Var(binding_name.clone());
        if candidate_key.size == key.size {
            Some(expr)
        } else {
            Some(PreHirExpr::Cast {
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
        let mut visiting = HashSet::default();
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
        crate::midend::structuring::loop_analysis::LoopBody,
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
}
