use super::*;
use std::collections::BTreeSet;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn prior_materialized_loop_carried_output_name(
        &self,
        output: &Varnode,
    ) -> Option<String> {
        let (site, op) = self.lookup_def_site(output)?;
        if Some(site) == self.current_lowering_site {
            return None;
        }
        let prior_output = op.output.as_ref()?;
        if VarnodeKey::from(prior_output) == VarnodeKey::from(output) {
            if let Some(name) = self
                .materialized_vns
                .get(&MaterializedVarnodeKey::new(prior_output, op))
                .cloned()
            {
                return Some(name);
            }
        }
        if !self.prior_output_aliases_loop_carried_update(prior_output, output) {
            return None;
        }
        // When the prior op is a passthrough (ZExt/SExt/Copy/Cast) whose input is
        // exactly the narrow register we are looking for, look through the passthrough
        // to the real narrow-register definition earlier in the same block.
        //
        // This check is intentionally placed BEFORE the loop-body guard below.
        // In x86-64 loop bodies, the accumulator pattern is:
        //   EAX = IntAdd(EAX, tmp)   ; seq N   ← actual narrow accumulator update
        //   RAX = IntZExt(EAX)       ; seq N+1 ← zero-extend side-effect
        //
        // `lookup_def_site(EAX)` picks RAX(seq N+1) because op_idx N+1 > N.
        // The wide def site is inside the loop body, so the guard below would return
        // None — but we can safely look through the passthrough to find the real
        // narrow definition and return its materialized name ("rax").
        if matches!(
            op.opcode,
            PcodeOpcode::IntZExt | PcodeOpcode::IntSExt | PcodeOpcode::Copy | PcodeOpcode::Cast
        ) {
            if let Some(input_vn) = op.inputs.first() {
                if !input_vn.is_constant
                    && input_vn.space_id == output.space_id
                    && input_vn.offset == output.offset
                    && input_vn.size == output.size
                {
                    if let Some(block) = self.pcode.blocks.get(site.block_idx) {
                        for prior_op_idx in (0..site.op_idx).rev() {
                            let prior_op2 = &block.ops[prior_op_idx];
                            if let Some(prior_output2) = &prior_op2.output {
                                if prior_output2.space_id == output.space_id
                                    && prior_output2.offset == output.offset
                                    && prior_output2.size == output.size
                                {
                                    // Found the narrow-register definition. Return its
                                    // materialized name if available, or None to prevent
                                    // the wide passthrough from hijacking this slot.
                                    return self
                                        .materialized_vns
                                        .get(&MaterializedVarnodeKey::new(prior_output2, prior_op2))
                                        .cloned();
                                }
                            }
                        }
                    }
                }
            }
        }
        // Reject wide→narrow reuse when the wide binding was created inside the
        // same loop body. This prevents a temporary RCX binding (e.g. from IntZExt
        // on a non-accumulator register) from hijacking a later ECX loop-carried
        // update in the same block. Passthrough ops are already handled above.
        if self
            .loop_bodies
            .iter()
            .any(|lb| lb.body.contains(&site.block_idx) && site.block_idx != lb.head)
        {
            return None;
        }
        self.materialized_vns
            .get(&MaterializedVarnodeKey::new(prior_output, op))
            .cloned()
    }

    pub(in crate::nir::builder) fn prior_materialized_same_register_output_name(
        &self,
        output: &Varnode,
    ) -> Option<String> {
        let output_key = VarnodeKey::from(output);
        let mut names = BTreeSet::new();
        for (key, name) in &self.materialized_vns {
            if !Self::varnode_key_may_alias_output(&key.varnode, &output_key)
                || key.varnode.size != output_key.size
                || name.starts_with("param_")
            {
                continue;
            }
            names.insert(name.clone());
        }
        if names.len() == 1 {
            names.into_iter().next()
        } else {
            None
        }
    }

    pub(in crate::nir::builder) fn prior_materialized_local_wide_alias_name(
        &self,
        block_idx: usize,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        let (site, op) = self.lookup_def_site(output)?;
        if site.block_idx != block_idx || site.op_idx >= op_idx {
            return None;
        }
        let prior_output = op.output.as_ref()?;
        if prior_output.size <= output.size
            || !self.prior_output_aliases_loop_carried_update(prior_output, output)
        {
            return None;
        }
        if matches!(
            op.opcode,
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt
        ) {
            return None;
        }
        self.materialized_vns
            .get(&MaterializedVarnodeKey::new(prior_output, op))
            .filter(|name| !name.starts_with("param_"))
            .cloned()
    }

    pub(in crate::nir::builder) fn loop_header_external_seed_binding_name_for_update(
        &self,
        block_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        if !matches!(
            self.options.calling_convention,
            CallingConvention::WindowsX64
                | CallingConvention::SystemVAmd64
                | CallingConvention::X86_32
        ) {
            return None;
        }
        let is_param = self.abi_state().param_slot_for_varnode(output).is_some();
        // x86-32 return / accumulator register is EAX (offset 0); same encoding as RAX.
        let is_return_reg = is_register_space_id(output.space_id) && output.offset == 0x00;
        let is_gpr = Self::is_loop_carried_register_update_candidate(output);
        if !is_param && !is_return_reg && !is_gpr {
            return None;
        }
        let output_key = VarnodeKey::from(output);
        let mut loop_candidates = BTreeSet::new();
        for loop_body in &self.loop_bodies {
            if !loop_body.body.contains(&block_idx)
                || !self.loop_update_reaches_backedge_tail(block_idx, loop_body)
                || self.loop_body_has_side_entry_or_irreducible_edge(loop_body)
            {
                continue;
            }
            let Some(header_preds) = self.predecessors.get(loop_body.head) else {
                continue;
            };
            let external_preds = header_preds
                .iter()
                .copied()
                .filter(|pred_idx| !loop_body.body.contains(pred_idx))
                .collect::<Vec<_>>();
            if external_preds.is_empty() {
                continue;
            }

            let mut names = BTreeSet::new();
            let mut complete = true;
            for pred_idx in external_preds {
                let mut visiting = HashSet::new();
                if !self.collect_external_seed_names_on_path(
                    pred_idx,
                    block_idx,
                    output,
                    &output_key,
                    0,
                    &mut visiting,
                    &mut names,
                ) {
                    complete = false;
                    break;
                }
            }
            if complete && names.len() == 1 {
                loop_candidates.extend(names);
            }
        }
        if loop_candidates.len() == 1 {
            loop_candidates.into_iter().next()
        } else {
            None
        }
    }

    fn collect_external_seed_names_on_path(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        output: &Varnode,
        output_key: &VarnodeKey,
        depth: usize,
        visiting: &mut HashSet<usize>,
        names: &mut BTreeSet<String>,
    ) -> bool {
        if depth > 8 || pred_idx == succ_idx || !visiting.insert(pred_idx) {
            return false;
        }

        let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
            visiting.remove(&pred_idx);
            return false;
        };

        let term_idx = self
            .block_terminator_index(pred_block)
            .unwrap_or(pred_block.ops.len());

        if let Some((def_idx, pred_op)) =
            self.last_register_redefinition_before(pred_block, term_idx, output)
        {
            if let Some(pred_output) = pred_op.output.as_ref() {
                if !Self::output_def_is_safe_direct_successor_merge(pred_op)
                    || self.has_call_between_ops(pred_block, def_idx + 1, term_idx)
                {
                    visiting.remove(&pred_idx);
                    return false;
                }

                let pred_key = VarnodeKey::from(pred_output);
                if !Self::varnode_key_may_alias_output(&pred_key, output_key)
                    && !self.prior_output_aliases_loop_carried_update(pred_output, output)
                {
                    visiting.remove(&pred_idx);
                    return false;
                }

                if let Some(name) = self
                    .materialized_vns
                    .get(&MaterializedVarnodeKey::new(pred_output, pred_op))
                    .filter(|name| !name.starts_with("param_"))
                {
                    names.insert(name.clone());
                    visiting.remove(&pred_idx);
                    return true;
                }
            }
            visiting.remove(&pred_idx);
            return false;
        }

        // When tracing register value provenance (e.g. RAX) across predecessor
        // blocks, only Call instructions can clobber registers in an
        // unanalysable way. Memory Store/Load operations do NOT overwrite
        // register values, so they must not act as a barrier here.
        if self.has_call_between_ops(pred_block, 0, term_idx) {
            visiting.remove(&pred_idx);
            return false;
        }

        let incoming = self
            .predecessors
            .get(pred_idx)
            .into_iter()
            .flatten()
            .copied()
            .filter(|incoming_idx| *incoming_idx != succ_idx)
            .collect::<Vec<_>>();

        if incoming.is_empty() {
            visiting.remove(&pred_idx);
            return false;
        }

        for incoming_idx in incoming {
            if !self.collect_external_seed_names_on_path(
                incoming_idx,
                pred_idx,
                output,
                output_key,
                depth + 1,
                visiting,
                names,
            ) {
                visiting.remove(&pred_idx);
                return false;
            }
        }

        visiting.remove(&pred_idx);
        true
    }

    pub(in crate::nir::builder) fn prior_materialized_loop_carried_input_name(
        &self,
        op: &PcodeOp,
        output: &Varnode,
    ) -> Option<String> {
        let output_key = VarnodeKey::from(output);
        for input in &op.inputs {
            if input.is_constant || !is_register_space_id(input.space_id) {
                continue;
            }
            let input_key = VarnodeKey::from(input);
            if Self::varnode_key_may_alias_output(&input_key, &output_key) {
                if let Some(name) = self.prior_materialized_loop_carried_output_name(input) {
                    return Some(name);
                }
            }
        }
        None
    }

    pub(in crate::nir::builder) fn loop_header_explicit_merge_binding_name(
        &self,
        block_idx: usize,
        output: &Varnode,
    ) -> Option<String> {
        let loop_head = self
            .loop_bodies
            .iter()
            .filter(|body| body.body.contains(&block_idx))
            .min_by_key(|body| body.body.len())
            .map(|body| body.head)?;
        let key = VarnodeKey::from(output);
        if let Some(name) = self.explicit_merge_bindings.get(&(loop_head, key.clone())) {
            return Some(name.clone());
        }
        self.explicit_merge_bindings
            .iter()
            .find(|((b_idx, candidate_key), _)| {
                *b_idx == loop_head
                    && (Self::register_key_covers(candidate_key, &key)
                        || self.register_key_zero_extends(candidate_key, &key)
                        || self.register_key_cross_space_covers(candidate_key, &key)
                        || self.register_key_cross_space_zero_extends(candidate_key, &key))
            })
            .map(|(_, name)| name.clone())
    }
}
