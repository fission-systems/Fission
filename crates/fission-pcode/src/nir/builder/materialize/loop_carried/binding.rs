use super::*;
use std::collections::BTreeSet;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn prior_materialized_loop_carried_output_name(&self, output: &Varnode) -> Option<String> {
        let (site, op) = self.lookup_def_site(output)?;
        if Some(site) == self.current_lowering_site {
            return None;
        }
        let prior_output = op.output.as_ref()?;
        if VarnodeKey::from(prior_output) == VarnodeKey::from(output) {
            return self.materialized_vns
                .get(&MaterializedVarnodeKey::new(prior_output, op))
                .cloned();
        }
        if !Self::prior_output_aliases_loop_carried_update(prior_output, output) {
            return None;
        }
        // Reject wide→narrow reuse when the wide binding was created inside the
        // same loop body. This prevents a temporary RCX binding (e.g. from IntZExt)
        // from hijacking a later ECX loop-carried update in the same block.
        if self
            .loop_bodies
            .iter()
            .any(|lb| lb.body.contains(&site.block_idx))
        {
            return None;
        }
        // When the prior op is a passthrough (ZExt/SExt/Copy/Cast) whose input is
        // exactly the narrow register we are looking for, look through the passthrough
        // to the real narrow-register definition earlier in the same block.
        //
        // Example: in x86-64, an init block often contains
        //   EAX = XOR(EAX, EAX)   ; seq N   (materialized as "rax")
        //   RAX = IntZExt(EAX)     ; seq N+1 (materialized as some wider temp)
        //
        // `lookup_def_site` picks RAX(seq N+1) over EAX(seq N) because op_idx N+1 > N.
        // Without the look-through, the loop-carried EAX accumulator would inherit the
        // wider temp's name instead of "rax", causing the return to capture the pre-loop
        // snapshot value rather than the accumulated result.
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
            || !Self::prior_output_aliases_loop_carried_update(prior_output, output)
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
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64
        ) {
            return None;
        }
        let is_param = self.abi_state().param_slot_for_varnode(output).is_some();
        let is_return_reg = is_register_space_id(output.space_id) && output.offset == 0x00;
        if !is_param && !is_return_reg {
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
                let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
                    complete = false;
                    break;
                };
                let term_idx = self
                    .block_terminator_index(pred_block)
                    .unwrap_or(pred_block.ops.len());
                let Some((def_idx, pred_op)) =
                    self.last_register_redefinition_before(pred_block, term_idx, output)
                else {
                    complete = false;
                    break;
                };
                let Some(pred_output) = pred_op.output.as_ref() else {
                    complete = false;
                    break;
                };
                if !Self::output_def_is_safe_direct_successor_merge(pred_op)
                    || self.has_call_between_ops(pred_block, def_idx + 1, term_idx)
                {
                    complete = false;
                    break;
                }
                let pred_key = VarnodeKey::from(pred_output);
                if !Self::varnode_key_may_alias_output(&pred_key, &output_key)
                    && !Self::prior_output_aliases_loop_carried_update(pred_output, output)
                {
                    complete = false;
                    break;
                }
                let Some(name) = self
                    .materialized_vns
                    .get(&MaterializedVarnodeKey::new(pred_output, pred_op))
                    .filter(|name| !name.starts_with("param_"))
                else {
                    complete = false;
                    break;
                };
                names.insert(name.clone());
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
}
